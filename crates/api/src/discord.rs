//! Discord channel webhooks: post a message (optionally with an image, or as
//! a new forum post), edit a message, read the webhook's guild and channel.
//!
//! No bot is involved: a webhook URL (`/api/webhooks/<id>/<token>`) is all
//! the user configures. See `docs/en/discord.md`.

use std::{thread, time::Duration};

use serde::Deserialize;

use crate::client::Client;

/// A checked webhook URL. Treat it as a secret: anyone who has it can post.
#[derive(Clone)]
pub struct Webhook {
    /// The webhook's numeric id (not secret).
    pub id: String,
    /// `https://…/api/webhooks/<id>/<token>`, without query or fragment.
    url: String,
}

impl std::fmt::Debug for Webhook {
    // Never print the token.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Webhook({})", self.id)
    }
}

const HOSTS: &[&str] =
    &["discord.com", "ptb.discord.com", "canary.discord.com", "discordapp.com", "ptb.discordapp.com", "canary.discordapp.com"];

impl Webhook {
    /// Accepts only `https://` Discord webhook URLs, so a typo can never
    /// send a card to some other host.
    pub fn parse(url: &str) -> Option<Webhook> {
        let rest = url.trim().strip_prefix("https://")?;
        let (host, path) = rest.split_once('/')?;
        if !HOSTS.contains(&host.to_ascii_lowercase().as_str()) {
            return None;
        }
        Self::parse_path(&format!("https://{host}"), path)
    }

    /// Like [`Webhook::parse`] but for any `base` (tests against a local server).
    #[doc(hidden)]
    pub fn with_base(base: &str, id: &str, token: &str) -> Webhook {
        Webhook { id: id.into(), url: format!("{base}/api/webhooks/{id}/{token}") }
    }

    fn parse_path(base: &str, path: &str) -> Option<Webhook> {
        let path = path.split(['?', '#']).next()?.trim_end_matches('/');
        let mut parts = path.split('/');
        if parts.next()? != "api" {
            return None;
        }
        let mut next = parts.next()?;
        // Optional API version: /api/v10/webhooks/…
        if next.len() > 1 && next.starts_with('v') && next[1..].bytes().all(|b| b.is_ascii_digit()) {
            next = parts.next()?;
        }
        if next != "webhooks" {
            return None;
        }
        let id = parts.next()?;
        let token = parts.next()?;
        let token_ok = !token.is_empty() && token.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
        if parts.next().is_some() || id.is_empty() || !id.bytes().all(|b| b.is_ascii_digit()) || !token_ok {
            return None;
        }
        Some(Webhook { id: id.into(), url: format!("{base}/api/webhooks/{id}/{token}") })
    }
}

/// `GET /webhooks/<id>/<token>`: where the webhook posts.
#[derive(Deserialize, Clone, Debug, Default)]
pub struct WebhookInfo {
    #[serde(default)]
    pub guild_id: Option<String>,
    #[serde(default)]
    pub channel_id: Option<String>,
}

/// The parts of a posted message we need.
#[derive(Deserialize, Clone, Debug)]
pub struct Message {
    pub id: String,
    /// The channel the message landed in: the thread when posted into (or
    /// creating) a forum post.
    pub channel_id: String,
    #[serde(default)]
    pub attachments: Vec<AttachmentInfo>,
    /// Raw embeds. An attachment an embed shows is listed only here (as its
    /// CDN URL), not in `attachments`.
    #[serde(default)]
    pub embeds: Vec<serde_json::Value>,
}

impl Message {
    /// The attachment embed `i` shows as its thumbnail, read from the CDN
    /// URL `…/attachments/<channel>/<attachment id>/<filename>?…`.
    pub fn embed_thumb(&self, i: usize) -> Option<AttachmentInfo> {
        let url = self.embeds.get(i)?.get("thumbnail")?.get("url")?.as_str()?;
        let mut parts = url.split(['?', '#']).next()?.rsplit('/');
        let filename = parts.next()?;
        let id = parts.next()?;
        (!filename.is_empty() && !id.is_empty() && id.bytes().all(|b| b.is_ascii_digit()))
            .then(|| AttachmentInfo { id: id.into(), filename: filename.into() })
    }
}

/// An attachment of a posted message.
#[derive(Deserialize, Clone, Debug, PartialEq)]
pub struct AttachmentInfo {
    pub id: String,
    pub filename: String,
}

/// A message edit. Only `content` is always sent; the rest is left as it
/// is on Discord unless set.
#[derive(Default)]
pub struct Edit<'a> {
    pub content: &'a str,
    /// Replaces all embeds.
    pub embeds: Option<serde_json::Value>,
    /// Attachment ids to keep; every other attachment is removed. Needed
    /// whenever `file` is set.
    pub keep: Option<Vec<String>>,
    /// A new attachment (embeds can show it as `attachment://<filename>`).
    pub file: Option<Attachment<'a>>,
}

pub struct Attachment<'a> {
    pub filename: &'a str,
    pub content_type: &'a str,
    pub bytes: &'a [u8],
}

