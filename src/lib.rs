//! Quagorious — a Nexus addon for Guild Wars 2.
//!
//! Listens to the Quagorious live feed; whenever one of your encounter logs is
//! uploaded it fetches the analysis, renders a share card (PNG) and shows it in
//! a window, from where it's one click (or keybind) to copy it to the clipboard
//! for pasting into Discord.
//!
//! Threads:
//! - render thread (Nexus): draws the UI, reads `SHARED`, sends `Cmd`s.
//! - worker: all request/response HTTP + card rendering, driven by `Cmd`s.
//! - live: holds the SSE stream open and turns events into `Cmd::Refresh`.

mod cards;
mod clipboard;
mod net;
mod shell;

use std::{
    collections::{HashMap, HashSet},
    fs,
    path::PathBuf,
    sync::{
        Arc, LazyLock, Mutex, MutexGuard,
        atomic::{AtomicBool, Ordering},
        mpsc::{Receiver, Sender, channel},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use chrono::{Local, TimeZone};
use nexus::{
    AddonFlags, UpdateProvider,
    alert::send_alert,
    gui::{RenderType, register_render, render},
    imgui::{Condition, Image, Ui, Window},
    keybind::{keybind_handler, register_keybind_with_string},
    paths::get_addon_dir,
    texture::get_texture_or_create_from_memory,
};
use cards::{CardView, VariantView};
use quagorious_card::{Encounter, FullEncounter, Lang, MATTE, TrySummary};
use serde::{Deserialize, Serialize};

use net::{NetError, NetResult, Session};

const ADDON_NAME: &str = "Quagorious";

nexus::export! {
    name: "Quagorious",
    signature: -0x51A6_0115, // unique negative signature; never change
    load,
    unload,
    flags: AddonFlags::None,
    // Nexus checks api.github.com/repos/quagorious/nexus/releases, takes the
    // highest `v<version>` tag that has a .dll asset and updates when it's
    // newer than the version compiled in from Cargo.toml.
    provider: UpdateProvider::GitHub,
    update_link: "https://github.com/quagorious/nexus",
    log_filter: "info"
}

// ---------------------------------------------------------------------------
// Global state
// ---------------------------------------------------------------------------

static STOP: AtomicBool = AtomicBool::new(false);
/// Ask the live thread to drop its connection and reconnect (account changed).
static LIVE_KICK: AtomicBool = AtomicBool::new(false);
static WINDOW_OPEN: AtomicBool = AtomicBool::new(false);
/// Key of the card variant the user last looked at ("overview", "fight",
/// "boon:alacrity"); the carousel and copy keybind follow it.
static FOCUS_KEY: Mutex<String> = Mutex::new(String::new());

static SETTINGS: LazyLock<Mutex<Settings>> = LazyLock::new(|| Mutex::new(Settings::default()));
static SHARED: LazyLock<Mutex<Shared>> = LazyLock::new(|| Mutex::new(Shared::default()));
/// The current module session, shared by worker and live thread.
static SESSION: Mutex<Option<Session>> = Mutex::new(None);
static CMD_TX: Mutex<Option<Sender<Cmd>>> = Mutex::new(None);
static THREADS: Mutex<Vec<JoinHandle<()>>> = Mutex::new(Vec::new());

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
struct Settings {
    /// Account-only GW2 subtoken (the API key itself is never stored).
    subtoken: Option<String>,
    locale: Lang,
    /// Pop the window open when a new encounter arrives.
    auto_open: bool,
    /// Copy the card image to the clipboard as soon as a new encounter arrives.
    auto_copy: bool,
    /// Show a Nexus alert when a new encounter arrives.
    notify: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            subtoken: None,
            locale: Lang::De,
            auto_open: true,
            auto_copy: false,
            notify: true,
        }
    }
}

#[derive(Default, Clone)]
enum Conn {
    #[default]
    NotConfigured,
    Connecting,
    Connected(String),
    Error(String),
}

