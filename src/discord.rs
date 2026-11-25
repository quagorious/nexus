//! Posting cards to Discord through a channel webhook. Runs on its own
//! thread so a slow Discord never holds up the cards. Tonight's forum post
//! is kept in `discord.json` after every step, so after a game crash the
//! next try goes into the same post (see `quagorious_api::daylog`).

use std::{
    fs,
    sync::{
        Mutex,
        atomic::Ordering,
        mpsc::{Receiver, Sender, channel},
    },
    thread::{self, JoinHandle},
};

use quagorious_api::{
    daylog::{DayLog, Outcome, TryPost, WebhookSink},
    discord::Webhook,
};

use crate::{STOP, data_path, lang, lock, net, set_message, tr};

pub enum Job {
    /// Record a try; `manual` = the "Post to Discord" button.
    Post { webhook: String, post: TryPost, manual: bool },
    /// Check the webhook URL (options page).
    Check { webhook: String },
}

static TX: Mutex<Option<Sender<Job>>> = Mutex::new(None);

pub fn start() -> JoinHandle<()> {
    let (tx, rx) = channel();
    *lock(&TX) = Some(tx);
    thread::spawn(move || run(rx))
}

/// The thread ends once its queue is closed.
pub fn stop() {
    lock(&TX).take();
}

pub fn queue(job: Job) {
    if let Some(tx) = lock(&TX).as_ref() {
        let _ = tx.send(job);
    }
}

/// Whether `url` is a usable webhook URL (empty counts as "off", not invalid).
pub fn is_valid(url: &str) -> bool {
    Webhook::parse(url).is_some()
}

const FILE: &str = "discord.json";

fn load() -> DayLog {
    data_path(FILE)
        .and_then(|p| fs::read_to_string(p).ok())
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

/// Write to a temp file and rename, so a crash mid-write keeps the old state.
fn save(log: &DayLog) {
    let Some(path) = data_path(FILE) else { return };
    let tmp = path.with_extension("json.tmp");
    let result = serde_json::to_string_pretty(log)
        .map_err(std::io::Error::other)
        .and_then(|t| fs::write(&tmp, t))
        .and_then(|()| fs::rename(&tmp, &path));
    if let Err(e) = result {
        log::warn!("could not save {FILE}: {e}");
    }
}

fn invalid_url() {
    set_message(tr("Discord: keine gültige Webhook-URL", "Discord: not a valid webhook URL"), true);
}

fn run(rx: Receiver<Job>) {
    let mut day = load();
    while let Ok(job) = rx.recv() {
        if STOP.load(Ordering::Relaxed) {
            break;
        }
        match job {
            Job::Check { webhook } => {
                let Some(hook) = Webhook::parse(&webhook) else {
                    invalid_url();
                    continue;
                };
                match net::client().discord_info(&hook) {
                    Ok(_) => set_message(tr("Discord-Webhook funktioniert", "Discord webhook works"), false),
                    Err(e) => set_message(format!("{} {e}", tr("Discord-Webhook geht nicht:", "Discord webhook failed:")), true),
                }
            }
            Job::Post { webhook, post, manual } => {
                let Some(hook) = Webhook::parse(&webhook) else {
                    invalid_url();
                    continue;
                };
                let sink = WebhookSink { client: net::client(), hook: &hook };
                match day.record(&sink, &hook.id, &post, lang(), &mut |l| save(l)) {
                    Ok(Outcome::Posted) => set_message(tr("Karte an Discord gesendet", "Card posted to Discord"), false),
                    Ok(Outcome::Duplicate) if manual => {
                        set_message(tr("Diese Karte ist schon auf Discord", "This card is already on Discord"), false)
                    }
                    Ok(_) => {}
                    Err(e) => {
                        log::warn!("discord: {e}");
                        set_message(format!("{} {e}", tr("Discord-Post fehlgeschlagen:", "Discord post failed:")), true);
                    }
                }
            }
        }
    }
}
