use std::{
    io::{BufRead, BufReader, Read},
    net::{Shutdown, TcpStream},
    sync::{
        Arc, LazyLock, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use quagorious_card::{Encounter, TryList};
use serde::Deserialize;

/// Base URLs; the defaults are production. The live host comes with the
/// session (`liveUrl`).
#[derive(Clone, Debug)]
pub struct Endpoints {
    pub site: String,
    pub gw2: String,
}

impl Default for Endpoints {
    fn default() -> Self {
        Self { site: "https://quagorious.gg".into(), gw2: "https://api.guildwars2.com".into() }
    }
}

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
    #[serde(alias = "access_token")]
    pub token: String,
    /// Epoch milliseconds.
    #[serde(alias = "expires_at")]
    pub expires_at: i64,
    #[serde(alias = "live_url")]
    pub live_url: String,
    #[serde(default)]
    pub account: String,
}

/// Same setup as ureq's default (ring + webpki roots); only needed because
/// `CapturingTls` has to wrap a config of its own.
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

type SocketSlot = Arc<Mutex<Option<TcpStream>>>;

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

/// TLS connector that remembers the raw TCP socket before handing it to rustls.
struct CapturingTls(Arc<rustls::ClientConfig>, SocketSlot);

impl ureq::TlsConnector for CapturingTls {
    fn connect(
        &self,
        dns_name: &str,
        io: Box<dyn ureq::ReadWrite>,
    ) -> Result<Box<dyn ureq::ReadWrite>, ureq::Error> {
        if let Some(clone) = io.socket().and_then(|s| s.try_clone().ok()) {
            *lock(&self.1) = Some(clone);
        }
        ureq::TlsConnector::connect(&self.0, dns_name, io)
    }
}

const USER_AGENT: &str = concat!("quagorious-nexus/", env!("CARGO_PKG_VERSION"));

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

/// Client with its agents and the remembered legacy-path fallbacks.
pub struct Client {
    ep: Endpoints,
    pub(crate) agent: ureq::Agent,
    sse_agent: ureq::Agent,
    live_socket: SocketSlot,
    legacy_session: AtomicBool,
    legacy_live_token: AtomicBool,
    legacy_events: AtomicBool,
}

impl Default for Client {
    fn default() -> Self {
        Self::new(Endpoints::default())
    }
}

/// 404/405: the path does not exist (yet) on the server.
fn is_missing(e: &ureq::Error) -> bool {
    matches!(e, ureq::Error::Status(404 | 405, _))
}

impl Client {
    pub fn new(ep: Endpoints) -> Self {
        let live_socket = SocketSlot::default();
        let agent = ureq::AgentBuilder::new()
            .timeout_connect(Duration::from_secs(10))
            .timeout(Duration::from_secs(20))
            .user_agent(USER_AGENT)
            .build();
        let sse_agent = ureq::AgentBuilder::new()
            .tls_connector(Arc::new(CapturingTls(tls_config(), live_socket.clone())))
            .timeout_connect(Duration::from_secs(10))
            // The server pings every 25 s; three missed pings means the
            // connection is dead. A list refresh on reconnect catches up.
            .timeout_read(Duration::from_secs(75))
            .user_agent(USER_AGENT)
            .build();
        Self {
            ep,
            agent,
            sse_agent,
            live_socket,
            legacy_session: AtomicBool::new(false),
            legacy_live_token: AtomicBool::new(false),
            legacy_events: AtomicBool::new(false),
        }
    }

    /// Closes the live connection (if any; only captured for https); the SSE
    /// reader then errors out.
    pub fn shutdown_live_socket(&self) {
        let socket = lock(&self.live_socket).take();
        if let Some(s) = socket {
            let _ = s.shutdown(Shutdown::Both);
        }
    }

    /// Tries the new path, then the legacy one on 404/405; the outcome is
    /// remembered in `legacy` so later calls go straight to the right path.
    fn with_fallback(
        &self,
        legacy: &AtomicBool,
        what: &str,
        new: impl FnOnce() -> Result<ureq::Response, ureq::Error>,
        old: impl FnOnce() -> Result<ureq::Response, ureq::Error>,
    ) -> Result<ureq::Response, ureq::Error> {
        if legacy.load(Ordering::Relaxed) {
            return old();
        }
        match new() {
            Err(e) if is_missing(&e) => {
                if !legacy.swap(true, Ordering::Relaxed) {
                    log::info!("{what}: new path not available, using the legacy one");
                }
                old()
            }
            r => r,
        }
    }

    /// Step 1: account-only subtoken (valid one year) from a full API key.
    pub fn create_subtoken(&self, api_key: &str) -> NetResult<String> {
        #[derive(Deserialize)]
        struct R {
            subtoken: String,
        }
        let expire = (chrono::Utc::now() + chrono::Duration::days(365))
            .format("%Y-%m-%dT00:00:00Z")
            .to_string();
        let r: R = json(
            self.agent
                .get(&format!("{}/v2/createsubtoken", self.ep.gw2))
                .query("permissions", "account")
                .query("expire", &expire)
                .set("Authorization", &format!("Bearer {}", api_key.trim()))
                .call(),
        )?;
        Ok(r.subtoken)
    }

    /// Step 2: subtoken → 24h module session.
    pub fn open_session(&self, subtoken: &str) -> NetResult<Session> {
        let site = &self.ep.site;
        json(self.with_fallback(
            &self.legacy_session,
            "session",
            || {
                self.agent.post(&format!("{site}/api/auth/token")).send_json(serde_json::json!({
                    "grant_type": "subtoken",
                    "client_id": "blish",
                    "subtoken": subtoken,
                }))
            },
            || {
                self.agent
                    .post(&format!("{site}/api/blish/session"))
                    .send_json(serde_json::json!({ "subtoken": subtoken }))
            },
        ))
    }

    /// Step 3: session → live token.
    pub fn live_token(&self, session: &Session) -> NetResult<String> {
        #[derive(Deserialize)]
        struct R {
            token: String,
        }
        let site = &self.ep.site;
        let auth = format!("Bearer {}", session.token);
        let r: R = json(self.with_fallback(
            &self.legacy_live_token,
            "live token",
            || self.agent.get(&format!("{site}/api/auth/live-token")).set("Authorization", &auth).call(),
            || self.agent.get(&format!("{site}/api/blish/live-token")).set("Authorization", &auth).call(),
        ))?;
        Ok(r.token)
    }

    /// Your recent tries, newest first.
    pub fn list_tries(&self, session: &Session, limit: u32) -> NetResult<TryList> {
        json(
            self.agent
                .get(&format!("{}/api/blish/encounters", self.ep.site))
                .query("limit", &limit.to_string())
                .set("Authorization", &format!("Bearer {}", session.token))
                .call(),
        )
    }

    pub fn encounter(&self, session: &Session, id: u64, locale: &str) -> NetResult<Encounter> {
        json(
            self.agent
                .get(&format!("{}/api/blish/encounters/{id}", self.ep.site))
                .query("locale", locale)
                .set("Authorization", &format!("Bearer {}", session.token))
                .call(),
        )
    }

    /// Step 4: opens the live stream and calls `on_event` per event until the
    /// stream ends, errors, or `on_event` returns `false`. `on_open` fires once
    /// the connection is established.
    pub fn listen(
        &self,
        session: &Session,
        live_token: &str,
        on_open: impl FnOnce(),
        mut on_event: impl FnMut(SseEvent) -> bool,
    ) -> NetResult<()> {
        let base = session.live_url.trim_end_matches('/');
        let get = |path: &str| {
            self.sse_agent
                .get(&format!("{base}{path}"))
                .query("token", live_token)
                .set("Accept", "text/event-stream")
                .call()
        };
        let resp = self
            .with_fallback(&self.legacy_events, "live stream", || get("/events"), || get("/api/blish/events"))
            .map_err(map_err)?;
        on_open();
        let reader = BufReader::new(resp.into_reader());
        let mut ev = SseEvent::default();
        for line in reader.lines() {
            let line = line.map_err(|e| NetError::Other(format!("live stream: {e}")))?;
            if line.is_empty() {
                // Dispatch.
                if (!ev.data.is_empty() || !ev.event.is_empty()) && !on_event(std::mem::take(&mut ev)) {
                    return Ok(());
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
    pub fn full_encounter(&self, id: u64, share_token: &str) -> NetResult<quagorious_card::FullEncounter> {
        json(
            self.agent
                .get(&format!("{}/api/encounters/{id}", self.ep.site))
                .query("share", share_token)
                .call(),
        )
    }

    /// A whole GW2 API collection, e.g. `/v2/specializations?ids=all`.
    pub fn gw2_all<T: serde::de::DeserializeOwned>(&self, path: &str) -> NetResult<Vec<T>> {
        json(self.agent.get(&format!("{}{path}", self.ep.gw2)).query("ids", "all").call())
    }

    /// Downloads a small file (icons, boss artwork); anything over 1 MB is an error.
    pub fn fetch_bytes(&self, url: &str) -> NetResult<Vec<u8>> {
        const LIMIT: u64 = 1 << 20;
        let resp = self.agent.get(url).call().map_err(map_err)?;
        let mut buf = Vec::new();
        resp.into_reader().take(LIMIT + 1).read_to_end(&mut buf).map_err(|e| NetError::Other(e.to_string()))?;
        if buf.len() as u64 > LIMIT {
            return Err(NetError::Other("file larger than 1 MB".into()));
        }
        Ok(buf)
    }
}

/// One server-sent event.
#[derive(Debug, Default)]
pub struct SseEvent {
    pub event: String,
    pub data: String,
}
