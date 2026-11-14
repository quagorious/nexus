//! Quagorious HTTP + live-events client.
//!
//! Auth flow (see README):
//! 1. GW2 API key → `createsubtoken?permissions=account` (done once; only the
//!    subtoken is stored, never the key)
//! 2. subtoken → `POST /api/blish/session` → 24h "lushm" session token
//! 3. session → `GET /api/blish/live-token` → short-lived live token
//! 4. live token → SSE `GET {liveUrl}/api/blish/events?token=..`
//! 5. session → `GET /api/blish/encounters[/<id>]`

use std::{
    io::{BufRead, BufReader, Read},
    net::{Shutdown, TcpStream},
    sync::{Arc, LazyLock, Mutex},
    time::Duration,
};

use quagorious_card::{Encounter, TryList};
use serde::Deserialize;

const SITE: &str = "https://quagorious.gg";
const GW2_API: &str = "https://api.guildwars2.com";

#[derive(Debug)]
pub enum NetError {
    /// 401/403: the session (or subtoken) is no longer accepted.
    Unauthorized(String),
    Other(String),
}

impl std::fmt::Display for NetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NetError::Unauthorized(m) | NetError::Other(m) => f.write_str(m),
        }
    }
}

pub type NetResult<T> = Result<T, NetError>;

#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    pub token: String,
    /// Epoch milliseconds.
    pub expires_at: i64,
    pub live_url: String,
    #[serde(default)]
    pub account: String,
}

// ---------------------------------------------------------------------------
// Agents
// ---------------------------------------------------------------------------

fn tls_config() -> Arc<rustls::ClientConfig> {
    static CFG: LazyLock<Arc<rustls::ClientConfig>> = LazyLock::new(|| {
        let roots = rustls::RootCertStore { roots: webpki_roots::TLS_SERVER_ROOTS.to_vec() };
        let cfg = rustls::ClientConfig::builder_with_provider(Arc::new(
            rustls::crypto::ring::default_provider(),
        ))
        .with_safe_default_protocol_versions()
        .expect("rustls protocol versions")
        .with_root_certificates(roots)
        .with_no_client_auth();
        Arc::new(cfg)
    });
    CFG.clone()
}

/// The socket of the currently open SSE connection, so `unload` can shut it
/// down and unblock the reader thread immediately.
static SSE_SOCKET: Mutex<Option<TcpStream>> = Mutex::new(None);

/// TLS connector that remembers the raw TCP socket before handing it to rustls.
struct CapturingTls(Arc<rustls::ClientConfig>);

impl ureq::TlsConnector for CapturingTls {
    fn connect(
        &self,
        dns_name: &str,
        io: Box<dyn ureq::ReadWrite>,
    ) -> Result<Box<dyn ureq::ReadWrite>, ureq::Error> {
        if let Some(clone) = io.socket().and_then(|s| s.try_clone().ok()) {
            if let Ok(mut slot) = SSE_SOCKET.lock() {
                *slot = Some(clone);
            }
        }
        ureq::TlsConnector::connect(&self.0, dns_name, io)
    }
}

/// Closes the live connection (if any); the SSE reader then errors out.
pub fn shutdown_live_socket() {
    if let Ok(mut slot) = SSE_SOCKET.lock() {
        if let Some(s) = slot.take() {
            let _ = s.shutdown(Shutdown::Both);
        }
    }
}

fn agent() -> &'static ureq::Agent {
    static AGENT: LazyLock<ureq::Agent> = LazyLock::new(|| {
        ureq::AgentBuilder::new()
            .tls_config(tls_config())
            .timeout_connect(Duration::from_secs(10))
            .timeout(Duration::from_secs(20))
            .user_agent(concat!("quagorious-nexus/", env!("CARGO_PKG_VERSION")))
            .build()
    });
    &AGENT
}

fn sse_agent() -> &'static ureq::Agent {
    static AGENT: LazyLock<ureq::Agent> = LazyLock::new(|| {
        ureq::AgentBuilder::new()
            .tls_connector(Arc::new(CapturingTls(tls_config())))
            .timeout_connect(Duration::from_secs(10))
            // The server pings every 25 s; three missed pings means the
            // connection is dead. A list refresh on reconnect catches up.
            .timeout_read(Duration::from_secs(75))
            .user_agent(concat!("quagorious-nexus/", env!("CARGO_PKG_VERSION")))
            .build()
    });
    &AGENT
}

/// Turns a ureq error into a short, user-presentable message.
fn map_err(e: ureq::Error) -> NetError {
    match e {
        ureq::Error::Status(code, resp) => {
            let body = resp.into_string().unwrap_or_default();
            // Nuxt errors carry a readable `statusMessage`.
            let msg = serde_json::from_str::<serde_json::Value>(&body)
                .ok()
                .and_then(|v| {
                    v.get("statusMessage")
                        .or_else(|| v.get("text"))
                        .or_else(|| v.get("message"))
                        .and_then(|m| m.as_str())
                        .map(str::to_owned)
                })
                .unwrap_or_else(|| body.chars().take(120).collect());
            let msg = format!("HTTP {code}: {msg}");
            if code == 401 || code == 403 { NetError::Unauthorized(msg) } else { NetError::Other(msg) }
        }
        ureq::Error::Transport(t) => NetError::Other(t.to_string()),
    }
}

