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

// A lock guard in an `if let`/`match` scrutinee lives for the whole block;
// calling code that takes the same lock in there deadlocks the game.
#![warn(clippy::significant_drop_in_scrutinee)]

mod cards;
mod clipboard;
mod discord;
mod net;
mod shell;
mod squad_ui;

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
/// The squad panel (follows the try shown in the main window).
static SQUAD_OPEN: AtomicBool = AtomicBool::new(false);
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
    /// Text size of the addon's windows (1.0 = the game's UI font).
    text_scale: f32,
    /// Discord channel webhook URL; empty = off. A secret: never log it.
    discord_webhook: String,
    discord_post: DiscordPost,
    /// Only post tries where you had the commander tag (one poster per squad).
    discord_commander_only: bool,
}

/// Which new tries go to Discord automatically.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Default)]
#[serde(rename_all = "lowercase")]
enum DiscordPost {
    #[default]
    Off,
    /// Kills get their card; wipes are only counted in the summary.
    Kills,
    All,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            subtoken: None,
            locale: Lang::De,
            auto_open: true,
            auto_copy: false,
            notify: true,
            text_scale: 1.0,
            discord_webhook: String::new(),
            discord_post: DiscordPost::Off,
            discord_commander_only: false,
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
    /// Squad panel for `current`; needs the try's full analysis.
    squad: Option<Arc<squad_ui::SquadPanel>>,
    loading: bool,
    /// (text, is_error, when)
    message: Option<(String, bool, Instant)>,
    /// Encounter id → share token, learned from live events (the module API
    /// doesn't return it) and persisted in `tokens.json`. Unlocks the web
    /// link and the full analysis (extra card variants).
    web_tokens: HashMap<u64, String>,
}

fn web_url(token: &str) -> String {
    format!("https://quagorious.gg/share/{token}")
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
    /// Post the shown card to Discord ("Post to Discord" button).
    PostDiscord,
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

/// `de` or `en`, following the language setting. Don't call it while
/// holding the `SETTINGS` lock (use [`tr_in`] there).
fn tr(de: &'static str, en: &'static str) -> &'static str {
    tr_in(lang(), de, en)
}

