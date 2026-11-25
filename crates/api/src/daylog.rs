//! One forum post per raid night in a Discord forum channel:
//!
//! ```text
//! 1125_Wed                      ← forum post (thread), one per raid night
//!   summary                     ← first message, edited after every try
//!   ## Harvest Temple CM        ← header message per boss (jump link target)
//!   Wipe 1 · 12.4 % · [card]
//!   Kill · 7:42 · [card]
//!   ## Dhuum CM
//!   …
//! ```
//!
//! Discord nests only one level (channel → thread), so the boss level is a
//! header message and the summary links to it. Webhooks can't list threads,
//! so [`DayLog`] remembers the ids. The caller persists it after every step
//! (`save`), so a game crash mid-run continues in the same post.
//!
//! The summary shows one embed per boss (up to [`MAX_EMBEDS`]) with the
//! boss's latest card as a thumbnail, attached to the summary itself: a click
//! opens Discord's image preview without leaving the top of the post. If
//! Discord rejects that, the summary falls back to plain text lines.
//!
//! A webhook of a plain text channel ignores the thread title (assumed:
//! Discord doesn't document it); then everything goes into the channel.

use chrono::{Datelike, Duration, Local, NaiveDate, TimeZone};
use quagorious_card::{
    Lang,
    text::{fmt_duration, fmt_hp, mode_label},
};
use serde::{Deserialize, Serialize};

use crate::{
    Client,
    discord::{Attachment, DiscordError, DiscordResult, Edit, Message, NewMessage, Webhook, WebhookInfo},
};

/// A raid night runs until this local hour of the next day, so a run past
/// midnight stays in one post.
pub const DAY_START_HOUR: i64 = 6;

/// Discord's limit for message text.
const MAX_CONTENT: usize = 2000;
/// Discord allows 10 embeds and 10 attachments per message; bosses past
/// this get a text line in the summary instead.
pub const MAX_EMBEDS: usize = 10;
const GREEN: u32 = 0x4ade80;
const RED: u32 = 0xf87171;

/// What [`DayLog`] needs from Discord; implemented for the real webhook by
/// [`WebhookSink`] and by fakes in tests.
pub trait Sink {
    fn info(&self) -> DiscordResult<WebhookInfo>;
    fn post(&self, m: &NewMessage) -> DiscordResult<Message>;
    fn edit(&self, thread_id: Option<&str>, message_id: &str, e: &Edit) -> DiscordResult<Message>;
}

pub struct WebhookSink<'a> {
    pub client: &'a Client,
    pub hook: &'a Webhook,
}

impl Sink for WebhookSink<'_> {
    fn info(&self) -> DiscordResult<WebhookInfo> {
        self.client.discord_info(self.hook)
    }
    fn post(&self, m: &NewMessage) -> DiscordResult<Message> {
        self.client.discord_post(self.hook, m)
    }
    fn edit(&self, thread_id: Option<&str>, message_id: &str, e: &Edit) -> DiscordResult<Message> {
        self.client.discord_edit_message(self.hook, thread_id, message_id, e)
    }
}