#[derive(Default)]
pub struct NewMessage<'a> {
    pub content: &'a str,
    /// Post into this thread (forum post).
    pub thread_id: Option<&'a str>,
    /// Create a forum post with this title (forum channels only).
    pub thread_name: Option<&'a str>,
    pub file: Option<Attachment<'a>>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum DiscordError {
    /// 404: webhook, thread or message deleted.
    NotFound(String),
    /// 401/403: wrong token, or the webhook was deleted.
    Unauthorized(String),
    Other(String),
}

impl std::fmt::Display for DiscordError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DiscordError::NotFound(m) | DiscordError::Unauthorized(m) | DiscordError::Other(m) => f.write_str(m),
        }
    }
}

pub type DiscordResult<T> = Result<T, DiscordError>;

/// Longest wait for a rate limit before giving up on this message (short,
/// so unloading the addon never waits long for it).
const MAX_RETRY_WAIT: Duration = Duration::from_secs(5);
/// Discord's upload limit without server boosts.
pub const MAX_UPLOAD: usize = 10 * 1024 * 1024;
const BOUNDARY: &str = "quagorious-7f3a9c1e5b";

fn map_err(e: ureq::Error) -> DiscordError {
    match e {
        ureq::Error::Status(code, resp) => {
            let body = resp.into_string().unwrap_or_default();
            let msg = serde_json::from_str::<serde_json::Value>(&body)
                .ok()
                .and_then(|v| v.get("message").and_then(|m| m.as_str()).map(str::to_owned))
                .unwrap_or_else(|| body.chars().take(120).collect());
            let msg = format!("Discord HTTP {code}: {msg}");
            match code {
                404 => DiscordError::NotFound(msg),
                401 | 403 => DiscordError::Unauthorized(msg),
                _ => DiscordError::Other(msg),
            }
        }
        // Not `t.to_string()`: that includes the URL, i.e. the secret token.
        ureq::Error::Transport(t) => DiscordError::Other(match t.message() {
            Some(m) => format!("Discord: {}: {m}", t.kind()),
            None => format!("Discord: {}", t.kind()),
        }),
    }
}

/// Seconds to wait from a 429 body (`retry_after`, seconds with decimals).
fn retry_after(resp: ureq::Response) -> Duration {
    let header = resp.header("retry-after").and_then(|v| v.parse::<f64>().ok());
    let body = resp
        .into_string()
        .ok()
        .and_then(|b| serde_json::from_str::<serde_json::Value>(&b).ok())
        .and_then(|v| v.get("retry_after").and_then(|r| r.as_f64()));
    Duration::from_secs_f64(body.or(header).unwrap_or(1.0).clamp(0.0, 3600.0))
}

/// Sends `req`, retrying once after a short rate limit.
fn send(req: impl Fn() -> Result<ureq::Response, ureq::Error>) -> DiscordResult<ureq::Response> {
    match req() {
        Err(ureq::Error::Status(429, resp)) => {
            let wait = retry_after(resp);
            if wait > MAX_RETRY_WAIT {
                return Err(DiscordError::Other(format!("Discord rate limit ({}s)", wait.as_secs())));
            }
            thread::sleep(wait);
            req().map_err(map_err)
        }
        r => r.map_err(map_err),
    }
}

fn parse<T: serde::de::DeserializeOwned>(r: ureq::Response) -> DiscordResult<T> {
    r.into_json::<T>().map_err(|e| DiscordError::Other(format!("unexpected Discord response: {e}")))
}

/// The JSON part of an execute request.
fn payload(m: &NewMessage) -> serde_json::Value {
    let mut p = serde_json::json!({
        "content": m.content,
        "username": "Quagorious",
        // Names in the text must never ping anyone.
        "allowed_mentions": { "parse": [] },
    });
    if let Some(name) = m.thread_name {
        p["thread_name"] = name.into();
    }
    if let Some(f) = &m.file {
        p["attachments"] = serde_json::json!([{ "id": 0, "filename": f.filename }]);
    }
    p
}

/// `multipart/form-data` body: `payload_json` plus `files[0]`.
fn multipart(payload: &serde_json::Value, f: &Attachment) -> Vec<u8> {
    let mut body = Vec::with_capacity(f.bytes.len() + 1024);
    body.extend_from_slice(
        format!(
            "--{BOUNDARY}\r\nContent-Disposition: form-data; name=\"payload_json\"\r\n\
             Content-Type: application/json\r\n\r\n{payload}\r\n"
        )
        .as_bytes(),
    );
    body.extend_from_slice(
        format!(
            "--{BOUNDARY}\r\nContent-Disposition: form-data; name=\"files[0]\"; filename=\"{}\"\r\n\
             Content-Type: {}\r\n\r\n",
            f.filename, f.content_type
        )
        .as_bytes(),
    );
    body.extend_from_slice(f.bytes);
    body.extend_from_slice(format!("\r\n--{BOUNDARY}--\r\n").as_bytes());
    body
}