fn tr_in(lang: Lang, de: &'static str, en: &'static str) -> &'static str {
    if lang == Lang::De { de } else { en }
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
    /// Mechanic name → localized text, collected from module details (the
    /// full analysis only has English descriptions).
    texts: HashMap<String, String>,
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
            Cmd::PostDiscord => self.post_discord(),
            Cmd::Relocale => {
                self.details.clear();
                self.texts.clear();
                // Copy it out first: `show` takes the lock itself.
                let current = lock(&SHARED).current;
                if let Some(id) = current {
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
                    sh.squad = None;
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
                set_message(format!("{} {e}", tr("Aktualisieren fehlgeschlagen:", "Refresh failed:")), true);
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
                    for m in &e.mechanics {
                        self.texts.insert(m.name.clone(), m.text.clone());
                    }
                    self.details.insert(id, e.clone());
                    e
                }
                Err(e) => {
                    lock(&SHARED).loading = false;
                    set_message(format!("{} {id}: {e}", tr("Konnte Try nicht laden", "Could not load encounter")), true);
                    return;
                }
            },
        };

        let (tries, token) = {
            let sh = lock(&SHARED);
            (sh.tries.clone(), sh.web_tokens.get(&id).cloned())
        };
        let full = token.as_deref().and_then(|t| self.full_analysis(id, t, true));
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
        // Before the card: the squad view is also the "Squad" card variant.
        let squad = full.as_ref().map(|f| self.squad_view(&enc, f, &tries, lang));
        let view = Arc::new(cards::build(
            &enc,
            full.as_ref(),
            &tries,
            lang,
            url.clone(),
            &self.specs.specs,
            boss_image,
            squad.as_ref(),
        ));
        {
            let mut sh = lock(&SHARED);
            sh.current = Some(id);
            sh.card = Some(view.clone());
            sh.squad = squad.map(|v| Arc::new(squad_ui::SquadPanel::new(v, url.clone())));
            sh.loading = false;
        }

        if is_new {
            let (auto_open, auto_copy, notify, webhook, post, commander_only) = {
                let s = lock(&SETTINGS);
                (s.auto_open, s.auto_copy, s.notify, s.discord_webhook.clone(), s.discord_post, s.discord_commander_only)
            };
            if post != DiscordPost::Off && !webhook.is_empty() && (!commander_only || enc.commander) {
                let png = (post == DiscordPost::All || enc.success).then(|| focused(&view).png.clone());
                let start = tries.iter().find(|t| t.id == id).map(|t| t.time_start);
                let p = try_post(&enc, start.or(full.as_ref().map(|f| f.encounter.time_start)), url.clone(), png);
                discord::queue(discord::Job::Post { webhook, post: p, manual: false });
            }
            if auto_open {
                WINDOW_OPEN.store(true, Ordering::Relaxed);
            }
            let copied = auto_copy.then(|| copy_image(focused(&view)));
            if notify {
                send_alert(new_card_alert(&enc, focused(&view), copied, lang));
            }
        }
    }

    /// The shown card goes to Discord, whatever the auto-post options say.
    fn post_discord(&mut self) {
        let webhook = lock(&SETTINGS).discord_webhook.clone();
        let (current, card, start) = {
            let sh = lock(&SHARED);
            let start = sh.current.and_then(|c| sh.tries.iter().find(|t| t.id == c)).map(|t| t.time_start);
            (sh.current, sh.card.clone(), start)
        };
        let (Some(id), Some(card)) = (current, card) else { return };
        let Some(enc) = self.details.get(&id) else { return };
        let start = start.or_else(|| self.full.get(&id).map(|f| f.encounter.time_start));
        let p = try_post(enc, start, card.url.clone(), Some(focused(&card).png.clone()));
        discord::queue(discord::Job::Post { webhook, post: p, manual: true });
    }

    /// Builds the squad view for `enc`, loading the full analyses of today's
    /// other tries at the same boss (those whose share token we know; cached).
    fn squad_view(
        &mut self,
        enc: &Encounter,
        full: &FullEncounter,
        tries: &[TrySummary],
        lang: Lang,
    ) -> quagorious_card::squad::SquadView {
        use quagorious_card::squad::{SquadInput, TodayTry, build};
        let mut today: Vec<&TrySummary> = match tries.iter().find(|t| t.id == enc.id) {
            Some(cur) => {
                let day = cards::local_day(cur.time_start);
                tries
                    .iter()
                    .filter(|t| t.boss == cur.boss && t.mode == cur.mode && cards::local_day(t.time_start) == day)
                    .collect()
            }
            None => Vec::new(),
        };
        today.sort_by_key(|t| t.time_start);
        let tokens = lock(&SHARED).web_tokens.clone();
        for t in &today {
            if t.id != enc.id
                && let Some(token) = tokens.get(&t.id)
            {
                self.full_analysis(t.id, token, false);
            }
        }
        let mut today_tries: Vec<TodayTry> = today
            .iter()
            .map(|t| TodayTry {
                id: t.id,
                success: t.success,
                boss_hp_left: t.boss_hp_left,
                full: if t.id == enc.id { Some(full) } else { self.full.get(&t.id) },
            })
            .collect();
        if today_tries.is_empty() {
            today_tries.push(TodayTry { id: enc.id, success: enc.success, boss_hp_left: enc.boss_hp_left, full: Some(full) });
        }
        let names: Vec<String> = full.players.iter().map(|p| p.profession.clone()).collect();
        self.specs.ensure(names.iter().map(String::as_str));
        let start = tries.iter().find(|t| t.id == enc.id).map(|t| t.time_start).unwrap_or(full.encounter.time_start);
        let stamp = cards::stamp(start, lang);
        build(&SquadInput {
            id: enc.id,
            full,
            boss: &enc.boss,
            mode: &enc.mode,
            account: &enc.account,
            today: &today_tries,
            stamp: &stamp,
            texts: &self.texts,
            specs: &self.specs.specs,
            lang,
        })
    }

    /// The website analysis for `id`. With `retry`, a failure is retried
    /// once, since a just-uploaded try may still be processing.
    fn full_analysis(&mut self, id: u64, token: &str, retry: bool) -> Option<FullEncounter> {
        if let Some(f) = self.full.get(&id) {
            return Some(f.clone());
        }
        for attempt in 1..=(if retry { 2 } else { 1 }) {
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

fn try_post(enc: &Encounter, start_ms: Option<i64>, share_url: Option<String>, png: Option<Vec<u8>>) -> quagorious_api::daylog::TryPost {
    quagorious_api::daylog::TryPost {
        id: enc.id,
        boss: enc.boss.clone(),
        mode: enc.mode.clone(),
        success: enc.success,
        duration_ms: enc.duration_ms,
        hp_left: enc.boss_hp_left,
        start_ms: start_ms.filter(|&t| t > 0).unwrap_or_else(now_ms),
        share_url,
        png,
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
            let msg = match lang() {
                Lang::De => format!("Karte \"{}\" in die Zwischenablage kopiert", v.label),
                Lang::En => format!("Card \"{}\" copied to clipboard", v.label),
            };
            set_message(msg, false);
            true
        }
        Err(e) => {
            set_message(format!("{} {e}", tr("Kopieren fehlgeschlagen:", "Copy failed:")), true);
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
        Ok(()) => set_message(tr("Zusammenfassung als Text kopiert", "Summary text copied to clipboard"), false),
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

fn handle_live_event(ev: net::SseEvent) {
    // Never log raw payloads: they carry the try's share token.
    match net::parse_live_event(&ev) {
        net::LiveEvent::NewTry { id, token, boss_id } => {
            log::info!("live: new encounter {id}");
            send(Cmd::LiveEncounter { id, token, boss_id });
        }
        net::LiveEvent::Ignore => {}
        net::LiveEvent::Unknown => {
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
        threads.push(discord::start());
    }

    register_render(
        RenderType::Render,
        render!(|ui| {
            draw_window(ui);
            squad_ui::draw(ui);
        }),
    )
    .revert_on_unload();
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
                Some(c) if copy_image(focused(&c)) => send_alert(tr("Quagorious: Karte kopiert", "Quagorious: card copied")),
                Some(_) => send_alert(tr("Quagorious: Kopieren fehlgeschlagen", "Quagorious: copy failed")),
                None => send_alert(tr("Quagorious: noch keine Karte", "Quagorious: no card yet")),
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
    discord::stop();
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
        Conn::NotConfigured => ui.text_colored(
            YELLOW,
            tr(
                "Nicht verbunden - API-Key unter Nexus > Optionen > Quagorious eintragen",
                "Not connected - set your API key in Nexus > Options > Quagorious",
            ),
        ),
        Conn::Connecting => ui.text_colored(MUTED, tr("Verbinde...", "Connecting...")),
        Conn::Error(e) => ui.text_colored(RED, format!("{} {e}", tr("Fehler:", "Error:"))),
        Conn::Connected(account) => {
            let (col, live) = match &sh.live {
                Live::Streaming => (GREEN, "live".to_string()),
                Live::Connecting => (MUTED, tr("verbinde Live-Feed...", "connecting live feed...").to_string()),
                Live::Off => (MUTED, tr("Live-Feed aus", "live feed off").to_string()),
                Live::Retrying(r) => match lang() {
                    Lang::De => (YELLOW, format!("Live-Feed unterbrochen ({r}), neuer Versuch")),
                    Lang::En => (YELLOW, format!("live feed lost ({r}), retrying")),
                },
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
            ui.set_window_font_scale(squad_ui::text_scale());
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
                    ui.text_disabled(tr(
                        "Weitere Karten brauchen den Share-Link des Trys (nur live gesehene Tries).",
                        "Extra cards need the try's share link (tries seen live).",
                    ));
                }

                if ui.button(tr("Bild kopieren", "Copy image")) {
                    let _ = copy_image(focused(card));
                }
                ui.same_line();
                if ui.button(tr("Text kopieren", "Copy text")) {
                    copy_text(card);
                }
                if !lock(&SETTINGS).discord_webhook.is_empty() {
                    ui.same_line();
                    if ui.button(tr("An Discord", "Post to Discord")) {
                        send(Cmd::PostDiscord);
                    }
                    if ui.is_item_hovered() {
                        ui.tooltip_text(tr(
                            "Postet die gezeigte Karte in den Raidabend-Post auf Discord.",
                            "Posts the card shown here to tonight's post on Discord.",
                        ));
                    }
                }
                if let Some(url) = &card.url {
                    ui.same_line();
                    if ui.button(tr("Im Browser öffnen", "Open in browser"))
                        && let Err(e) = shell::open_url(url)
                    {
                        set_message(format!("{} {e}", tr("Browser konnte nicht geöffnet werden:", "Could not open browser:")), true);
                    }
                }
                ui.same_line();
                let squad_ready = lock(&SHARED).squad.is_some();
                if squad_ready {
                    if ui.button(tr("Squad-Fenster", "Squad window")) {
                        SQUAD_OPEN.fetch_xor(true, Ordering::Relaxed);
                    }
                    if ui.is_item_hovered() {
                        ui.tooltip_text(tr(
                            "Die Squad-Ansicht im eigenen Fenster, mit Details beim Drüberfahren.",
                            "The squad view in its own window, with details on hover.",
                        ));
                    }
                } else {
                    ui.text_disabled(tr("Squad-Fenster", "Squad window"));
                    if ui.is_item_hovered() {
                        ui.tooltip_text(tr(
                            "Die Squad-Ansicht braucht den Share-Link des Trys (nur live gesehene Tries).",
                            "The squad view needs the try's share link (tries seen live).",
                        ));
                    }
                }
                if loading {
                    ui.same_line();
                    ui.text_colored(MUTED, tr("lädt...", "loading..."));
                }
            } else if loading {
                ui.text_colored(MUTED, tr("Lädt...", "Loading..."));
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
                    None => ui.text_colored(MUTED, tr("Vorschau wird vorbereitet...", "Preparing preview...")),
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

    let l = lang();
    let t = |de, en| tr_in(l, de, en);
    KEY_INPUT.with_borrow_mut(|key| {
        ui.set_next_item_width(320.0);
        let submitted = ui
            .input_text(t("GW2-API-Key", "GW2 API key"), key)
            .password(true)
            .enter_returns_true(true)
            .build();
        ui.same_line();
        if (ui.button(t("Verbinden", "Connect")) || submitted) && !key.trim().is_empty() {
            send(Cmd::Connect(std::mem::take(key)));
        }
    });
    ui.text_disabled(t(
        "Braucht einen Key mit der Berechtigung 'account'. Gespeichert wird nur ein daraus",
        "Needs a key with the 'account' permission. Only a 1-year, account-only",
    ));
    ui.text_disabled(t(
        "erzeugter Subtoken (1 Jahr, nur 'account'); der Key selbst wird nicht gespeichert.",
        "subtoken derived from it is stored; the key itself is not saved.",
    ));

    if lock(&SETTINGS).subtoken.is_some() && ui.button(t("Trennen", "Disconnect")) {
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
        if ui.combo_simple_string("Sprache / Language", &mut idx, &["Deutsch", "English"]) {
            s.locale = if idx == 1 { Lang::En } else { Lang::De };
            changed = true;
            relocale = true;
        }
        changed |= ui.checkbox(t("Fenster bei neuem Try öffnen", "Open window when a new encounter arrives"), &mut s.auto_open);
        changed |= ui.checkbox(
            t("Neue Karten automatisch in die Zwischenablage kopieren", "Copy new cards to the clipboard automatically"),
            &mut s.auto_copy,
        );
        changed |= ui.checkbox(
            t("Benachrichtigung bei neuem Try anzeigen", "Show a notification when a new encounter arrives"),
            &mut s.notify,
        );
        ui.set_next_item_width(160.0);
        let mut pct = (s.text_scale * 100.0).round() as i32;
        if nexus::imgui::Slider::new(t("Textgröße der Fenster", "Window text size"), 80, 200)
            .display_format("%d %%")
            .build(ui, &mut pct)
        {
            s.text_scale = pct as f32 / 100.0;
            changed = true;
        }
    }
    ui.spacing();
    ui.separator();
    ui.text("Discord");
    {
        let mut s = lock(&SETTINGS);
        ui.set_next_item_width(320.0);
        changed |= ui.input_text(t("Webhook-URL", "Webhook URL"), &mut s.discord_webhook).password(true).build();
        ui.same_line();
        if ui.button(t("Prüfen", "Check")) {
            discord::queue(discord::Job::Check { webhook: s.discord_webhook.clone() });
        }
        if !s.discord_webhook.is_empty() && !discord::is_valid(&s.discord_webhook) {
            ui.text_colored(
                RED,
                t(
                    "Keine Discord-Webhook-URL (https://discord.com/api/webhooks/...)",
                    "Not a Discord webhook URL (https://discord.com/api/webhooks/...)",
                ),
            );
        }
        let mut idx = s.discord_post as usize;
        ui.set_next_item_width(160.0);
        let modes = [t("Aus", "Off"), t("Nur Kills", "Kills only"), t("Jeder Try", "Every try")];
        if ui.combo_simple_string(t("Automatisch posten", "Post automatically"), &mut idx, &modes) {
            s.discord_post = [DiscordPost::Off, DiscordPost::Kills, DiscordPost::All][idx.min(2)];
            changed = true;
        }
        changed |= ui.checkbox(
            t("Nur posten, wenn ich Kommandeur bin", "Only post when I'm the commander"),
            &mut s.discord_commander_only,
        );
    }
    ui.text_disabled(t(
        "Gepostet wird die Karte, die im Fenster gezeigt wird. Ein Forum-Kanal bekommt einen Post pro Raidabend.",
        "Posts the card shown in the window. A forum channel gets one post per raid night.",
    ));
    ui.text_disabled(t(
        "Die URL ist geheim: Wer sie hat, kann in den Kanal posten.",
        "Keep the URL secret: anyone who has it can post to the channel.",
    ));

    if changed {
        save_settings();
    }
    if relocale {
        send(Cmd::Relocale);
    }
    ui.spacing();
    ui.text_disabled(t(
        "Tastenbelegung oben bzw. unter Nexus > Tastenbelegung:",
        "Keybinds are set above / in Nexus > Keybinds:",
    ));
    ui.text_disabled(t(
        "QUAGORIOUS_TOGGLE öffnet das Fenster, QUAGORIOUS_COPY kopiert die gezeigte Karte.",
        "QUAGORIOUS_TOGGLE opens the window, QUAGORIOUS_COPY copies the focused card.",
    ));
    if ui.button(t("Fenster öffnen", "Open window")) {
        WINDOW_OPEN.store(true, Ordering::Relaxed);
    }
}