#[derive(Default, Clone)]
enum Live {
    #[default]
    Off,
    Connecting,
    Streaming,
    Retrying(String),
}

/// Everything the UI shows; written by the background threads.
#[derive(Default)]
struct Shared {
    conn: Conn,
    live: Live,
    tries: Vec<TrySummary>,
    current: Option<u64>,
    card: Option<Arc<CardView>>,
    loading: bool,
    /// (text, is_error, when)
    message: Option<(String, bool, Instant)>,
    /// Encounter id → share token, learned from live events (the module API
    /// doesn't return it) and persisted in `tokens.json`. Unlocks the web
    /// link and the full analysis (extra card variants).
    web_tokens: HashMap<u64, String>,
}

fn web_url(token: &str) -> String {
    format!("https://quagorious.gg/app/encounters/{token}")
}

enum Cmd {
    Connect(String),
    Disconnect,
    /// Reload the try list; `announce` = show (and maybe copy) a new try.
    Refresh { announce: bool },
    Show(u64),
    /// A live `type: "encounter"` event: show this try right away.
    LiveEncounter { id: u64, token: Option<String>, boss_id: Option<u64> },
    /// Locale changed: drop cached details and re-render.
    Relocale,
}

fn send(cmd: Cmd) {
    if let Some(tx) = lock(&CMD_TX).as_ref() {
        let _ = tx.send(cmd);
    }
}

fn set_message(text: impl Into<String>, error: bool) {
    lock(&SHARED).message = Some((text.into(), error, Instant::now()));
}

fn lang() -> Lang {
    lock(&SETTINGS).locale
}

/// NUL-terminated UTF-16 for Win32 calls.
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

// ---------------------------------------------------------------------------
// Settings persistence
// ---------------------------------------------------------------------------

/// `file` (may include a subdirectory) inside the addon's data directory.
fn data_path(file: &str) -> Option<PathBuf> {
    let path = get_addon_dir(ADDON_NAME).ok()?.join(file);
    let _ = fs::create_dir_all(path.parent()?);
    Some(path)
}

fn load_settings() {
    let loaded = data_path("settings.json")
        .and_then(|p| fs::read_to_string(p).ok())
        .and_then(|t| serde_json::from_str::<Settings>(&t).ok());
    if let Some(s) = loaded {
        *lock(&SETTINGS) = s;
    }
}

fn save_settings() {
    let Some(path) = data_path("settings.json") else { return };
    let text = serde_json::to_string_pretty(&*lock(&SETTINGS));
    if let Ok(text) = text
        && let Err(e) = fs::write(&path, text)
    {
        log::warn!("could not save settings: {e}");
    }
}

// ---------------------------------------------------------------------------
// Session handling (shared by worker + live thread)
// ---------------------------------------------------------------------------

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

/// Returns a valid session, opening a new one from the stored subtoken when
/// there is none, it expires within 5 minutes, or `force` is set.
fn ensure_session(force: bool) -> NetResult<Session> {
    let mut slot = lock(&SESSION);
    if !force && let Some(s) = slot.as_ref().filter(|s| s.expires_at - now_ms() > 5 * 60_000) {
        return Ok(s.clone());
    }
    let Some(subtoken) = lock(&SETTINGS).subtoken.clone() else {
        return Err(NetError::Unauthorized("not connected".into()));
    };
    match net::open_session(&subtoken) {
        Ok(s) => {
            log::info!("session opened for {}", s.account);
            lock(&SHARED).conn = Conn::Connected(s.account.clone());
            *slot = Some(s.clone());
            Ok(s)
        }
        Err(e) => {
            *slot = None;
            lock(&SHARED).conn = Conn::Error(format!("Session: {e}"));
            Err(e)
        }
    }
}

/// Runs `f` with a session, retrying once with a fresh session on 401.
fn with_session<T>(f: impl Fn(&Session) -> NetResult<T>) -> NetResult<T> {
    let s = ensure_session(false)?;
    match f(&s) {
        Err(NetError::Unauthorized(_)) => f(&ensure_session(true)?),
        r => r,
    }
}