/// One try to record.
#[derive(Clone, Debug)]
pub struct TryPost {
    pub id: u64,
    pub boss: String,
    /// "cm", "lcm" or "nm".
    pub mode: String,
    pub success: bool,
    pub duration_ms: u64,
    pub hp_left: f64,
    /// Epoch milliseconds.
    pub start_ms: i64,
    pub share_url: Option<String>,
    /// The card image; `None` counts the try in the summary without posting it.
    pub png: Option<Vec<u8>>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct TryLog {
    pub id: u64,
    pub success: bool,
    pub duration_ms: u64,
    pub hp_left: f64,
    /// Epoch milliseconds (0 in logs saved before this field existed).
    pub start_ms: i64,
    /// The card's message, once posted.
    pub message_id: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct BossLog {
    /// "boss|mode".
    pub key: String,
    /// e.g. "Harvest Temple CM".
    pub name: String,
    pub header_id: Option<String>,
    pub tries: Vec<TryLog>,
    /// The boss's latest card, attached to the summary as its thumbnail.
    pub thumb: Option<Thumb>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct Thumb {
    pub attachment_id: String,
    pub filename: String,
}

/// One summary line: the padded code span, the result, the boss name and
/// where its name links to.
struct Row {
    code: String,
    killed: bool,
    name: String,
    link: Option<String>,
}

/// The state of tonight's forum post (persisted as `discord.json`).
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct DayLog {
    /// Which webhook this belongs to (its id, not the secret token).
    pub webhook_id: String,
    /// The raid night, `YYYY-MM-DD`.
    pub day: String,
    pub guild_id: Option<String>,
    /// The webhook's channel.
    pub channel_id: Option<String>,
    /// Tonight's forum post; `None` until created (or in `flat` mode).
    pub thread_id: Option<String>,
    /// The channel isn't a forum: post straight into it.
    pub flat: bool,
    pub summary_id: Option<String>,
    pub bosses: Vec<BossLog>,
}

/// What happened to a [`DayLog::record`] call.
#[derive(Debug, PartialEq)]
pub enum Outcome {
    Posted,
    /// Only counted in the summary (no image).
    Counted,
    /// Already posted earlier.
    Duplicate,
}

/// The raid night a try belongs to.
pub fn raid_day(start_ms: i64) -> NaiveDate {
    let t = Local
        .timestamp_millis_opt(start_ms)
        .single()
        .unwrap_or_else(|| Local.timestamp_millis_opt(0).unwrap());
    (t - Duration::hours(DAY_START_HOUR)).date_naive()
}

fn weekday(d: NaiveDate, lang: Lang) -> &'static str {
    let i = d.weekday().num_days_from_monday() as usize;
    match lang {
        Lang::De => ["Mo", "Di", "Mi", "Do", "Fr", "Sa", "So"][i],
        Lang::En => ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"][i],
    }
}

/// Forum post title, e.g. `1125_Wed`.
pub fn thread_title(d: NaiveDate, lang: Lang) -> String {
    format!("{}_{}", d.format("%m%d"), weekday(d, lang))
}

fn boss_name(boss: &str, mode: &str) -> String {
    match mode_label(mode) {
        Some(m) => format!("{boss} {m}"),
        None => boss.to_string(),
    }
}

fn clock(ms: i64) -> String {
    Local.timestamp_millis_opt(ms).single().map(|t| t.format("%H:%M").to_string()).unwrap_or_default()
}

impl BossLog {
    /// From the first try's start to the last try's end, e.g. `20:11–21:05`.
    pub fn span(&self) -> Option<String> {
        let start = self.tries.iter().map(|t| t.start_ms).filter(|&s| s > 0).min()?;
        let end = self.tries.iter().filter(|t| t.start_ms > 0).map(|t| t.start_ms + t.duration_ms as i64).max()?;
        Some(format!("{}–{}", clock(start), clock(end)))
    }

    /// The heading message, e.g. `## Harvest Temple CM · 20:11–21:05`.
    pub fn heading(&self) -> String {
        match self.span() {
            Some(span) => format!("## {} · {span}", self.name),
            None => format!("## {}", self.name),
        }
    }
}

fn truncate(mut s: String, max: usize) -> String {
    if s.chars().count() > max {
        s = s.chars().take(max - 1).collect::<String>() + "…";
    }
    s
}

impl DayLog {
    fn jump(&self, message_id: &str) -> Option<String> {
        let guild = self.guild_id.as_deref()?;
        let channel = self.thread_id.as_deref().or(self.channel_id.as_deref())?;
        Some(format!("https://discord.com/channels/{guild}/{channel}/{message_id}"))
    }

    fn title(&self, lang: Lang) -> String {
        let day = NaiveDate::parse_from_str(&self.day, "%Y-%m-%d").ok();
        let date = day.map(|d| match lang {
            Lang::De => format!("{} {}", weekday(d, lang), d.format("%d.%m.%Y")),
            Lang::En => format!("{} {}", weekday(d, lang), d.format("%Y-%m-%d")),
        });
        format!(
            "**{}{}**\n",
            if lang == Lang::De { "Raidabend" } else { "Raid night" },
            date.map(|d| format!(" · {d}")).unwrap_or_default()
        )
    }

    /// Discord has no tables, so the numbers go into one code span per boss
    /// (monospace, padded to equal width): they line up as columns.
    fn rows(&self, lang: Lang) -> Vec<Row> {
        let raw: Vec<(String, String, String, bool, &BossLog)> = self
            .bosses
            .iter()
            .map(|b| {
                let wipes = b.tries.iter().filter(|t| !t.success).count();
                let kill = b.tries.iter().filter(|t| t.success).min_by_key(|t| t.duration_ms);
                let wipes = match (lang, wipes) {
                    (_, 0) => String::new(),
                    (_, 1) => "1 Wipe".to_string(),
                    (Lang::De, n) => format!("{n} Wipes"),
                    (Lang::En, n) => format!("{n} wipes"),
                };
                let result = match kill {
                    Some(k) => format!("Kill {}", fmt_duration(k.duration_ms)),
                    None => {
                        let best = b.tries.iter().map(|t| t.hp_left).fold(f64::INFINITY, f64::min);
                        match lang {
                            Lang::De => format!("bestes {} %", fmt_hp(best, lang)),
                            Lang::En => format!("best {} %", fmt_hp(best, lang)),
                        }
                    }
                };
                let span = b.span().unwrap_or_else(|| "--:-----:--".into());
                (span, wipes, result, kill.is_some(), b)
            })
            .collect();
        let w_wipes = raw.iter().map(|r| r.1.chars().count()).max().unwrap_or(0);
        let w_result = raw.iter().map(|r| r.2.chars().count()).max().unwrap_or(0);
        raw.into_iter()
            .map(|(span, wipes, result, killed, b)| {
                // Right-aligned, so no column ends in spaces Discord might trim.
                let wipes = if w_wipes > 0 { format!("  {wipes:>w_wipes$}") } else { String::new() };
                // A boss with one try links straight to its card, otherwise
                // to its heading.
                let target = match b.tries.as_slice() {
                    [only] if only.message_id.is_some() => only.message_id.as_deref(),
                    _ => b.header_id.as_deref(),
                };
                Row {
                    code: format!("`{span}{wipes}  {result:>w_result$}`"),
                    killed,
                    name: b.name.clone(),
                    link: target.and_then(|m| self.jump(m)),
                }
            })
            .collect()
    }

    fn text_line(r: &Row) -> String {
        let mark = if r.killed { "✅" } else { "❌" };
        match &r.link {
            Some(link) => format!("{} {mark} [**{}**](<{link}>)\n", r.code, r.name),
            None => format!("{} {mark} **{}**\n", r.code, r.name),
        }
    }

    /// The summary as plain text: one line per boss with a link to it.
    pub fn summary(&self, lang: Lang) -> String {
        let mut out = self.title(lang);
        if self.bosses.is_empty() {
            out.push_str(if lang == Lang::De { "Noch keine Tries." } else { "No tries yet." });
        }
        for r in self.rows(lang) {
            out.push_str(&Self::text_line(&r));
        }
        truncate(out, MAX_CONTENT)
    }

    /// The summary as embeds, one per boss, each with the boss's card as a
    /// thumbnail. `new_thumb` = (boss index, filename) of a card uploaded
    /// with this edit. Returns (content, embeds, attachment ids to keep).
    pub fn summary_embeds(&self, lang: Lang, new_thumb: Option<(usize, &str)>) -> (String, serde_json::Value, Vec<String>) {
        let rows = self.rows(lang);
        let mut content = self.title(lang);
        let mut embeds = Vec::new();
        let mut keep = Vec::new();
        for (i, (r, b)) in rows.iter().zip(&self.bosses).enumerate() {
            if i >= MAX_EMBEDS {
                content.push_str(&Self::text_line(r));
                continue;
            }
            let mut e = serde_json::json!({
                "title": truncate(format!("{} {}", if r.killed { "✅" } else { "❌" }, r.name), 256),
                "description": r.code,
                "color": if r.killed { GREEN } else { RED },
            });
            if let Some(link) = &r.link {
                e["url"] = link.clone().into();
            }
            let file = match (new_thumb, &b.thumb) {
                (Some((bi, f)), _) if bi == i => Some(f.to_string()),
                (_, Some(t)) => {
                    keep.push(t.attachment_id.clone());
                    Some(t.filename.clone())
                }
                _ => None,
            };
            if let Some(f) = file {
                e["thumbnail"] = serde_json::json!({ "url": format!("attachment://{f}") });
            }
            embeds.push(e);
        }
        (truncate(content, MAX_CONTENT), embeds.into(), keep)
    }

    /// Edits the summary: embeds with thumbnails, or plain text if Discord
    /// rejects those. `card` is a new card of boss `bi` for its thumbnail.
    fn update_summary(
        &mut self,
        sink: &dyn Sink,
        bi: usize,
        card: Option<(u64, &[u8])>,
        lang: Lang,
    ) -> DiscordResult<()> {
        let Some(id) = self.summary_id.clone() else { return Ok(()) };
        let thread = self.thread_id.clone();
        let card = card.filter(|_| bi < MAX_EMBEDS);
        let filename = card.map(|(try_id, _)| format!("thumb-{try_id}.png"));
        let (content, embeds, keep) = self.summary_embeds(lang, filename.as_deref().map(|f| (bi, f)));
        let edit = Edit {
            content: &content,
            embeds: Some(embeds),
            keep: Some(keep.clone()),
            file: card.zip(filename.as_deref()).map(|((_, png), f)| Attachment {
                filename: f,
                content_type: "image/png",
                bytes: png,
            }),
        };
        match sink.edit(thread.as_deref(), &id, &edit) {
            Ok(m) => {
                // Discord lists an attachment an embed shows only in that
                // embed, so the ids to keep next time come from there.
                for (i, b) in self.bosses.iter_mut().take(MAX_EMBEDS).enumerate() {
                    if let Some(a) = m.embed_thumb(i) {
                        b.thumb = Some(Thumb { attachment_id: a.id, filename: a.filename });
                    } else if b.thumb.is_some() || (i == bi && filename.is_some()) {
                        log::warn!("discord: thumbnail of {} missing in the summary", b.name);
                        b.thumb = None;
                    }
                }
                Ok(())
            }
            // Deleted by hand; the cards are still there.
            Err(DiscordError::NotFound(_)) => {
                log::warn!("discord: summary message is gone");
                Ok(())
            }
            Err(e) => {
                log::warn!("discord: summary with thumbnails failed ({e}), using plain text");
                for b in &mut self.bosses {
                    b.thumb = None;
                }
                let text = self.summary(lang);
                let plain =
                    Edit { content: &text, embeds: Some(serde_json::json!([])), keep: Some(Vec::new()), file: None };
                match sink.edit(thread.as_deref(), &id, &plain) {
                    Ok(_) | Err(DiscordError::NotFound(_)) => Ok(()),
                    Err(e) => Err(e),
                }
            }
        }
    }

    /// The text under a card, e.g. `**Wipe 3** · 12.4 % · 20:41 · <link>`.
    fn try_line(p: &TryPost, nth: usize, lang: Lang) -> String {
        let what = if p.success {
            let n = if nth > 1 { format!(" {nth}") } else { String::new() };
            format!("**Kill{n}** · {}", fmt_duration(p.duration_ms))
        } else {
            format!("**Wipe {nth}** · {} %", fmt_hp(p.hp_left, lang))
        };
        let mut line = format!("{what} · {}", clock(p.start_ms));
        if let Some(url) = &p.share_url {
            line.push_str(&format!(" · <{url}>"));
        }
        line
    }

    /// Forget tonight's post (it was deleted), keeping the counts.
    fn forget_post(&mut self) {
        self.thread_id = None;
        self.flat = false;
        self.summary_id = None;
        for b in &mut self.bosses {
            b.header_id = None;
            b.thumb = None;
            for t in &mut b.tries {
                t.message_id = None;
            }
        }
    }

    /// Adds a try to tonight's post: creates the post, the boss header and
    /// the card message as needed, then updates the summary. `save` runs
    /// after every step that changed the log. `force` posts the image even
    /// when the try was only counted before (manual "post" button).
    pub fn record(
        &mut self,
        sink: &dyn Sink,
        webhook_id: &str,
        p: &TryPost,
        lang: Lang,
        save: &mut dyn FnMut(&DayLog),
    ) -> DiscordResult<Outcome> {
        let day = raid_day(p.start_ms).format("%Y-%m-%d").to_string();
        if self.webhook_id != webhook_id || self.day.as_str() < day.as_str() {
            *self = DayLog { webhook_id: webhook_id.into(), day: day.clone(), ..DayLog::default() };
            save(self);
        } else if self.day != day {
            // Manual post of an older night: that post is gone from our log.
            return Err(DiscordError::Other(match lang {
                Lang::De => "nur Tries des aktuellen Raidabends können gepostet werden".into(),
                Lang::En => "only tries of the current raid night can be posted".into(),
            }));
        }
        match self.record_in_post(sink, p, lang, save) {
            Err(DiscordError::NotFound(_)) if self.thread_id.is_some() || self.flat => {
                // Someone deleted tonight's post: start a new one.
                self.forget_post();
                save(self);
                self.record_in_post(sink, p, lang, save)
            }
            r => r,
        }
    }

    fn record_in_post(
        &mut self,
        sink: &dyn Sink,
        p: &TryPost,
        lang: Lang,
        save: &mut dyn FnMut(&DayLog),
    ) -> DiscordResult<Outcome> {
        let already = self.bosses.iter().flat_map(|b| &b.tries).find(|t| t.id == p.id);
        match already {
            Some(t) if t.message_id.is_some() || p.png.is_none() => return Ok(Outcome::Duplicate),
            _ => {}
        }

        if self.channel_id.is_none() && self.guild_id.is_none() {
            let info = sink.info()?;
            self.guild_id = info.guild_id;
            self.channel_id = info.channel_id;
            save(self);
        }

        // Tonight's post, with the summary as its first message.
        if !self.flat && self.thread_id.is_none() {
            let title = thread_title(raid_day(p.start_ms), lang);
            let summary = self.summary(lang);
            let m = sink.post(&NewMessage { content: &summary, thread_name: Some(&title), ..Default::default() })?;
            if self.channel_id.as_deref() == Some(m.channel_id.as_str()) {
                self.flat = true;
            } else {
                self.thread_id = Some(m.channel_id);
            }
            self.summary_id = Some(m.id);
            save(self);
        }
        let thread = self.thread_id.clone();

        let key = format!("{}|{}", p.boss, p.mode);
        let bi = match self.bosses.iter().position(|b| b.key == key) {
            Some(i) => i,
            None => {
                self.bosses.push(BossLog { key, name: boss_name(&p.boss, &p.mode), ..BossLog::default() });
                self.bosses.len() - 1
            }
        };
        let boss = &mut self.bosses[bi];
        let ti = match boss.tries.iter().position(|t| t.id == p.id) {
            Some(i) => i,
            None => {
                boss.tries.push(TryLog {
                    id: p.id,
                    success: p.success,
                    duration_ms: p.duration_ms,
                    hp_left: p.hp_left,
                    start_ms: p.start_ms,
                    message_id: None,
                });
                boss.tries.len() - 1
            }
        };
        // A new heading already shows this try's times; an existing one is
        // edited once the card is up.
        let new_heading = self.bosses[bi].header_id.is_none();
        if new_heading {
            let heading = self.bosses[bi].heading();
            let m = sink.post(&NewMessage { content: &heading, thread_id: thread.as_deref(), ..Default::default() })?;
            self.bosses[bi].header_id = Some(m.id);
            save(self);
        }
        let outcome = match &p.png {
            Some(png) => {
                let nth = self.bosses[bi].tries[..=ti].iter().filter(|t| t.success == p.success).count();
                let line = Self::try_line(p, nth, lang);
                let filename = format!("quagorious-{}.png", p.id);
                let m = sink.post(&NewMessage {
                    content: &line,
                    thread_id: thread.as_deref(),
                    file: Some(Attachment { filename: &filename, content_type: "image/png", bytes: png }),
                    ..Default::default()
                })?;
                self.bosses[bi].tries[ti].message_id = Some(m.id);
                Outcome::Posted
            }
            None => Outcome::Counted,
        };
        save(self);

        if !new_heading && let Some(h) = self.bosses[bi].header_id.clone() {
            let heading = self.bosses[bi].heading();
            match sink.edit(thread.as_deref(), &h, &Edit { content: &heading, ..Default::default() }) {
                // Deleted by hand; the cards are still there.
                Err(DiscordError::NotFound(_)) => log::warn!("discord: heading {h} is gone"),
                Err(e) => return Err(e),
                Ok(_) => {}
            }
        }
        let card = (outcome == Outcome::Posted).then_some(p.png.as_deref()).flatten().map(|png| (p.id, png));
        self.update_summary(sink, bi, card, lang)?;
        save(self);
        Ok(outcome)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    /// Records calls and answers like a forum channel (`channel` "9", guild "1").
    #[derive(Default)]
    struct Fake {
        calls: RefCell<Vec<String>>,
        next: RefCell<u64>,
        /// Answer like a text channel (thread title ignored).
        text_channel: bool,
        /// Threads that answer 404.
        deleted: RefCell<Vec<String>>,
        threads: RefCell<u64>,
        /// Answer 400 to edits with embeds.
        reject_embeds: bool,
        /// Attachment ids the fake has handed out, with their filenames.
        thumb_ids: RefCell<Vec<(String, String)>>,
        /// (content, embeds, keep) of the last summary edit.
        last_summary: RefCell<Option<(String, serde_json::Value, Option<Vec<String>>)>>,
    }

    impl Fake {
        fn id(&self) -> String {
            *self.next.borrow_mut() += 1;
            format!("m{}", self.next.borrow())
        }
    }

    impl Sink for Fake {
        fn info(&self) -> DiscordResult<WebhookInfo> {
            self.calls.borrow_mut().push("info".into());
            Ok(WebhookInfo { guild_id: Some("1".into()), channel_id: Some("9".into()) })
        }
        fn post(&self, m: &NewMessage) -> DiscordResult<Message> {
            if let Some(t) = m.thread_id
                && self.deleted.borrow().iter().any(|d| d == t)
            {
                return Err(DiscordError::NotFound("Unknown Channel".into()));
            }
            let what = match (m.thread_name, m.thread_id, &m.file) {
                (Some(n), ..) => format!("thread {n}"),
                (_, _, Some(f)) => format!("card {} in {:?}", f.filename, m.thread_id),
                _ => format!("text {} in {:?}", m.content, m.thread_id),
            };
            self.calls.borrow_mut().push(what);
            let channel = match (m.thread_name, m.thread_id) {
                (Some(_), _) if self.text_channel => "9".to_string(),
                (Some(_), _) => {
                    *self.threads.borrow_mut() += 1;
                    format!("t{}", self.threads.borrow())
                }
                (None, Some(t)) => t.to_string(),
                (None, None) => "9".to_string(),
            };
            Ok(Message { id: self.id(), channel_id: channel, attachments: Vec::new(), embeds: Vec::new() })
        }
        fn edit(&self, thread_id: Option<&str>, message_id: &str, e: &Edit) -> DiscordResult<Message> {
            if self.reject_embeds && e.embeds.as_ref().is_some_and(|v| v.as_array().is_some_and(|a| !a.is_empty())) {
                self.calls.borrow_mut().push(format!("rejected {message_id}"));
                return Err(DiscordError::Other("Discord HTTP 400: Invalid Form Body".into()));
            }
            let file = e.file.as_ref().map(|f| format!(" +{}", f.filename)).unwrap_or_default();
            self.calls.borrow_mut().push(format!("edit {message_id} in {thread_id:?}{file}"));
            if e.embeds.is_some() {
                *self.last_summary.borrow_mut() = Some((e.content.to_string(), e.embeds.clone().unwrap(), e.keep.clone()));
            }
            // Like Discord: images shown by embeds come back as CDN URLs in
            // the embeds, not in `attachments`. Kept ids survive, a new file
            // gets id 100 + try id.
            let mut embeds: Vec<serde_json::Value> = e.embeds.clone().and_then(|v| v.as_array().cloned()).unwrap_or_default();
            for em in &mut embeds {
                let Some(f) = em["thumbnail"]["url"].as_str().and_then(|u| u.strip_prefix("attachment://")).map(str::to_owned)
                else {
                    continue;
                };
                let new = e.file.as_ref().filter(|nf| nf.filename == f).map(|_| {
                    let n: u64 = f.trim_start_matches("thumb-").trim_end_matches(".png").parse().unwrap();
                    (100 + n).to_string()
                });
                let id = new.or_else(|| {
                    let known = self.thumb_ids.borrow();
                    known.iter().find(|(_, kf)| *kf == f).map(|(id, _)| id.clone())
                        .filter(|id| e.keep.iter().flatten().any(|k| k == id))
                });
                match id {
                    Some(id) => {
                        self.thumb_ids.borrow_mut().push((id.clone(), f.clone()));
                        em["thumbnail"]["url"] = format!("https://cdn.discordapp.com/attachments/9/{id}/{f}?ex=1").into();
                    }
                    None => {
                        em.as_object_mut().unwrap().remove("thumbnail");
                    }
                }
            }
            Ok(Message { id: message_id.into(), channel_id: thread_id.unwrap_or("9").into(), attachments: Vec::new(), embeds })
        }
    }

    fn at(day: u32, hour: u32, min: u32) -> i64 {
        Local.with_ymd_and_hms(2026, 11, day, hour, min, 0).single().unwrap().timestamp_millis()
    }

    fn try_(id: u64, boss: &str, success: bool, start_ms: i64) -> TryPost {
        TryPost {
            id,
            boss: boss.into(),
            mode: "cm".into(),
            success,
            duration_ms: 462_000,
            hp_left: if success { 0.0 } else { 12.4 },
            start_ms,
            share_url: Some(format!("https://quagorious.gg/share/s{id}")),
            png: Some(vec![1, 2, 3]),
        }
    }

    fn record(log: &mut DayLog, sink: &Fake, p: &TryPost) -> (DiscordResult<Outcome>, Vec<DayLog>) {
        let mut saves = Vec::new();
        let r = log.record(sink, "hook1", p, Lang::En, &mut |l| saves.push(l.clone()));
        (r, saves)
    }

    #[test]
    fn raid_night_runs_past_midnight() {
        let d = raid_day(at(25, 23, 50));
        assert_eq!(raid_day(at(26, 0, 30)), d);
        assert_eq!(raid_day(at(26, 5, 59)), d);
        assert_ne!(raid_day(at(26, 6, 0)), d);
        assert_eq!(thread_title(d, Lang::En), "1125_Wed");
        assert_eq!(thread_title(d, Lang::De), "1125_Mi");
    }

    #[test]
    fn builds_one_post_per_night_with_boss_headers() {
        let sink = Fake::default();
        let mut log = DayLog::default();
        assert_eq!(record(&mut log, &sink, &try_(1, "Harvest Temple", false, at(25, 20, 0))).0, Ok(Outcome::Posted));
        record(&mut log, &sink, &try_(2, "Harvest Temple", true, at(25, 20, 10))).0.unwrap();
        record(&mut log, &sink, &try_(3, "Dhuum", false, at(25, 21, 0))).0.unwrap();
        record(&mut log, &sink, &try_(4, "Cairn", true, at(25, 21, 30))).0.unwrap();
        sink.calls.borrow_mut().truncate(10);
        assert_eq!(
            *sink.calls.borrow(),
            [
                "info",
                "thread 1125_Wed",
                "text ## Harvest Temple CM · 20:00–20:07 in Some(\"t1\")",
                "card quagorious-1.png in Some(\"t1\")",
                "edit m1 in Some(\"t1\") +thumb-1.png",
                "card quagorious-2.png in Some(\"t1\")",
                "edit m2 in Some(\"t1\")",
                "edit m1 in Some(\"t1\") +thumb-2.png",
                "text ## Dhuum CM · 21:00–21:07 in Some(\"t1\")",
                "card quagorious-3.png in Some(\"t1\")",
            ]
        );
        // The heading is edited to span the boss's first start to last end.
        assert_eq!(log.bosses[0].heading(), "## Harvest Temple CM · 20:00–20:17");
        let s = log.summary(Lang::En);
        // Equal-width code spans, so the columns line up in Discord.
        assert_eq!(
            s.lines().skip(1).collect::<Vec<_>>(),
            [
                "`20:00–20:17  1 Wipe    Kill 7:42` ✅ [**Harvest Temple CM**](<https://discord.com/channels/1/t1/m2>)",
                "`21:00–21:07  1 Wipe  best 12.4 %` ❌ [**Dhuum CM**](<https://discord.com/channels/1/t1/m6>)",
                "`21:30–21:37            Kill 7:42` ✅ [**Cairn CM**](<https://discord.com/channels/1/t1/m8>)",
            ]
        );
    }

    #[test]
    fn resumes_after_a_crash_from_the_saved_log() {
        let sink = Fake::default();
        let mut log = DayLog::default();
        let (_, saves) = record(&mut log, &sink, &try_(1, "Harvest Temple", false, at(25, 20, 0)));
        // The game crashed; the addon starts again from the last save.
        let mut restored: DayLog = serde_json::from_str(&serde_json::to_string(saves.last().unwrap()).unwrap()).unwrap();
        sink.calls.borrow_mut().clear();
        record(&mut restored, &sink, &try_(2, "Harvest Temple", true, at(26, 0, 30))).0.unwrap();
        assert_eq!(
            *sink.calls.borrow(),
            ["card quagorious-2.png in Some(\"t1\")", "edit m2 in Some(\"t1\")", "edit m1 in Some(\"t1\") +thumb-2.png"]
        );
        // A crash between creating the post and saving the header still reuses the post.
        let mut half = saves.iter().find(|l| l.thread_id.is_some() && l.bosses.is_empty()).unwrap().clone();
        sink.calls.borrow_mut().clear();
        record(&mut half, &sink, &try_(5, "Harvest Temple", false, at(25, 20, 30))).0.unwrap();
        assert_eq!(sink.calls.borrow()[0], "text ## Harvest Temple CM · 20:30–20:37 in Some(\"t1\")");
    }

    #[test]
    fn skips_duplicates_and_posts_counted_tries_on_demand() {
        let sink = Fake::default();
        let mut log = DayLog::default();
        let p = try_(1, "Dhuum", false, at(25, 20, 0));
        record(&mut log, &sink, &p).0.unwrap();
        assert_eq!(record(&mut log, &sink, &p).0, Ok(Outcome::Duplicate));
        let counted = TryPost { png: None, ..try_(2, "Dhuum", false, at(25, 20, 5)) };
        assert_eq!(record(&mut log, &sink, &counted).0, Ok(Outcome::Counted));
        assert_eq!(record(&mut log, &sink, &counted).0, Ok(Outcome::Duplicate));
        // The "post" button sends the image of a try that was only counted.
        assert_eq!(record(&mut log, &sink, &try_(2, "Dhuum", false, at(25, 20, 5))).0, Ok(Outcome::Posted));
        assert_eq!(log.bosses[0].tries.len(), 2);
    }

    #[test]
    fn new_night_or_webhook_starts_a_new_post() {
        let sink = Fake::default();
        let mut log = DayLog::default();
        record(&mut log, &sink, &try_(1, "Dhuum", false, at(25, 20, 0))).0.unwrap();
        record(&mut log, &sink, &try_(2, "Dhuum", false, at(26, 20, 0))).0.unwrap();
        assert_eq!(log.thread_id.as_deref(), Some("t2"));
        assert_eq!(log.bosses[0].tries.len(), 1);
        // An older night can't be posted any more.
        assert!(record(&mut log, &sink, &try_(3, "Dhuum", false, at(25, 21, 0))).0.is_err());
        let mut saves = Vec::new();
        log.record(&sink, "hook2", &try_(4, "Dhuum", false, at(26, 21, 0)), Lang::En, &mut |l| saves.push(l.clone()))
            .unwrap();
        assert_eq!(log.webhook_id, "hook2");
        assert_eq!(log.thread_id.as_deref(), Some("t3"));
    }

    #[test]
    fn deleted_post_is_recreated() {
        let sink = Fake::default();
        let mut log = DayLog::default();
        record(&mut log, &sink, &try_(1, "Dhuum", false, at(25, 20, 0))).0.unwrap();
        sink.deleted.borrow_mut().push("t1".into());
        record(&mut log, &sink, &try_(2, "Dhuum", true, at(25, 20, 10))).0.unwrap();
        assert_eq!(log.thread_id.as_deref(), Some("t2"));
        let s = log.summary(Lang::En);
        assert!(s.contains("1 Wipe  Kill 7:42` ✅"), "counts survive: {s}");
    }

    #[test]
    fn text_channel_gets_everything_flat() {
        let sink = Fake { text_channel: true, ..Fake::default() };
        let mut log = DayLog::default();
        record(&mut log, &sink, &try_(1, "Dhuum", false, at(25, 20, 0))).0.unwrap();
        assert!(log.flat);
        assert_eq!(log.thread_id, None);
        assert_eq!(sink.calls.borrow()[2], "text ## Dhuum CM · 20:00–20:07 in None");
        assert!(log.summary(Lang::En).contains("https://discord.com/channels/1/9/m3"), "one try links its card");
    }

    #[test]
    fn summary_embeds_carry_each_boss_card_as_thumbnail() {
        let sink = Fake::default();
        let mut log = DayLog::default();
        record(&mut log, &sink, &try_(1, "Harvest Temple", false, at(25, 20, 0))).0.unwrap();
        record(&mut log, &sink, &try_(2, "Dhuum", true, at(25, 21, 0))).0.unwrap();
        record(&mut log, &sink, &try_(3, "Harvest Temple", true, at(25, 21, 30))).0.unwrap();
        let (content, embeds, keep) = sink.last_summary.borrow().clone().unwrap();
        assert_eq!(content, "**Raid night · Wed 2026-11-25**\n");
        let embeds = embeds.as_array().unwrap();
        assert_eq!(embeds.len(), 2);
        // Harvest Temple's thumbnail moved to its newest card; the old one is dropped.
        assert_eq!(embeds[0]["title"], "✅ Harvest Temple CM");
        assert_eq!(embeds[0]["thumbnail"]["url"], "attachment://thumb-3.png");
        assert_eq!(embeds[0]["url"], "https://discord.com/channels/1/t1/m2");
        assert_eq!(embeds[0]["color"], GREEN);
        assert_eq!(embeds[1]["thumbnail"]["url"], "attachment://thumb-2.png");
        assert_eq!(keep, Some(vec!["102".to_string()]), "Dhuum's image is kept, Harvest Temple's old one dropped");
        assert_eq!(log.bosses[0].thumb.as_ref().unwrap().attachment_id, "103");
        assert_eq!(log.bosses[1].thumb.as_ref().unwrap().attachment_id, "102");
        // Descriptions are equal-width code spans, so they line up.
        let widths: Vec<usize> = embeds.iter().map(|e| e["description"].as_str().unwrap().chars().count()).collect();
        assert_eq!(widths[0], widths[1]);
    }

    #[test]
    fn rejected_embeds_fall_back_to_plain_text() {
        let sink = Fake { reject_embeds: true, ..Fake::default() };
        let mut log = DayLog::default();
        assert_eq!(record(&mut log, &sink, &try_(1, "Dhuum", true, at(25, 20, 0))).0, Ok(Outcome::Posted));
        assert_eq!(sink.calls.borrow()[4..], ["rejected m1", "edit m1 in Some(\"t1\")"]);
        let (content, embeds, keep) = sink.last_summary.borrow().clone().unwrap();
        assert!(content.contains("✅ [**Dhuum CM**]"), "{content}");
        assert_eq!(embeds, serde_json::json!([]));
        assert_eq!(keep, Some(vec![]));
        assert_eq!(log.bosses[0].thumb, None);
    }
}