fn json<T: serde::de::DeserializeOwned>(r: Result<ureq::Response, ureq::Error>) -> NetResult<T> {
    r.map_err(map_err)?
        .into_json::<T>()
        .map_err(|e| NetError::Other(format!("unexpected response: {e}")))
}

// ---------------------------------------------------------------------------
// Endpoints
// ---------------------------------------------------------------------------

/// Step 1: account-only subtoken from a full API key. Returns (subtoken, expire).
pub fn create_subtoken(api_key: &str) -> NetResult<(String, String)> {
    #[derive(Deserialize)]
    struct R {
        subtoken: String,
    }
    let expire = (chrono::Utc::now() + chrono::Duration::days(365))
        .format("%Y-%m-%dT00:00:00Z")
        .to_string();
    let r: R = json(
        agent()
            .get(&format!("{GW2_API}/v2/createsubtoken"))
            .query("permissions", "account")
            .query("expire", &expire)
            .set("Authorization", &format!("Bearer {}", api_key.trim()))
            .call(),
    )?;
    Ok((r.subtoken, expire))
}

/// Step 2: subtoken → 24h module session.
pub fn open_session(subtoken: &str) -> NetResult<Session> {
    json(
        agent()
            .post(&format!("{SITE}/api/blish/session"))
            .send_json(serde_json::json!({ "subtoken": subtoken })),
    )
}

/// Step 3: session → live token.
pub fn live_token(session: &Session) -> NetResult<String> {
    #[derive(Deserialize)]
    struct R {
        token: String,
    }
    let r: R = json(
        agent()
            .get(&format!("{SITE}/api/blish/live-token"))
            .set("Authorization", &format!("Bearer {}", session.token))
            .call(),
    )?;
    Ok(r.token)
}

/// Your recent tries, newest first.
pub fn list_tries(session: &Session, limit: u32) -> NetResult<TryList> {
    json(
        agent()
            .get(&format!("{SITE}/api/blish/encounters"))
            .query("limit", &limit.to_string())
            .set("Authorization", &format!("Bearer {}", session.token))
            .call(),
    )
}

pub fn encounter(session: &Session, id: u64, locale: &str) -> NetResult<Encounter> {
    json(
        agent()
            .get(&format!("{SITE}/api/blish/encounters/{id}"))
            .query("locale", locale)
            .set("Authorization", &format!("Bearer {}", session.token))
            .call(),
    )
}

/// One server-sent event.
#[derive(Debug, Default)]
pub struct SseEvent {
    pub event: String,
    pub data: String,
}

/// Step 4: opens the live stream and calls `on_event` per event until the
/// stream ends, errors, or `on_event` returns `false`. `on_open` fires once
/// the connection is established.
pub fn listen(
    session: &Session,
    live_token: &str,
    on_open: impl FnOnce(),
    mut on_event: impl FnMut(SseEvent) -> bool,
) -> NetResult<()> {
    let base = session.live_url.trim_end_matches('/');
    let resp = sse_agent()
        .get(&format!("{base}/api/blish/events"))
        .query("token", live_token)
        .set("Accept", "text/event-stream")
        .call()
        .map_err(map_err)?;
    on_open();
    let reader = BufReader::new(resp.into_reader());
    let mut ev = SseEvent::default();
    for line in reader.lines() {
        let line = line.map_err(|e| NetError::Other(format!("live stream: {e}")))?;
        if line.is_empty() {
            // Dispatch.
            if !ev.data.is_empty() || !ev.event.is_empty() {
                if !on_event(std::mem::take(&mut ev)) {
                    return Ok(());
                }
            }
            continue;
        }
        if line.starts_with(':') {
            continue; // comment / keep-alive
        }
        let (field, value) = match line.split_once(':') {
            Some((f, v)) => (f, v.strip_prefix(' ').unwrap_or(v)),
            None => (line.as_str(), ""),
        };
        match field {
            "event" => ev.event = value.to_string(),
            "data" => {
                if !ev.data.is_empty() {
                    ev.data.push('\n');
                }
                ev.data.push_str(value);
            }
            _ => {}
        }
    }
    Ok(())
}

/// The website's full analysis of a try, readable with its share token.
pub fn full_encounter(id: u64, share_token: &str) -> NetResult<quagorious_card::FullEncounter> {
    json(
        agent()
            .get(&format!("{SITE}/api/encounters/{id}"))
            .query("share", share_token)
            .call(),
    )
}

/// GW2 `/v2/specializations?ids=all` and `/v2/professions?ids=all`, raw.
pub fn gw2_specs_raw() -> NetResult<(String, String)> {
    let get = |path: &str| -> NetResult<String> {
        agent()
            .get(&format!("{GW2_API}{path}"))
            .query("ids", "all")
            .call()
            .map_err(map_err)?
            .into_string()
            .map_err(|e| NetError::Other(e.to_string()))
    };
    Ok((get("/v2/specializations")?, get("/v2/professions")?))
}

pub fn fetch_bytes(url: &str) -> NetResult<Vec<u8>> {
    let resp = agent().get(url).call().map_err(map_err)?;
    let mut buf = Vec::new();
    resp.into_reader().take(1 << 20).read_to_end(&mut buf)
        .map_err(|e| NetError::Other(e.to_string()))?;
    Ok(buf)
}