// ---------------------------------------------------------------------------
// Worker thread
// ---------------------------------------------------------------------------

#[derive(Default)]
struct Worker {
    /// Encounter details for the current locale.
    details: HashMap<u64, Encounter>,
    /// Try ids seen so far, to tell new uploads apart.
    known: HashSet<u64>,
    loaded_once: bool,
    /// Full analyses (locale-independent), fetched with the share token.
    full: HashMap<u64, FullEncounter>,
    specs: cards::SpecStore,
    /// Encounter id → boss id (from live events; the module API lacks it).
    boss_ids: HashMap<u64, u64>,
    boss_images: cards::BossImages,
}

fn worker_loop(rx: Receiver<Cmd>) {
    let mut w = Worker::default();
    if lock(&SETTINGS).subtoken.is_some() {
        lock(&SHARED).conn = Conn::Connecting;
        w.refresh(false);
    }
    while let Ok(cmd) = rx.recv() {
        if STOP.load(Ordering::Relaxed) {
            break;
        }
        w.handle(cmd);
    }
}

impl Worker {
    fn handle(&mut self, cmd: Cmd) {
        match cmd {
            Cmd::Connect(key) => self.connect(&key),
            Cmd::Disconnect => {
                lock(&SETTINGS).subtoken = None;
                save_settings();
                *lock(&SESSION) = None;
                *self = Worker::default();
                *lock(&SHARED) = Shared { web_tokens: cards::load_tokens(), ..Shared::default() };
                LIVE_KICK.store(true, Ordering::Relaxed);
                net::shutdown_live_socket();
            }
            Cmd::Refresh { announce } => self.refresh(announce),
            Cmd::Show(id) => self.show(id, false),
            Cmd::LiveEncounter { id, token, boss_id } => {
                if let Some(b) = boss_id {
                    self.boss_ids.insert(id, b);
                }
                if let Some(token) = token {
                    let tokens = {
                        let mut sh = lock(&SHARED);
                        sh.web_tokens.insert(id, token);
                        sh.web_tokens.clone()
                    };
                    cards::save_tokens(&tokens);
                }
                let is_new = self.known.insert(id);
                // Update the list first so the card gets its "today" chips.
                self.fetch_list();
                self.show(id, is_new);
            }
            Cmd::Relocale => {
                self.details.clear();
                if let Some(id) = lock(&SHARED).current {
                    self.show(id, false);
                }
            }
        }
    }

    fn connect(&mut self, api_key: &str) {
        lock(&SHARED).conn = Conn::Connecting;
        let subtoken = match net::create_subtoken(api_key) {
            Ok(t) => t,
            Err(e) => {
                lock(&SHARED).conn = Conn::Error(format!("GW2 API: {e}"));
                return;
            }
        };
        lock(&SETTINGS).subtoken = Some(subtoken);
        match ensure_session(true) {
            Ok(_) => {
                save_settings();
                *self = Worker::default();
                {
                    let mut sh = lock(&SHARED);
                    sh.tries.clear();
                    sh.card = None;
                    sh.current = None;
                }
                LIVE_KICK.store(true, Ordering::Relaxed);
                net::shutdown_live_socket();
                self.refresh(false);
            }
            Err(e) => log::warn!("connect failed: {e}"),
        }
    }

    /// Loads the try list into `SHARED`. Returns whether it changed, or
    /// `None` when loading failed (the error is shown in the window).
    fn fetch_list(&mut self) -> Option<bool> {
        let list = match with_session(|s| net::list_tries(s, 50)) {
            Ok(l) => l.tries,
            Err(e) => {
                log::warn!("refresh failed: {e}");
                set_message(format!("Refresh failed: {e}"), true);
                return None;
            }
        };
        let mut sh = lock(&SHARED);
        let changed = sh.tries != list;
        sh.tries = list;
        Some(changed)
    }

