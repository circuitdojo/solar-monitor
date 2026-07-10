//! Channel senders. Each takes the kind-specific config map from the DTO;
//! missing required keys are hard errors surfaced to the caller/test button.

use anyhow::{anyhow, Context, Result};
use contracts::{NotificationChannelDto, NotificationChannelKind};

use crate::Notification;

pub async fn send(
    http: &reqwest::Client,
    channel: &NotificationChannelDto,
    note: &Notification,
) -> Result<()> {
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
) -> Result<()> {
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
    let resp = req.send().await.context("ntfy request failed")?;
    if !resp.status().is_success() {
        return Err(anyhow!("ntfy returned {}", resp.status()));
    }
    Ok(())
}

async fn pushover(
    http: &reqwest::Client,
    channel: &NotificationChannelDto,
    note: &Notification,
) -> Result<()> {
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
        .context("pushover request failed")?;
    if !resp.status().is_success() {
        return Err(anyhow!("pushover returned {}", resp.status()));
    }
    Ok(())
}

async fn webhook(
    http: &reqwest::Client,
    channel: &NotificationChannelDto,
    note: &Notification,
) -> Result<()> {
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
        .context("webhook request failed")?;
    if !resp.status().is_success() {
        return Err(anyhow!("webhook returned {}", resp.status()));
    }
    Ok(())
}

async fn email(channel: &NotificationChannelDto, note: &Notification) -> Result<()> {
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
        .body(note.body.clone())?;

    // STARTTLS on 587 (the common case); implicit TLS on 465
    let mut builder = if port == 465 {
        AsyncSmtpTransport::<Tokio1Executor>::relay(host)?
    } else {
        AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(host)?
    };
    builder = builder.port(port);
    if let (Ok(user), Ok(pass)) = (cfg(channel, "username"), cfg(channel, "password")) {
        builder = builder.credentials(Credentials::new(user.to_string(), pass.to_string()));
    }
    builder
        .build()
        .send(message)
        .await
        .context("smtp send failed")?;
    Ok(())
}
