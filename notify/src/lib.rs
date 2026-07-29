//! Notification engine: watches the live `DeviceData` stream, detects
//! edge-triggered events (with hysteresis and per-transition cooldown), and
//! fans alerts out to configured channels (ntfy, email, Pushover, webhook).

mod channels;
mod events;

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Result;
use contracts::{DeviceData, NotificationChannelDto, NotificationRuleDto};
use solar_monitor_storage::DataStore;
use tokio::sync::{Mutex, RwLock, broadcast};

use events::{Transition, evaluate};

#[derive(Clone)]
pub struct Notification {
    pub title: String,
    pub body: String,
}

/// Backoff for retryable delivery failures (~38 min of coverage). A grid
/// outage takes the router down with it, so the alert about it is exactly the
/// one that can't get out on the first try — keep trying until the network is
/// back rather than dropping the most important notification on the floor.
const RETRY_DELAYS: [Duration; 7] = [
    Duration::from_secs(30),
    Duration::from_secs(60),
    Duration::from_secs(120),
    Duration::from_secs(300),
    Duration::from_secs(600),
    Duration::from_secs(600),
    Duration::from_secs(600),
];

struct RuleState {
    /// Last observed condition (None until first sample: baseline silently)
    active: Option<bool>,
    /// Last firing per transition direction (false=cleared, true=triggered)
    last_fired: HashMap<bool, Instant>,
}

pub struct Notifier {
    store: Arc<DataStore>,
    config: RwLock<(Vec<NotificationChannelDto>, Vec<NotificationRuleDto>)>,
    /// Keyed by (rule id, device id)
    states: Mutex<HashMap<(String, String), RuleState>>,
    /// Device id -> (last data arrival, device considered offline)
    last_seen: Mutex<HashMap<String, (Instant, bool)>>,
    http: reqwest::Client,
}

impl Notifier {
    pub async fn new(store: Arc<DataStore>) -> Result<Arc<Self>> {
        let channels = store.list_notification_channels().await?;
        let rules = store.list_notification_rules().await?;
        Ok(Arc::new(Self {
            store,
            config: RwLock::new((channels, rules)),
            states: Mutex::new(HashMap::new()),
            last_seen: Mutex::new(HashMap::new()),
            http: reqwest::Client::builder()
                .timeout(Duration::from_secs(15))
                .build()
                .expect("http client"),
        }))
    }

    /// Re-read channels/rules from storage (called after config mutations).
    pub async fn reload(&self) -> Result<()> {
        let channels = self.store.list_notification_channels().await?;
        let rules = self.store.list_notification_rules().await?;
        *self.config.write().await = (channels, rules);
        Ok(())
    }

    /// Send a test notification through one channel (not necessarily saved).
    pub async fn send_test(&self, channel: &NotificationChannelDto) -> Result<()> {
        channels::send(
            &self.http,
            channel,
            &Notification {
                title: "Solar Monitor test".into(),
                body: "Test notification — the channel is configured correctly.".into(),
            },
        )
        .await
        .map_err(|e| e.error)
    }