    /// Reloads the list (initial load and after reconnects). With `announce`,
    /// a try we haven't seen yet is shown like a live event.
    fn refresh(&mut self, announce: bool) {
        let Some(changed) = self.fetch_list() else { return };
        // Everything in the first list we get is old news, never "new".
        let first_load = !std::mem::replace(&mut self.loaded_once, true);
        if !changed && !first_load {
            return;
        }
        let (ids, current) = {
            let sh = lock(&SHARED);
            (sh.tries.iter().map(|t| t.id).collect::<Vec<_>>(), sh.current)
        };
        let newest = ids.first().copied();
        let unseen = newest.filter(|id| !self.known.contains(id));
        self.known.extend(ids);

        match unseen {
            Some(id) if announce && !first_load => {
                log::info!("new encounter {id}");
                self.show(id, true);
            }
            // Re-render: the "today" chips depend on the list.
            _ => {
                if let Some(id) = current.or(newest) {
                    self.show(id, false);
                }
            }
        }
    }

    fn show(&mut self, id: u64, is_new: bool) {
        lock(&SHARED).loading = true;
        let lang = lang();
        let enc = match self.details.get(&id) {
            Some(e) => e.clone(),
            None => match with_session(|s| net::encounter(s, id, lang.code())) {
                Ok(e) => {
                    self.details.insert(id, e.clone());
                    e
                }
                Err(e) => {
                    lock(&SHARED).loading = false;
                    set_message(format!("Could not load encounter {id}: {e}"), true);
                    return;
                }
            },
        };

        let (tries, token) = {
            let sh = lock(&SHARED);
            (sh.tries.clone(), sh.web_tokens.get(&id).cloned())
        };
        let full = token.as_deref().and_then(|t| self.full_analysis(id, t));
        let mut names: Vec<&str> = vec![enc.profession.as_str()];
        if let Some(f) = &full {
            names.extend(f.players.iter().map(|p| p.profession.as_str()));
        }
        self.specs.ensure(names);
        let url = token.as_deref().map(web_url);
        let boss_image = full
            .as_ref()
            .and_then(|f| f.encounter.boss_id)
            .or_else(|| self.boss_ids.get(&id).copied())
            .and_then(|b| self.boss_images.get(b));
        let view = Arc::new(cards::build(&enc, full.as_ref(), &tries, lang, url, &self.specs.specs, boss_image));
        {
            let mut sh = lock(&SHARED);
            sh.current = Some(id);
            sh.card = Some(view.clone());
            sh.loading = false;
        }

        if is_new {
            let (auto_open, auto_copy, notify) = {
                let s = lock(&SETTINGS);
                (s.auto_open, s.auto_copy, s.notify)
            };
            if auto_open {
                WINDOW_OPEN.store(true, Ordering::Relaxed);
            }
            let copied = auto_copy.then(|| copy_image(focused(&view)));
            if notify {
                send_alert(new_card_alert(&enc, focused(&view), copied, lang));
            }
        }
    }

    /// The website analysis for `id`; retried once since a just-uploaded
    /// try may still be processing.
    fn full_analysis(&mut self, id: u64, token: &str) -> Option<FullEncounter> {
        if let Some(f) = self.full.get(&id) {
            return Some(f.clone());
        }
        for attempt in 1..=2 {
            if attempt > 1 {
                thread::sleep(Duration::from_secs(2));
            }
            match net::full_encounter(id, token) {
                Ok(f) => {
                    self.full.insert(id, f.clone());
                    return Some(f);
                }
                Err(e) => log::warn!("full analysis for {id} failed (attempt {attempt}): {e}"),
            }
        }
        None
    }
}

/// The variant the carousel shows: the last-focused kind, else the first.
fn focused(card: &CardView) -> &VariantView {
    let key = lock(&FOCUS_KEY).clone();
    card.variants.iter().find(|v| v.key == key).unwrap_or(&card.variants[0])
}

/// Copies a card image; returns whether it worked.
fn copy_image(v: &VariantView) -> bool {
    match clipboard::set_image(v.width, v.height, &v.rgba, &v.png, MATTE) {
        Ok(()) => {
            set_message(format!("Card \"{}\" copied to clipboard", v.label), false);
            true
        }
        Err(e) => {
            set_message(format!("Copy failed: {e}"), true);
            false
        }
    }
}