impl Client {
    /// Checks the webhook and reads where it posts.
    pub fn discord_info(&self, hook: &Webhook) -> DiscordResult<WebhookInfo> {
        parse(send(|| self.agent.get(&hook.url).call())?)
    }

    /// Executes the webhook (`wait=true`, so the message comes back).
    pub fn discord_post(&self, hook: &Webhook, m: &NewMessage) -> DiscordResult<Message> {
        let p = payload(m);
        let req = || {
            let mut r = self.agent.post(&hook.url).query("wait", "true");
            if let Some(t) = m.thread_id {
                r = r.query("thread_id", t);
            }
            r
        };
        let resp = match &m.file {
            Some(f) if f.bytes.len() > MAX_UPLOAD => {
                return Err(DiscordError::Other(format!("image too large for Discord ({} MB)", f.bytes.len() >> 20)));
            }
            Some(f) => {
                let body = multipart(&p, f);
                let ct = format!("multipart/form-data; boundary={BOUNDARY}");
                send(|| req().set("Content-Type", &ct).send_bytes(&body))?
            }
            None => send(|| req().send_json(p.clone()))?,
        };
        parse(resp)
    }

    /// Replaces a message's text (`thread_id` when it's in a forum post).
    pub fn discord_edit(&self, hook: &Webhook, thread_id: Option<&str>, message_id: &str, content: &str) -> DiscordResult<()> {
        self.discord_edit_message(hook, thread_id, message_id, &Edit { content, ..Default::default() }).map(|_| ())
    }

    /// Edits a message (text, embeds, attachments); returns the new state.
    pub fn discord_edit_message(
        &self,
        hook: &Webhook,
        thread_id: Option<&str>,
        message_id: &str,
        e: &Edit,
    ) -> DiscordResult<Message> {
        let mut p = serde_json::json!({ "content": e.content, "allowed_mentions": { "parse": [] } });
        if let Some(embeds) = &e.embeds {
            p["embeds"] = embeds.clone();
        }
        if e.keep.is_some() || e.file.is_some() {
            let mut list: Vec<serde_json::Value> =
                e.keep.iter().flatten().map(|id| serde_json::json!({ "id": id })).collect();
            if let Some(f) = &e.file {
                list.push(serde_json::json!({ "id": 0, "filename": f.filename }));
            }
            p["attachments"] = list.into();
        }
        let req = || {
            let mut r = self.agent.request("PATCH", &format!("{}/messages/{message_id}", hook.url));
            if let Some(t) = thread_id {
                r = r.query("thread_id", t);
            }
            r
        };
        let resp = match &e.file {
            Some(f) if f.bytes.len() > MAX_UPLOAD => {
                return Err(DiscordError::Other(format!("image too large for Discord ({} MB)", f.bytes.len() >> 20)));
            }
            Some(f) => {
                let body = multipart(&p, f);
                let ct = format!("multipart/form-data; boundary={BOUNDARY}");
                send(|| req().set("Content-Type", &ct).send_bytes(&body))?
            }
            None => send(|| req().send_json(p.clone()))?,
        };
        parse(resp)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_discord_urls_only() {
        let ok = Webhook::parse("https://discord.com/api/webhooks/123456/abc-DEF_9").unwrap();
        assert_eq!(ok.id, "123456");
        assert_eq!(ok.url, "https://discord.com/api/webhooks/123456/abc-DEF_9");
        let v = Webhook::parse(" https://canary.discordapp.com/api/v10/webhooks/1/t/?wait=true ").unwrap();
        assert_eq!(v.url, "https://canary.discordapp.com/api/webhooks/1/t");
        for bad in [
            "http://discord.com/api/webhooks/1/t",
            "https://discord.com.evil.example/api/webhooks/1/t",
            "https://evil.example/api/webhooks/1/t",
            "https://discord.com/api/webhooks/1",
            "https://discord.com/api/webhooks/x/t",
            "https://discord.com/api/webhooks/1/t/extra",
            "https://discord.com/api/webhooks/1/t%2F..",
            "https://discord.com/channels/1/2",
        ] {
            assert!(Webhook::parse(bad).is_none(), "{bad}");
        }
    }

    #[test]
    fn reads_the_thumbnail_attachment_from_the_embed() {
        let m: Message = serde_json::from_str(
            r#"{"id":"1","channel_id":"2","embeds":[{"title":"x"},{"thumbnail":{"url":"https://cdn.discordapp.com/attachments/2/77/thumb-5.png?ex=1&is=2"}}]}"#,
        )
        .unwrap();
        assert_eq!(m.embed_thumb(0), None);
        assert_eq!(m.embed_thumb(1), Some(AttachmentInfo { id: "77".into(), filename: "thumb-5.png".into() }));
        assert_eq!(m.embed_thumb(2), None);
    }

    #[test]
    fn debug_hides_the_token() {
        let w = Webhook::parse("https://discord.com/api/webhooks/1/secret").unwrap();
        assert_eq!(format!("{w:?}"), "Webhook(1)");
    }
}