    /// Spawn the engine: consumes live data and runs the offline ticker.
    pub fn spawn(self: Arc<Self>, mut rx: broadcast::Receiver<DeviceData>) {
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(Duration::from_secs(15));
            tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            loop {
                tokio::select! {
                    msg = rx.recv() => match msg {
                        Ok(data) => self.on_data(&data).await,
                        Err(broadcast::error::RecvError::Lagged(_)) => continue,
                        Err(broadcast::error::RecvError::Closed) => break,
                    },
                    _ = tick.tick() => self.check_offline().await,
                }
            }
        });
    }

    async fn on_data(self: &Arc<Self>, data: &DeviceData) {
        // Offline bookkeeping: arrival marks the device online again
        let came_back = {
            let mut seen = self.last_seen.lock().await;
            let was_offline = seen
                .get(&data.device_id)
                .map(|(_, off)| *off)
                .unwrap_or(false);
            seen.insert(data.device_id.clone(), (Instant::now(), false));
            was_offline
        };

        let (channels, rules) = &*self.config.read().await;
        for rule in rules.iter().filter(|r| r.enabled) {
            if let Some(dev) = &rule.device_id
                && dev != &data.device_id
            {
                continue;
            }
            let outcome = match rule.event {
                contracts::NotificationEvent::DeviceOffline => {
                    if came_back {
                        Some(Transition {
                            triggered: false,
                            title: format!("Device online — {}", data.device_id),
                            body: format!("{} is reporting data again.", data.device_id),
                        })
                    } else {
                        None
                    }
                }
                _ => {
                    let condition = evaluate(rule, data);
                    match condition {
                        Some((active, transition)) => {
                            self.edge(rule, &data.device_id, active, transition).await
                        }
                        None => None,
                    }
                }
            };
            if let Some(t) = outcome
                && self.cooldown_ok(rule, &data.device_id, t.triggered).await
            {
                self.dispatch(rule, channels, &t, Some(&data.device_id))
                    .await;
            }
        }
    }

    /// Track condition state per (rule, device); return the transition to fire
    /// on an edge. The first observation only sets the baseline.
    async fn edge(
        &self,
        rule: &NotificationRuleDto,
        device_id: &str,
        active: bool,
        transition: Option<Transition>,
    ) -> Option<Transition> {
        let key = (rule.id.clone(), device_id.to_string());
        let mut states = self.states.lock().await;
        let st = states.entry(key).or_insert(RuleState {
            active: None,
            last_fired: HashMap::new(),
        });
        let fire = match st.active {
            None => false, // baseline on first sample, don't alert on startup
            Some(prev) => prev != active,
        };
        st.active = Some(active);
        if fire { transition } else { None }
    }

    async fn cooldown_ok(&self, rule: &NotificationRuleDto, device_id: &str, dir: bool) -> bool {
        let key = (rule.id.clone(), device_id.to_string());
        let mut states = self.states.lock().await;
        let st = states.entry(key).or_insert(RuleState {
            active: None,
            last_fired: HashMap::new(),
        });
        let cool = Duration::from_secs(rule.cooldown_seconds as u64);
        if let Some(prev) = st.last_fired.get(&dir)
            && prev.elapsed() < cool
        {
            return false;
        }
        st.last_fired.insert(dir, Instant::now());
        true
    }

    async fn check_offline(self: &Arc<Self>) {
        let (channels, rules) = &*self.config.read().await;
        let offline_rules: Vec<&NotificationRuleDto> = rules
            .iter()
            .filter(|r| r.enabled && r.event == contracts::NotificationEvent::DeviceOffline)
            .collect();
        if offline_rules.is_empty() {
            return;
        }
        let mut newly_offline: Vec<String> = Vec::new();
        {
            let mut seen = self.last_seen.lock().await;
            for (device_id, (at, offline)) in seen.iter_mut() {
                if *offline {
                    continue;
                }
                // Use the strictest applicable threshold
                let threshold = offline_rules
                    .iter()
                    .filter(|r| r.device_id.as_deref().is_none_or(|d| d == device_id))
                    .map(|r| events::param(r, "offlineAfterSeconds", 120.0))
                    .fold(f64::INFINITY, f64::min);
                if threshold.is_finite() && at.elapsed() > Duration::from_secs_f64(threshold) {
                    *offline = true;
                    newly_offline.push(device_id.clone());
                }
            }
        }
        for device_id in newly_offline {
            for rule in &offline_rules {
                if rule.device_id.as_deref().is_none_or(|d| d == device_id) {
                    let t = Transition {
                        triggered: true,
                        title: format!("Device offline — {}", device_id),
                        body: format!(
                            "No data from {} for over {:.0} seconds.",
                            device_id,
                            events::param(rule, "offlineAfterSeconds", 120.0)
                        ),
                    };
                    if self.cooldown_ok(rule, &device_id, true).await {
                        self.dispatch(rule, channels, &t, Some(&device_id)).await;
                    }
                }
            }
        }
    }

    async fn dispatch(
        self: &Arc<Self>,
        rule: &NotificationRuleDto,
        channels: &[NotificationChannelDto],
        t: &Transition,
        device_id: Option<&str>,
    ) {
        let note = Notification {
            title: t.title.clone(),
            body: t.body.clone(),
        };
        for ch in channels
            .iter()
            .filter(|c| c.enabled && rule.channel_ids.contains(&c.id))
        {
            let result = channels::send(&self.http, ch, &note).await;
            let retryable = match &result {
                Err(e) => {
                    tracing::warn!(
                        "notification '{}' via channel '{}' failed: {}{}",
                        rule.name,
                        ch.name,
                        e,
                        if e.retryable { " (will retry)" } else { "" }
                    );
                    e.retryable
                }
                Ok(()) => {
                    tracing::info!("notification '{}' sent via '{}'", rule.name, ch.name);
                    false
                }
            };
            let entry = contracts::NotificationLogEntryDto {
                id: 0, // assigned by the database
                timestamp: chrono::Utc::now(),
                rule_id: rule.id.clone(),
                rule_name: rule.name.clone(),
                device_id: device_id.map(|s| s.to_string()),
                title: note.title.clone(),
                body: note.body.clone(),
                channel_id: ch.id.clone(),
                channel_name: ch.name.clone(),
                ok: result.is_ok(),
                error: result.err().map(|e| e.to_string()),
            };
            let log_id = match self.store.append_notification_log(&entry).await {
                Ok(id) => Some(id),
                Err(e) => {
                    tracing::warn!("failed to record notification log entry: {}", e);
                    None
                }
            };
            if retryable {
                Arc::clone(self).spawn_retry(ch.clone(), note.clone(), rule.name.clone(), log_id);
            }
        }
    }

    /// Keep re-sending a delivery that failed for a transient reason, amending
    /// its log entry with the eventual outcome.
    fn spawn_retry(
        self: Arc<Self>,
        channel: NotificationChannelDto,
        note: Notification,
        rule_name: String,
        log_id: Option<i64>,
    ) {
        tokio::spawn(async move {
            let mut last_error = String::new();
            for (i, delay) in RETRY_DELAYS.iter().enumerate() {
                tokio::time::sleep(*delay).await;
                let attempt = i + 2; // the inline send was attempt 1
                match channels::send(&self.http, &channel, &note).await {
                    Ok(()) => {
                        tracing::info!(
                            "notification '{}' delivered via '{}' on attempt {}",
                            rule_name,
                            channel.name,
                            attempt
                        );
                        self.amend_log(
                            log_id,
                            true,
                            Some(&format!("delivered on attempt {}", attempt)),
                        )
                        .await;
                        return;
                    }
                    Err(e) => {
                        last_error = e.to_string();
                        tracing::warn!(
                            "notification '{}' via channel '{}' attempt {} failed: {}",
                            rule_name,
                            channel.name,
                            attempt,
                            last_error
                        );
                        if !e.retryable {
                            break;
                        }
                    }
                }
            }
            self.amend_log(
                log_id,
                false,
                Some(&format!("{} (gave up after retrying)", last_error)),
            )
            .await;
        });
    }

    async fn amend_log(&self, log_id: Option<i64>, ok: bool, error: Option<&str>) {
        let Some(id) = log_id else { return };
        if let Err(e) = self
            .store
            .update_notification_log_result(id, ok, error)
            .await
        {
            tracing::warn!("failed to update notification log entry: {}", e);
        }
    }
}