/// e.g. "Quagorious: KILL Faultierion 3:12 · Karte kopiert". Only Latin-1
/// characters: the in-game font has nothing beyond that.
/// `copied`: None = auto-copy off, Some(ok) = copy attempted.
fn new_card_alert(enc: &Encounter, v: &VariantView, copied: Option<bool>, lang: Lang) -> String {
    use quagorious_card::text::{fmt_duration, fmt_hp};
    let result = if enc.success {
        format!("KILL {} {}", enc.boss, fmt_duration(enc.duration_ms))
    } else {
        format!("WIPE {} {} %", enc.boss, fmt_hp(enc.boss_hp_left, lang))
    };
    let what = match (copied, lang) {
        (Some(true), Lang::De) => format!("Karte \"{}\" kopiert", v.label),
        (Some(true), Lang::En) => format!("card \"{}\" copied", v.label),
        (Some(false), Lang::De) => "Kopieren fehlgeschlagen".to_string(),
        (Some(false), Lang::En) => "copy failed".to_string(),
        (None, Lang::De) => "neue Karte, mit dem Kopier-Keybind kopieren".to_string(),
        (None, Lang::En) => "new card, press the copy keybind".to_string(),
    };
    format!("Quagorious: {result} · {what}")
}

fn copy_text(card: &CardView) {
    match clipboard::set_text(&card.text) {
        Ok(()) => set_message("Summary text copied to clipboard", false),
        Err(e) => set_message(format!("Copy failed: {e}"), true),
    }
}

// ---------------------------------------------------------------------------
// Live (SSE) thread
// ---------------------------------------------------------------------------

/// Sleeps up to `d`, waking early on stop/kick.
fn nap(d: Duration) {
    let until = Instant::now() + d;
    while Instant::now() < until {
        if STOP.load(Ordering::Relaxed) || LIVE_KICK.load(Ordering::Relaxed) {
            return;
        }
        thread::sleep(Duration::from_millis(100));
    }
}

fn set_live(l: Live) {
    lock(&SHARED).live = l;
}

/// Payload of a live event (`data:` line). Observed:
/// `{"type":"encounter","id":24499,"token":"…","bossName":"Cairn","cm":0,
///   "success":true,"bossHpLeft":0,"durationMs":…,"timeStart":…,"seq":4,
///   "accounts":[…squad…],"bossId":…}`. Pings are `event: ping` with no data.
#[derive(Deserialize)]
struct LiveMessage {
    #[serde(rename = "type", default)]
    kind: String,
    #[serde(default)]
    id: Option<u64>,
    /// Web link token: https://quagorious.gg/app/encounters/<token>
    #[serde(default)]
    token: Option<String>,
    #[serde(default, rename = "bossId")]
    boss_id: Option<u64>,
}

fn handle_live_event(ev: net::SseEvent) {
    if ev.event == "ping" {
        return;
    }
    // Never log raw payloads: they carry the encounter's web token.
    match serde_json::from_str::<LiveMessage>(&ev.data) {
        Ok(LiveMessage { kind, id: Some(id), token, boss_id }) if kind == "encounter" => {
            log::info!("live: new encounter {id}");
            send(Cmd::LiveEncounter { id, token, boss_id });
        }
        Ok(msg) => log::debug!("live: ignoring '{}' / type '{}'", ev.event, msg.kind),
        Err(_) => {
            // Unknown shape: fall back to a list refresh so nothing is missed.
            log::info!("live: unrecognised event '{}', refreshing", ev.event);
            send(Cmd::Refresh { announce: true });
        }
    }
}

