//! Channel senders. Each takes the kind-specific config map from the DTO;
//! missing required keys are hard errors surfaced to the caller/test button.

use anyhow::{Context, Result, anyhow};
use contracts::{NotificationChannelDto, NotificationChannelKind};

use crate::Notification;

/// A delivery failure plus whether retrying it could plausibly succeed.
///
/// Transport failures (DNS, connect, TLS, timeout) are retryable — a grid
/// outage takes the router down with it, so the very alert the user most
/// wants is the one that can't leave the house on the first try. Config and
/// 4xx failures (bad token, missing key) are permanent: retrying only burns
/// requests.
#[derive(Debug)]
pub struct SendError {
    pub error: anyhow::Error,
    pub retryable: bool,
}

impl std::fmt::Display for SendError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // `{:#}` renders the whole context chain, not just the outermost layer
        write!(f, "{:#}", self.error)
    }
}

impl From<anyhow::Error> for SendError {
    /// Config/validation errors bubble up through `?` — never retryable.
    fn from(error: anyhow::Error) -> Self {
        Self {
            error,
            retryable: false,
        }
    }
}

type SendResult = std::result::Result<(), SendError>;

/// Map a reqwest failure: anything that never reached the server is retryable.
fn transport_err(context: &'static str, e: reqwest::Error) -> SendError {
    let retryable = e.is_timeout() || e.is_connect() || e.is_request();
    SendError {
        error: anyhow::Error::new(e).context(context),
        retryable,
    }
}

/// Map a non-2xx response: 408/429 and 5xx are worth another attempt.
fn status_err(service: &str, status: reqwest::StatusCode) -> SendError {
    let retryable = status.is_server_error()
        || status == reqwest::StatusCode::REQUEST_TIMEOUT
        || status == reqwest::StatusCode::TOO_MANY_REQUESTS;
    SendError {
        error: anyhow!("{} returned {}", service, status),
        retryable,
    }
}

pub async fn send(
    http: &reqwest::Client,
    channel: &NotificationChannelDto,
    note: &Notification,
) -> SendResult {
    match channel.kind {
        NotificationChannelKind::Ntfy => ntfy(http, channel, note).await,
        NotificationChannelKind::Pushover => pushover(http, channel, note).await,
        NotificationChannelKind::Webhook => webhook(http, channel, note).await,
        NotificationChannelKind::Email => email(channel, note).await,
    }
}

fn cfg<'a>(channel: &'a NotificationChannelDto, key: &str) -> Result<&'a str> {
    channel
        .config
        .get(key)
        .map(|s| s.as_str())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow!("channel '{}' is missing '{}'", channel.name, key))
}

async fn ntfy(
    http: &reqwest::Client,
    channel: &NotificationChannelDto,
    note: &Notification,
) -> SendResult {
    let server = channel
        .config
        .get("serverUrl")
        .filter(|s| !s.is_empty())
        .map(|s| s.trim_end_matches('/'))
        .unwrap_or("https://ntfy.sh");
    let topic = cfg(channel, "topic")?;
    let mut req = http
        .post(format!("{}/{}", server, topic))
        .header("Title", note.title.clone())
        .body(note.body.clone());
    if let Some(token) = channel.config.get("token").filter(|s| !s.is_empty()) {
        req = req.bearer_auth(token);
    }
    let resp = req
        .send()
        .await
        .map_err(|e| transport_err("ntfy request failed", e))?;
    if !resp.status().is_success() {
        return Err(status_err("ntfy", resp.status()));
    }
    Ok(())
}

async fn pushover(
    http: &reqwest::Client,
    channel: &NotificationChannelDto,
    note: &Notification,
) -> SendResult {
    let resp = http
        .post("https://api.pushover.net/1/messages.json")
        .form(&[
            ("token", cfg(channel, "appToken")?),
            ("user", cfg(channel, "userKey")?),
            ("title", &note.title),
            ("message", &note.body),
        ])
        .send()
        .await
        .map_err(|e| transport_err("pushover request failed", e))?;
    if !resp.status().is_success() {
        return Err(status_err("pushover", resp.status()));
    }
    Ok(())
}

async fn webhook(
    http: &reqwest::Client,
    channel: &NotificationChannelDto,
    note: &Notification,
) -> SendResult {
    let resp = http
        .post(cfg(channel, "url")?)
        .json(&serde_json::json!({
            "title": note.title,
            "message": note.body,
            "source": "solar-monitor",
            "timestamp": chrono::Utc::now().to_rfc3339(),
        }))
        .send()
        .await
        .map_err(|e| transport_err("webhook request failed", e))?;
    if !resp.status().is_success() {
        return Err(status_err("webhook", resp.status()));
    }
    Ok(())
}

async fn email(channel: &NotificationChannelDto, note: &Notification) -> SendResult {
    use lettre::message::header::ContentType;
    use lettre::transport::smtp::authentication::Credentials;
    use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};

    let host = cfg(channel, "smtpHost")?;
    let port: u16 = channel
        .config
        .get("smtpPort")
        .and_then(|s| s.parse().ok())
        .unwrap_or(587);
    let message = Message::builder()
        .from(
            cfg(channel, "from")?
                .parse()
                .context("bad 'from' address")?,
        )
        .to(cfg(channel, "to")?.parse().context("bad 'to' address")?)
        .subject(note.title.clone())
        .header(ContentType::TEXT_PLAIN)
        .body(note.body.clone())
        .context("building the message failed")
        .map_err(SendError::from)?;

    // STARTTLS on 587 (the common case); implicit TLS on 465
    let mut builder = if port == 465 {
        AsyncSmtpTransport::<Tokio1Executor>::relay(host)
    } else {
        AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(host)
    }
    .context("building the smtp transport failed")
    .map_err(SendError::from)?;
    builder = builder.port(port);
    if let (Ok(user), Ok(pass)) = (cfg(channel, "username"), cfg(channel, "password")) {
        builder = builder.credentials(Credentials::new(user.to_string(), pass.to_string()));
    }
    builder.build().send(message).await.map_err(|e| SendError {
        // permanent = the server rejected it outright (bad auth, bad
        // recipient); everything else is a transient/connection failure
        retryable: !e.is_permanent(),
        error: anyhow::Error::new(e).context("smtp send failed"),
    })?;
    Ok(())
}