fn live_loop() {
    let mut backoff = 2u64;
    while !STOP.load(Ordering::Relaxed) {
        LIVE_KICK.store(false, Ordering::Relaxed);
        if lock(&SETTINGS).subtoken.is_none() {
            set_live(Live::Off);
            nap(Duration::from_secs(1));
            continue;
        }
        set_live(Live::Connecting);
        let started = Instant::now();
        let result = (|| -> NetResult<()> {
            let (session, token) = with_session(|s| Ok((s.clone(), net::live_token(s)?)))?;
            net::listen(
                &session,
                &token,
                || {
                    log::info!("live feed connected");
                    set_live(Live::Streaming);
                    // Catch up on anything uploaded while we were away.
                    send(Cmd::Refresh { announce: true });
                },
                |ev| {
                    handle_live_event(ev);
                    !STOP.load(Ordering::Relaxed) && !LIVE_KICK.load(Ordering::Relaxed)
                },
            )
        })();
        if STOP.load(Ordering::Relaxed) {
            break;
        }
        if LIVE_KICK.load(Ordering::Relaxed) {
            backoff = 2;
            continue;
        }
        let reason = match result {
            Ok(()) => "stream closed".to_string(),
            Err(e) => e.to_string(),
        };
        log::warn!("live feed: {reason}");
        if started.elapsed() > Duration::from_secs(60) {
            backoff = 2;
        }
        set_live(Live::Retrying(reason));
        nap(Duration::from_secs(backoff));
        backoff = (backoff * 2).min(60);
    }
    set_live(Live::Off);
}

// ---------------------------------------------------------------------------
// Lifecycle
// ---------------------------------------------------------------------------

fn load() {
    log::info!("Quagorious loading");
    STOP.store(false, Ordering::Relaxed);
    load_settings();
    lock(&SHARED).web_tokens = cards::load_tokens();

    let (tx, rx) = channel();
    *lock(&CMD_TX) = Some(tx);
    {
        let mut threads = lock(&THREADS);
        threads.push(thread::spawn(move || worker_loop(rx)));
        threads.push(thread::spawn(live_loop));
    }

    register_render(RenderType::Render, render!(|ui| draw_window(ui))).revert_on_unload();
    register_render(RenderType::OptionsRender, render!(|ui| draw_options(ui))).revert_on_unload();

    let toggle = keybind_handler!(|_id, is_release| {
        if !is_release {
            WINDOW_OPEN.fetch_xor(true, Ordering::Relaxed);
        }
    });
    register_keybind_with_string("QUAGORIOUS_TOGGLE", toggle, "ALT+SHIFT+L").revert_on_unload();

    let copy = keybind_handler!(|_id, is_release| {
        if !is_release {
            let card = lock(&SHARED).card.clone();
            match card {
                Some(c) if copy_image(focused(&c)) => send_alert("Quagorious: card copied"),
                Some(_) => send_alert("Quagorious: copy failed"),
                None => send_alert("Quagorious: no card yet"),
            }
        }
    });
    register_keybind_with_string("QUAGORIOUS_COPY", copy, "ALT+SHIFT+K").revert_on_unload();

    log::info!("Quagorious loaded");
}

fn unload() {
    log::info!("Quagorious unloading");
    STOP.store(true, Ordering::Relaxed);
    net::shutdown_live_socket();
    lock(&CMD_TX).take(); // worker's recv() returns Err once the sender is gone
    // Our code must not run after the DLL is gone: wait for the threads. The
    // socket is shut down again on every poll because the live thread may
    // have been mid-connect (no socket to close yet) the first time.
    let threads: Vec<_> = lock(&THREADS).drain(..).collect();
    let deadline = Instant::now() + Duration::from_secs(25);
    for t in threads {
        while !t.is_finished() && Instant::now() < deadline {
            net::shutdown_live_socket();
            thread::sleep(Duration::from_millis(20));
        }
        if t.is_finished() {
            let _ = t.join();
        } else {
            log::warn!("background thread did not stop in time");
        }
    }
}

// ---------------------------------------------------------------------------
// UI
// ---------------------------------------------------------------------------

const GREEN: [f32; 4] = [0.29, 0.87, 0.5, 1.0];
const RED: [f32; 4] = [0.97, 0.44, 0.44, 1.0];
const YELLOW: [f32; 4] = [0.98, 0.75, 0.14, 1.0];
const MUTED: [f32; 4] = [0.6, 0.58, 0.65, 1.0];
const PURPLE: [f32; 4] = [0.65, 0.55, 0.98, 1.0];

thread_local! {
    static KEY_INPUT: std::cell::RefCell<String> = const { std::cell::RefCell::new(String::new()) };
}

fn status_line(ui: &Ui, sh: &Shared) {
    match &sh.conn {
        Conn::NotConfigured => ui.text_colored(YELLOW, "Not connected - set your API key in Nexus > Options > Quagorious"),
        Conn::Connecting => ui.text_colored(MUTED, "Connecting..."),
        Conn::Error(e) => ui.text_colored(RED, format!("Error: {e}")),
        Conn::Connected(account) => {
            let (col, live) = match &sh.live {
                Live::Streaming => (GREEN, "live".to_string()),
                Live::Connecting => (MUTED, "connecting live feed...".to_string()),
                Live::Off => (MUTED, "live feed off".to_string()),
                Live::Retrying(r) => (YELLOW, format!("live feed lost ({r}), retrying")),
            };
            ui.text_colored(col, format!("{account} · {live}"));
        }
    }
}

fn try_label(t: &TrySummary, lang: Lang) -> String {
    let time = Local
        .timestamp_millis_opt(t.time_start)
        .single()
        .map(|d| d.format("%d.%m. %H:%M").to_string())
        .unwrap_or_default();
    let mode = if t.mode == "cm" { " CM" } else { "" };
    let res = if t.success {
        "Kill".to_string()
    } else {
        format!("{} %", quagorious_card::text::fmt_hp(t.boss_hp_left, lang))
    };
    format!("{time}  {}{mode} · {res}##{}", t.boss, t.id)
}

fn draw_window(ui: &Ui) {
    if !WINDOW_OPEN.load(Ordering::Relaxed) {
        return;
    }
    let mut open = true;
    Window::new("Quagorious##QUAGORIOUS")
        .opened(&mut open)
        .size([460.0, 720.0], Condition::FirstUseEver)
        .build(ui, || {
            let (tries, current, card, loading, message) = {
                let sh = lock(&SHARED);
                status_line(ui, &sh);
                (sh.tries.clone(), sh.current, sh.card.clone(), sh.loading, sh.message.clone())
            };
            let lang = lang();

            if !tries.is_empty() {
                let idx = current.and_then(|c| tries.iter().position(|t| t.id == c)).unwrap_or(0);
                if ui.arrow_button("##older", nexus::imgui::Direction::Left) && idx + 1 < tries.len() {
                    send(Cmd::Show(tries[idx + 1].id));
                }
                ui.same_line();
                let labels: Vec<String> = tries.iter().map(|t| try_label(t, lang)).collect();
                let mut sel = idx;
                ui.set_next_item_width(ui.content_region_avail()[0] - 30.0);
                if ui.combo_simple_string("##tries", &mut sel, &labels) && sel != idx {
                    send(Cmd::Show(tries[sel].id));
                }
                ui.same_line();
                if ui.arrow_button("##newer", nexus::imgui::Direction::Right) && idx > 0 {
                    send(Cmd::Show(tries[idx - 1].id));
                }
            }

            if let Some(card) = &card {
                // Carousel: one tab per card variant; the focused one is what
                // gets copied (also by the copy keybind).
                let cur = focused(card).key.clone();
                let idx = card.variants.iter().position(|v| v.key == cur).unwrap_or(0);
                let n = card.variants.len();
                if n > 1 {
                    let mut go = None;
                    if ui.arrow_button("##prevcard", nexus::imgui::Direction::Left) {
                        go = Some((idx + n - 1) % n);
                    }
                    for (i, v) in card.variants.iter().enumerate() {
                        ui.same_line();
                        if i == idx {
                            ui.text_colored(PURPLE, format!("[{}]", v.label));
                        } else if ui.small_button(format!("{}##var{i}", v.label)) {
                            go = Some(i);
                        }
                    }
                    ui.same_line();
                    if ui.arrow_button("##nextcard", nexus::imgui::Direction::Right) {
                        go = Some((idx + 1) % n);
                    }
                    if let Some(i) = go {
                        *lock(&FOCUS_KEY) = card.variants[i].key.clone();
                    }
                } else {
                    ui.text_disabled("Extra cards need the try's share link (tries seen live).");
                }

                if ui.button("Copy image") {
                    let _ = copy_image(focused(card));
                }
                ui.same_line();
                if ui.button("Copy text") {
                    copy_text(card);
                }
                if let Some(url) = &card.url {
                    ui.same_line();
                    if ui.button("Open in browser")
                        && let Err(e) = shell::open_url(url)
                    {
                        set_message(format!("Could not open browser: {e}"), true);
                    }
                }
                if loading {
                    ui.same_line();
                    ui.text_colored(MUTED, "loading...");
                }
            } else if loading {
                ui.text_colored(MUTED, "Loading...");
            }

            if let Some((text, err, at)) = &message
                && at.elapsed() < Duration::from_secs(6)
            {
                ui.text_colored(if *err { RED } else { GREEN }, text);
            }
            ui.separator();

            if let Some(card) = &card {
                let v = focused(card);
                match get_texture_or_create_from_memory(&v.preview_id, &v.preview_png) {
                    Some(tex) => {
                        let avail = ui.content_region_avail()[0].max(100.0);
                        let [w, h] = v.preview_size;
                        let k = (avail / w).min(1.0);
                        Image::new(tex.id(), [w * k, h * k]).build(ui);
                    }
                    None => ui.text_colored(MUTED, "Preparing preview..."),
                }
            }
        });
    if !open {
        WINDOW_OPEN.store(false, Ordering::Relaxed);
    }
}

fn draw_options(ui: &Ui) {
    {
        let sh = lock(&SHARED);
        status_line(ui, &sh);
    }
    ui.spacing();

    KEY_INPUT.with_borrow_mut(|key| {
        ui.set_next_item_width(320.0);
        let submitted = ui
            .input_text("GW2 API key", key)
            .password(true)
            .enter_returns_true(true)
            .build();
        ui.same_line();
        if (ui.button("Connect") || submitted) && !key.trim().is_empty() {
            send(Cmd::Connect(std::mem::take(key)));
        }
    });
    ui.text_disabled("Needs a key with the 'account' permission. Only a 1-year, account-only");
    ui.text_disabled("subtoken derived from it is stored; the key itself is not saved.");

    if lock(&SETTINGS).subtoken.is_some() && ui.button("Disconnect") {
        send(Cmd::Disconnect);
    }
    ui.spacing();
    ui.separator();

    let mut changed = false;
    let mut relocale = false;
    {
        let mut s = lock(&SETTINGS);
        let mut idx = s.locale as usize;
        ui.set_next_item_width(160.0);
        if ui.combo_simple_string("Card language", &mut idx, &["Deutsch", "English"]) {
            s.locale = if idx == 1 { Lang::En } else { Lang::De };
            changed = true;
            relocale = true;
        }
        changed |= ui.checkbox("Open window when a new encounter arrives", &mut s.auto_open);
        changed |= ui.checkbox("Copy new cards to the clipboard automatically", &mut s.auto_copy);
        changed |= ui.checkbox("Show a notification when a new encounter arrives", &mut s.notify);
    }
    if changed {
        save_settings();
    }
    if relocale {
        send(Cmd::Relocale);
    }
    ui.spacing();
    ui.text_disabled("Keybinds are set above / in Nexus > Keybinds:");
    ui.text_disabled("QUAGORIOUS_TOGGLE opens the window, QUAGORIOUS_COPY copies the focused card.");
    if ui.button("Open window") {
        WINDOW_OPEN.store(true, Ordering::Relaxed);
    }
}
