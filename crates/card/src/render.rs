//! Software renderer for the share cards (tiny-skia shapes + ab_glyph text).
//!
//! Layout runs twice: once without a pixmap to measure the height, then for
//! real. All coordinates are in "units" (cards are 420 units wide) and scaled
//! by `scale` at draw time, so the PNG can be rendered at 2× for crisp pasting.

use std::collections::HashMap;

use ab_glyph::{Font, FontRef, GlyphId, PxScale, ScaleFont, point};
use tiny_skia::{
    Color, FillRule, GradientStop, LinearGradient, Paint, Path, PathBuilder, Pixmap, Point,
    PremultipliedColorU8, Rect, SpreadMode, Transform,
};

use crate::model::{Encounter, FullEncounter, Player};
use crate::text::{Lang, fmt_dec, fmt_duration, fmt_hp, fmt_int, mode_label, short_mechanic};

static REGULAR: &[u8] = include_bytes!("../assets/NotoSans-Regular.ttf");
static BOLD: &[u8] = include_bytes!("../assets/NotoSans-Bold.ttf");

pub const WIDTH: f32 = 420.0;
const PAD: f32 = 18.0;
const INNER: f32 = WIDTH - 2.0 * PAD;

/// Extra information that isn't part of the encounter response itself.
#[derive(Clone, Debug, Default)]
pub struct CardContext {
    /// Today's tries at the same boss + mode, oldest first.
    pub today: Vec<TodayChip>,
    /// Footer text on the right, e.g. a local date/time.
    pub stamp: String,
    /// Boss artwork for the header's top-right corner.
    pub boss_image: Option<BossImage>,
}

/// A decoded boss image.
#[derive(Clone, Debug)]
pub struct BossImage(Pixmap);

impl BossImage {
    pub fn from_png(bytes: &[u8]) -> Option<Self> {
        Pixmap::decode_png(bytes).ok().map(Self)
    }
}

#[derive(Clone, Debug)]
pub struct TodayChip {
    pub success: bool,
    pub boss_hp_left: f64,
    pub is_current: bool,
}

/// Which card to draw. See [`crate::variants`] for when each is available.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Variant {
    /// Result, DPS vs. your history, squad DPS, mistakes, today's tries.
    Overview,
    /// Phase timeline, where the try was lost, squad DPS, mistakes.
    Fight,
    /// Uptime of a boon you provided, per player (e.g. "alacrity").
    Boon(String),
}

impl Variant {
    pub fn label(&self, lang: Lang) -> String {
        match self {
            Variant::Overview => lang.variant_overview().into(),
            Variant::Fight => lang.fight_timeline().into(),
            Variant::Boon(b) => lang.boon_name(b),
        }
    }
}

/// A profession/spec icon reduced to a coverage mask, tinted at draw time.
#[derive(Clone, Debug)]
pub struct IconMask {
    pub w: u32,
    pub h: u32,
    pub mask: Vec<u8>,
}

impl IconMask {
    /// Uses alpha × brightness, so the icon's inner detail survives tinting.
    pub fn from_png(bytes: &[u8]) -> Option<Self> {
        let pm = Pixmap::decode_png(bytes).ok()?;
        let lum: Vec<f32> = pm
            .pixels()
            .iter()
            .map(|p| {
                let c = p.demultiply();
                (0.3 * c.red() as f32 + 0.59 * c.green() as f32 + 0.11 * c.blue() as f32) / 255.0
            })
            .collect();
        let max = lum.iter().cloned().fold(0.01, f32::max);
        let mask = pm
            .pixels()
            .iter()
            .zip(&lum)
            .map(|(p, l)| (p.alpha() as f32 * (0.35 + 0.65 * (l / max))).round() as u8)
            .collect();
        Some(Self { w: pm.width(), h: pm.height(), mask })
    }
}

/// Elite spec (or base profession) → its profession and icon.
#[derive(Clone, Debug, Default)]
pub struct SpecInfo {
    pub profession: String,
    pub icon: Option<IconMask>,
}

pub type Specs = HashMap<String, SpecInfo>;

/// Everything a card is drawn from.
pub struct CardInput<'a> {
    pub enc: &'a Encounter,
    /// Full analysis, when the try's share token is known.
    pub full: Option<&'a FullEncounter>,
    pub ctx: &'a CardContext,
    pub specs: &'a Specs,
    pub lang: Lang,
}

impl CardInput<'_> {
    fn me(&self) -> Option<&Player> {
        self.full?.players.iter().find(|p| p.account == self.enc.account)
    }
}

/// A rendered card: straight (non-premultiplied) RGBA + PNG bytes.
pub struct RenderedCard {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    pub png: Vec<u8>,
}

// --- palette --------------------------------------------------------------
type Rgba = [u8; 4];
/// Card background, for consumers that have to flatten the rounded corners.
pub const MATTE: [u8; 3] = [24, 22, 30];
const BG: Rgba = [MATTE[0], MATTE[1], MATTE[2], 255];
const PANEL: Rgba = [36, 33, 44, 255];
const TRACK: Rgba = [52, 48, 62, 255];
const TEXT: Rgba = [240, 238, 245, 255];
const MUTED: Rgba = [150, 145, 165, 255];
const DIM: Rgba = [110, 105, 125, 255];
const GREEN: Rgba = [74, 222, 128, 255];
const RED: Rgba = [248, 113, 113, 255];
const ORANGE: Rgba = [251, 191, 36, 255];
const PURPLE: Rgba = [167, 139, 250, 255];
const WHITE_SOFT: Rgba = [255, 255, 255, 215];
const PHASE_A: Rgba = [64, 60, 78, 255];
const PHASE_B: Rgba = [92, 66, 160, 255];
const LOST: Rgba = [248, 113, 113, 70];

/// The usual GW2 profession colours.
fn profession_color(profession: &str) -> Rgba {
    match profession {
        "Guardian" => [114, 193, 217, 255],
        "Revenant" => [209, 110, 90, 255],
        "Warrior" => [255, 209, 102, 255],
        "Engineer" => [208, 156, 89, 255],
        "Ranger" => [140, 220, 130, 255],
        "Thief" => [192, 143, 149, 255],
        "Elementalist" => [246, 138, 135, 255],
        "Mesmer" => [182, 121, 213, 255],
        "Necromancer" => [82, 167, 111, 255],
        _ => [140, 136, 150, 255],
    }
}

pub fn render(inp: &CardInput, variant: &Variant, scale: f32) -> RenderedCard {
    let fonts = Fonts::load();
    // Pass 1: measure.
    let mut p = Painter { pm: None, s: scale, fonts: &fonts };
    let h = layout(&mut p, inp, variant, 0.0);
    // Pass 2: draw.
    let (pw, ph) = ((WIDTH * scale).ceil() as u32, (h * scale).ceil() as u32);
    p.pm = Some(Pixmap::new(pw, ph).expect("card size"));
    layout(&mut p, inp, variant, h);
    let pm = p.pm.take().unwrap();
    let png = pm.encode_png().unwrap_or_default();
    let mut rgba = Vec::with_capacity(pm.data().len());
    for px in pm.pixels() {
        let c = px.demultiply();
        rgba.extend_from_slice(&[c.red(), c.green(), c.blue(), c.alpha()]);
    }
    RenderedCard { width: pw, height: ph, rgba, png }
}

/// Lays out (and, with a pixmap, draws) a card. Returns the total height.
/// `total_h` is only used for the background in the drawing pass.
fn layout(p: &mut Painter, inp: &CardInput, variant: &Variant, total_h: f32) -> f32 {
    if total_h > 0.0 {
        p.rrect(0.0, 0.0, WIDTH, total_h, 14.0, BG);
    }
    let mut y = header(p, inp);
    y = main_number(p, inp, y);
    match variant {
        Variant::Overview => {
            y = reference(p, inp, y);
            if let Some(full) = inp.full {
                y = squad_dps(p, inp, full, y);
            } else {
                y = boon_summary(p, inp, y);
            }
            y = mistakes(p, inp, y, false);
            y = today(p, inp, y);
        }
        Variant::Fight => {
            if let Some(full) = inp.full {
                y = timeline(p, inp, full, y);
                y = squad_dps(p, inp, full, y);
            }
            y = mistakes(p, inp, y, true);
        }
        Variant::Boon(boon) => {
            if let Some(full) = inp.full {
                y = boon_players(p, inp, full, boon, y);
            }
            y = mistakes(p, inp, y, false);
            y = today(p, inp, y);
        }
    }
    footer(p, inp, y)
}

// --------------------------------------------------------------------------
// Sections. Each takes the current y and returns the y below itself.
// --------------------------------------------------------------------------

fn caption(p: &mut Painter, y: f32, left: &str, right: &str, right_col: Rgba) {
    let rw = p.measure(10.5, true, right);
    p.text_fit(PAD, y, 10.5, true, MUTED, &left.to_uppercase(), INNER - rw - 12.0);
    p.text_right(WIDTH - PAD, y, 10.5, true, right_col, right);
}

fn header(p: &mut Painter, inp: &CardInput) -> f32 {
    let e = inp.enc;
    let lang = inp.lang;
    let header_h = 74.0;
    let (c0, c1) = if e.success {
        ([112, 64, 214, 255], [138, 44, 112, 255])
    } else {
        ([190, 42, 48, 255], [118, 30, 62, 255])
    };
    p.gradient_rrect(0.0, 0.0, WIDTH, header_h + 14.0, 14.0, c0, c1);
    p.rect(0.0, header_h, WIDTH, 14.0, BG);

    let (word, rest) = if e.success {
        ("KILL", fmt_duration(e.duration_ms))
    } else {
        ("WIPE", format!("{} %", fmt_hp(e.boss_hp_left, lang)))
    };
    let mut text_w = INNER;
    if let Some(img) = &inp.ctx.boss_image {
        let size = header_h;
        p.image(&img.0, WIDTH - size, 0.0, size, size, 34.0, 14.0);
        text_w = INNER - size + 12.0;
    }
    let x = PAD + p.text(PAD, 12.0, 27.0, true, TEXT, word);
    p.text(x + 8.0, 12.0, 27.0, false, TEXT, &rest);

    let mut sub = vec![e.boss.clone()];
    if let Some(m) = mode_label(&e.mode) {
        sub.push(m.into());
    }
    if e.try_number > 0 {
        sub.push(format!("Try {}", e.try_number));
    }
    if !e.success && e.duration_ms > 0 {
        sub.push(fmt_duration(e.duration_ms));
    }
    p.text_fit(PAD, 49.0, 13.0, false, WHITE_SOFT, &sub.join(" · "), text_w);
    header_h + 14.0
}

fn main_number(p: &mut Painter, inp: &CardInput, y: f32) -> f32 {
    let e = inp.enc;
    let lang = inp.lang;
    let dps = fmt_int(e.target_dps, lang);
    let w = p.text(PAD, y, 34.0, true, TEXT, &dps);
    p.text(PAD + w + 7.0, y + 21.0, 13.0, false, MUTED, "DPS");
    let mut who = format!("{} · {}", e.profession, lang.role(&e.role));
    if e.commander {
        who = format!("{who} · {}", lang.commander());
    }
    let ww = p.measure(13.0, false, &who);
    let icon_w = if inp.specs.get(&e.profession).and_then(|s| s.icon.as_ref()).is_some() { 22.0 } else { 0.0 };
    if icon_w > 0.0 {
        p.spec_icon(inp.specs, &e.profession, WIDTH - PAD - ww - icon_w, y + 20.0, 16.0);
    }
    p.text_right(WIDTH - PAD, y + 19.0, 13.0, false, TEXT, &who);
    y + 54.0
}

/// Your earlier tries on this boss with the same spec + role.
fn reference(p: &mut Painter, inp: &CardInput, mut y: f32) -> f32 {
    let e = inp.enc;
    let lang = inp.lang;
    let Some(r) = e.reference.as_ref().filter(|r| r.tries > 0) else { return y };
    let best = format!("Best {}", fmt_int(r.best_dps, lang));
    caption(p, y, &lang.your_tries_as(&e.profession, &lang.role(&e.role)), &best, GREEN);
    y += 20.0;

    // Bar: 0 .. max * 1.12, avg tick, best tick, you dot.
    let bar_h = 30.0;
    p.rrect(PAD, y, INNER, bar_h, 7.0, PANEL);
    let max = r.best_dps.max(e.target_dps).max(1.0) * 1.12;
    let tx = |v: f64| PAD + 14.0 + (INNER - 28.0) * (v / max).clamp(0.0, 1.0) as f32;
    let mid = y + bar_h / 2.0;
    p.rrect(PAD + 14.0, mid - 1.5, INNER - 28.0, 3.0, 1.5, TRACK);
    p.rrect(tx(r.avg_dps) - 1.0, mid - 7.0, 2.0, 14.0, 1.0, MUTED);
    p.rrect(tx(r.best_dps) - 1.5, mid - 9.0, 3.0, 18.0, 1.5, GREEN);
    p.circle(tx(e.target_dps), mid, 7.5, TEXT);
    p.circle(tx(e.target_dps), mid, 6.0, PURPLE);
    y += bar_h + 6.0;

    let delta = if r.avg_dps > 0.0 { (e.target_dps / r.avg_dps - 1.0) * 100.0 } else { 0.0 };
    let col = if delta >= 0.0 { GREEN } else { ORANGE };
    p.text(PAD, y, 11.5, false, MUTED, &format!("Ø {}", fmt_int(r.avg_dps, lang)));
    p.text_right(WIDTH - PAD, y, 11.5, false, col, &lang.vs_avg(delta, r.tries));
    y + 26.0
}

/// Squad-wide boon uptime from the module API (used when no full analysis).
fn boon_summary(p: &mut Painter, inp: &CardInput, mut y: f32) -> f32 {
    let lang = inp.lang;
    let Some(b) = inp.enc.boons.as_ref() else { return y };
    caption(p, y, &lang.your_boon(&b.boon), &lang.squad_uptime(b.squad_uptime), MUTED);
    y += 20.0;
    let col = uptime_color(b.squad_uptime);
    p.rrect(PAD, y, INNER, 8.0, 4.0, TRACK);
    p.rrect(PAD, y, INNER * (b.squad_uptime / 100.0).clamp(0.0, 1.0) as f32, 8.0, 4.0, col);
    y += 15.0;
    let gaps: Vec<String> = b
        .gaps
        .iter()
        .filter(|g| g.active_ms >= 100)
        .map(|g| format!("{} {} s", lang.gap_cause(&g.cause), fmt_dec(g.active_ms as f64 / 1000.0, 1, lang)))
        .collect();
    if !gaps.is_empty() {
        let line = format!("{}: {}", lang.gaps(), gaps.join(" · "));
        y += p.wrapped(PAD, y, 11.5, MUTED, &line, INNER);
    }
    y + 12.0
}

/// Bars of every player's DPS, sorted, with spec icons; you highlighted.
fn squad_dps(p: &mut Painter, inp: &CardInput, full: &FullEncounter, mut y: f32) -> f32 {
    let lang = inp.lang;
    let mut players: Vec<&Player> = full.players.iter().collect();
    if players.is_empty() {
        return y;
    }
    players.sort_by(|a, b| b.target_dps.total_cmp(&a.target_dps));

    let me = inp.me();
    let rank = me.and_then(|me| {
        let dps_only = me.role == "DPS";
        let pool: Vec<&&Player> = players.iter().filter(|p| !dps_only || p.role == "DPS").collect();
        let pos = pool.iter().position(|p| p.account == me.account)? + 1;
        Some(lang.rank(pos, pool.len(), dps_only))
    });
    // Only the bars are capped; the rank above counts the whole squad.
    players.truncate(12);
    caption(p, y, lang.squad_dps(), rank.as_deref().unwrap_or(""), MUTED);
    y += 22.0;

    let n = players.len() as f32;
    let gap = 6.0;
    let bw = (INNER - gap * (n - 1.0)) / n;
    let max_h = 56.0;
    let icon = 16.0;
    let max = players[0].target_dps.max(1.0);
    for (i, pl) in players.iter().enumerate() {
        let x = PAD + i as f32 * (bw + gap);
        let is_me = me.is_some_and(|m| m.account == pl.account);
        if is_me {
            p.rrect(x - 3.0, y - 4.0, bw + 6.0, max_h + icon + 15.0, 6.0, TEXT);
            p.rrect(x - 1.5, y - 2.5, bw + 3.0, max_h + icon + 12.0, 5.0, PANEL);
        }
        let h = (max_h * (pl.target_dps / max) as f32).max(3.0);
        let prof = inp.specs.get(&pl.profession).map(|s| s.profession.as_str()).unwrap_or(&pl.profession);
        let mut col = profession_color(prof);
        if !is_me && me.is_some() {
            col[3] = 200;
        }
        p.rrect(x, y + max_h - h, bw, h, 3.0, col);
        p.spec_icon(inp.specs, &pl.profession, x + bw / 2.0 - icon / 2.0, y + max_h + 5.0, icon);
    }
    y + max_h + icon + 22.0
}

/// Phases as a bar, the point the try was lost, and who started it.
fn timeline(p: &mut Painter, inp: &CardInput, full: &FullEncounter, mut y: f32) -> f32 {
    let lang = inp.lang;
    let h = &full.encounter;
    let total = h.duration_ms.max(inp.enc.duration_ms).max(1) as f32;
    let lost_at = h.lost_at_ms.filter(|_| !inp.enc.success);
    let right = lost_at.map(|ms| lang.lost_from(&fmt_duration(ms))).unwrap_or_default();
    caption(p, y, lang.fight_timeline(), &right, RED);
    y += 20.0;

    let bar_h = 24.0;
    let tx = |ms: u64| PAD + INNER * (ms as f32 / total).clamp(0.0, 1.0);
    p.rrect(PAD, y, INNER, bar_h, 5.0, PANEL);
    // Leave out sub-phases nested inside a bigger one.
    let mut phases: Vec<_> = h
        .phases
        .iter()
        .filter(|a| {
            !h.phases.iter().any(|b| {
                !std::ptr::eq(*a, b) && b.start <= a.start && b.end >= a.end && (b.end - b.start) > (a.end - a.start)
            })
        })
        .collect();
    phases.sort_by_key(|ph| ph.start);
    for (i, ph) in phases.iter().enumerate() {
        let (x0, x1) = (tx(ph.start), tx(ph.end));
        let w = (x1 - x0 - 1.0).max(1.0);
        p.rrect(x0 + 0.5, y + 1.0, w, bar_h - 2.0, 3.0, if i % 2 == 0 { PHASE_A } else { PHASE_B });
        let tw = p.measure(10.0, false, &ph.name);
        if tw <= w - 6.0 {
            p.text(x0 + (w - tw) / 2.0, y + 5.5, 10.0, false, TEXT, &ph.name);
        } else {
            p.text_fit(x0 + 3.0, y + 5.5, 10.0, false, TEXT, &ph.name, w - 4.0);
        }
    }
    if let Some(ms) = lost_at {
        let x = tx(ms);
        p.rrect(x, y, WIDTH - PAD - x, bar_h, 0.0, LOST);
        p.rect(x - 1.0, y - 3.0, 2.0, bar_h + 6.0, RED);
    }
    y += bar_h + 5.0;

    // Minute ticks, at most ~6 labels.
    let minutes = total / 60_000.0;
    let step = [1.0, 2.0, 5.0, 10.0, 15.0].into_iter().find(|s| minutes / s <= 6.0).unwrap_or(30.0);
    let mut m = 0.0;
    while m <= minutes {
        let x = tx((m * 60_000.0) as u64);
        let label = format!("{}:00", m as u32);
        let w = p.measure(10.0, false, &label);
        let lx = if m == 0.0 { x } else { (x - w / 2.0).min(WIDTH - PAD - w) };
        p.text(lx, y, 10.0, false, DIM, &label);
        m += step;
    }
    y += 22.0;

    if lost_at.is_some() {
        if let Some(by) = &h.lost_by {
            let name = full
                .players
                .iter()
                .find(|pl| &pl.account == by)
                .map(|pl| pl.display_name().to_string())
                .unwrap_or_else(|| by.clone());
            let mut x = PAD;
            x += p.text(x, y, 12.5, false, MUTED, lang.first());
            x += p.text(x, y, 12.5, true, RED, &name);
            p.text_fit(x, y, 12.5, false, MUTED, &lang.then_deaths(h.lost_deaths), WIDTH - PAD - x);
            y += 19.0;
        }
        p.text_fit(PAD, y, 11.5, false, DIM, &lang.before_that(h.deaths, h.downs), INNER);
        y += 26.0;
    } else {
        y += 4.0;
    }
    y
}

/// Uptime of `boon` on everyone in your subgroup (your party), then you.
fn boon_players(p: &mut Painter, inp: &CardInput, full: &FullEncounter, boon: &str, mut y: f32) -> f32 {
    let lang = inp.lang;
    let me = inp.enc.account.as_str();
    let total = full.encounter.duration_ms.max(1) as f64;
    let uptime = |pl: &Player| -> f64 {
        pl.uptime_weighted.get(boon).copied().flatten().unwrap_or_else(|| {
            let gap = pl.boon_gaps.get(boon).map(|g| g.gap_ms).unwrap_or(0) as f64;
            (100.0 - gap / total * 100.0).clamp(0.0, 100.0)
        })
    };
    // A party member out of range for at least half the fight is away on
    // purpose (kiter, other side of the arena), not a gap in your boon:
    // listed, but not averaged.
    let far_away = |pl: &Player| -> bool {
        let oor: u64 = pl
            .boon_gaps
            .get(boon)
            .map(|g| g.gaps.iter().filter(|x| x.cause == "outOfRange").map(|x| x.active_ms).sum())
            .unwrap_or(0);
        oor as f64 >= total * 0.5
    };
    // Your party: same subgroup, same boons.
    let others = crate::party(inp.enc, full);
    let (mut rows, mut away): (Vec<&Player>, Vec<&Player>) = others.into_iter().partition(|pl| !far_away(pl));
    rows.sort_by(|a, b| uptime(a).total_cmp(&uptime(b)));
    let counted = rows.len();
    let avg = if counted > 0 { rows.iter().map(|pl| uptime(pl)).sum::<f64>() / counted as f64 } else { 0.0 };
    away.sort_by(|a, b| uptime(a).total_cmp(&uptime(b)));
    rows.extend(away.iter().copied());
    if let Some(mine) = inp.me() {
        rows.push(mine);
    }

    caption(p, y, &lang.your_boon(boon), &lang.boon_avg(avg, counted), MUTED);
    y += 24.0;
    let name_w = 150.0;
    let bar_x = PAD + 22.0 + name_w + 8.0;
    let bar_w = WIDTH - PAD - 52.0 - bar_x;
    for pl in rows {
        let up = uptime(pl);
        let excluded = pl.account != me && far_away(pl);
        let col = if excluded { DIM } else { uptime_color(up) };
        p.spec_icon(inp.specs, &pl.profession, PAD, y + 1.0, 16.0);
        let name = if pl.account == me { lang.you().to_string() } else { pl.display_name().to_string() };
        let name_col = if excluded { MUTED } else { TEXT };
        p.text_fit(PAD + 22.0, y, 13.0, pl.account == me, name_col, &name, name_w);
        p.rrect(bar_x, y + 6.0, bar_w, 7.0, 3.5, TRACK);
        p.rrect(bar_x, y + 6.0, bar_w * (up / 100.0).clamp(0.0, 1.0) as f32, 7.0, 3.5, col);
        p.text_right(WIDTH - PAD, y, 12.5, true, col, &format!("{} %", fmt_dec(up, 1, lang)));
        y += 22.0;

        // Main gap causes, when there's something worth mentioning.
        if up < 99.5 {
            let mut by_cause: Vec<(String, u64)> = Vec::new();
            for g in pl.boon_gaps.get(boon).map(|g| g.gaps.as_slice()).unwrap_or(&[]) {
                match by_cause.iter_mut().find(|(c, _)| *c == g.cause) {
                    Some((_, ms)) => *ms += g.active_ms,
                    None => by_cause.push((g.cause.clone(), g.active_ms)),
                }
            }
            by_cause.sort_by_key(|(_, ms)| std::cmp::Reverse(*ms));
            let parts: Vec<String> = by_cause
                .iter()
                .filter(|(_, ms)| *ms >= 500)
                .take(2)
                .map(|(c, ms)| format!("{} {} s", lang.gap_cause(c), fmt_dec(*ms as f64 / 1000.0, 1, lang)))
                .collect();
            if !parts.is_empty() {
                let mut line = format!("{}: {}", lang.gaps(), parts.join(" · "));
                if excluded {
                    line = format!("{line} · {}", lang.not_in_avg());
                }
                p.text_fit(PAD + 22.0, y - 3.0, 11.0, false, DIM, &line, INNER - 22.0);
                y += 17.0;
            }
        }
        y += 4.0;
    }
    y + 10.0
}

/// Mistakes and contributions side by side. In `fight` mode on a wipe your
/// own downs/deaths are those before the loss.
fn mistakes(p: &mut Painter, inp: &CardInput, mut y: f32, fight: bool) -> f32 {
    let e = inp.enc;
    let lang = inp.lang;
    let col_w = (INNER - 16.0) / 2.0;
    let left = PAD;
    let right = PAD + col_w + 16.0;

    let mut fails: Vec<_> = e.mechanics.iter().filter(|m| m.fails > 0).collect();
    fails.sort_by_key(|m| std::cmp::Reverse(m.fails));
    let mut tasks: Vec<_> = e.mechanics.iter().filter(|m| m.tasks > 0).collect();
    tasks.sort_by_key(|m| std::cmp::Reverse(m.tasks));

    let fail_col = if e.fail_points == 0 { GREEN } else { RED };
    let w = p.text(left, y, 17.0, true, fail_col, &e.fail_points.to_string());
    p.text(left + w + 6.0, y + 3.0, 13.0, false, TEXT, lang.errors(e.fail_points));
    let task_col = if e.tasks > 0 { GREEN } else { MUTED };
    let w = p.text(right, y, 17.0, true, task_col, &e.tasks.to_string());
    p.text(right + w + 6.0, y + 3.0, 13.0, false, TEXT, lang.contributions(e.tasks));
    y += 28.0;

    const MAX_LINES: usize = 5;
    let label = |m: &crate::model::Mechanic| -> String {
        short_mechanic(if m.text.is_empty() { &m.name } else { &m.text }).to_string()
    };
    let mut ly = y;
    for m in fails.iter().take(MAX_LINES) {
        p.text_fit(left, ly, 12.5, false, MUTED, &format!("{}× {}", m.fails, label(m)), col_w);
        ly += 19.0;
    }
    if fails.len() > MAX_LINES {
        p.text(left, ly, 11.5, false, DIM, &format!("+{} …", fails.len() - MAX_LINES));
        ly += 19.0;
    }
    let mut ry = y;
    for m in tasks.iter().take(MAX_LINES - 1) {
        p.text_fit(right, ry, 12.5, false, MUTED, &format!("{}× {}", m.tasks, label(m)), col_w);
        ry += 19.0;
    }
    let wiped = fight && !e.success && inp.full.is_some_and(|f| f.encounter.lost_at_ms.is_some());
    let (downs, deaths) = match inp.me().filter(|_| wiped) {
        Some(me) => (me.downs, me.deaths),
        None => (e.downs, e.deaths),
    };
    let (dd, dd_col) = if downs == 0 && deaths == 0 {
        (lang.no_downs_deaths().to_string(), GREEN)
    } else if wiped {
        (lang.before_loss(downs, deaths), ORANGE)
    } else {
        (lang.downs_deaths(downs, deaths), ORANGE)
    };
    // Wrapped lines are 18.75 high; the rows on the left step by 19.
    ry += p.wrapped(right, ry, 12.5, dd_col, &dd, col_w) + 0.25;
    ly.max(ry) + 12.0
}

fn today(p: &mut Painter, inp: &CardInput, mut y: f32) -> f32 {
    let lang = inp.lang;
    let chips = &inp.ctx.today;
    if chips.is_empty() {
        return y;
    }
    caption(p, y, lang.today_at_boss(), &lang.n_tries(chips.len()), MUTED);
    y += 20.0;
    const MAX_CHIPS: usize = 5;
    let skip = chips.len().saturating_sub(MAX_CHIPS);
    let gap = 7.0;
    let chip_w = (INNER - gap * (MAX_CHIPS as f32 - 1.0)) / MAX_CHIPS as f32;
    let chip_h = 38.0;
    for (i, chip) in chips.iter().enumerate().skip(skip) {
        let n = i + 1;
        let x = PAD + (i - skip) as f32 * (chip_w + gap);
        let accent = if chip.success { GREEN } else { RED };
        if chip.is_current {
            p.rrect(x - 1.5, y - 1.5, chip_w + 3.0, chip_h + 3.0, 7.5, accent);
        }
        p.rrect(x, y, chip_w, chip_h, 6.0, PANEL);
        let label = if chip.success {
            format!("{n} · Kill")
        } else {
            format!("{n} · {} %", fmt_hp(chip.boss_hp_left, lang))
        };
        let col = if chip.is_current { TEXT } else { MUTED };
        p.text_fit(x + 8.0, y + 6.0, 11.5, chip.is_current, col, &label, chip_w - 12.0);
        let frac = if chip.success { 1.0 } else { (chip.boss_hp_left / 100.0).clamp(0.03, 1.0) };
        p.rrect(x + 8.0, y + chip_h - 10.0, chip_w - 16.0, 4.0, 2.0, TRACK);
        p.rrect(x + 8.0, y + chip_h - 10.0, (chip_w - 16.0) * frac as f32, 4.0, 2.0, accent);
    }
    y + chip_h + 16.0
}

fn footer(p: &mut Painter, inp: &CardInput, mut y: f32) -> f32 {
    p.rect(PAD, y, INNER, 1.0, TRACK);
    y += 10.0;
    p.text(PAD, y, 11.5, true, PURPLE, "quagorious.gg");
    if !inp.ctx.stamp.is_empty() {
        p.text_right(WIDTH - PAD, y, 11.5, false, DIM, &inp.ctx.stamp);
    }
    y + 28.0
}

fn uptime_color(pct: f64) -> Rgba {
    if pct >= 98.0 {
        GREEN
    } else if pct >= 85.0 {
        ORANGE
    } else {
        RED
    }
}

// --------------------------------------------------------------------------
// Drawing primitives
// --------------------------------------------------------------------------

struct Fonts {
    regular: FontRef<'static>,
    bold: FontRef<'static>,
}

impl Fonts {
    fn load() -> Self {
        Self {
            regular: FontRef::try_from_slice(REGULAR).expect("embedded font"),
            bold: FontRef::try_from_slice(BOLD).expect("embedded font"),
        }
    }
}

struct Painter<'f> {
    pm: Option<Pixmap>,
    s: f32,
    fonts: &'f Fonts,
}

fn paint(c: Rgba) -> Paint<'static> {
    let mut p = Paint::default();
    p.set_color_rgba8(c[0], c[1], c[2], c[3]);
    p.anti_alias = true;
    p
}

fn rrect_path(x: f32, y: f32, w: f32, h: f32, r: f32) -> Option<Path> {
    let r = r.min(w / 2.0).min(h / 2.0).max(0.0);
    if w <= 0.0 || h <= 0.0 {
        return None;
    }
    if r == 0.0 {
        return Some(PathBuilder::from_rect(Rect::from_xywh(x, y, w, h)?));
    }
    // Cubic approximation of a quarter circle.
    let k = r * 0.552_284_8;
    let mut pb = PathBuilder::new();
    pb.move_to(x + r, y);
    pb.line_to(x + w - r, y);
    pb.cubic_to(x + w - r + k, y, x + w, y + r - k, x + w, y + r);
    pb.line_to(x + w, y + h - r);
    pb.cubic_to(x + w, y + h - r + k, x + w - r + k, y + h, x + w - r, y + h);
    pb.line_to(x + r, y + h);
    pb.cubic_to(x + r - k, y + h, x, y + h - r + k, x, y + h - r);
    pb.line_to(x, y + r);
    pb.cubic_to(x, y + r - k, x + r - k, y, x + r, y);
    pb.close();
    pb.finish()
}

impl Painter<'_> {
    fn font(&self, bold: bool) -> &FontRef<'static> {
        if bold { &self.fonts.bold } else { &self.fonts.regular }
    }

    fn rect(&mut self, x: f32, y: f32, w: f32, h: f32, c: Rgba) {
        self.rrect(x, y, w, h, 0.0, c);
    }

    fn rrect(&mut self, x: f32, y: f32, w: f32, h: f32, r: f32, c: Rgba) {
        let s = self.s;
        let Some(pm) = self.pm.as_mut() else { return };
        if let Some(path) = rrect_path(x * s, y * s, w * s, h * s, r * s) {
            pm.fill_path(&path, &paint(c), FillRule::Winding, Transform::identity(), None);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn gradient_rrect(&mut self, x: f32, y: f32, w: f32, h: f32, r: f32, a: Rgba, b: Rgba) {
        let s = self.s;
        let Some(pm) = self.pm.as_mut() else { return };
        let Some(path) = rrect_path(x * s, y * s, w * s, h * s, r * s) else { return };
        let shader = LinearGradient::new(
            Point::from_xy(x * s, y * s),
            Point::from_xy((x + w) * s, (y + h) * s),
            vec![
                GradientStop::new(0.0, Color::from_rgba8(a[0], a[1], a[2], a[3])),
                GradientStop::new(1.0, Color::from_rgba8(b[0], b[1], b[2], b[3])),
            ],
            SpreadMode::Pad,
            Transform::identity(),
        );
        if let Some(shader) = shader {
            let p = Paint { shader, anti_alias: true, ..Paint::default() };
            pm.fill_path(&path, &p, FillRule::Winding, Transform::identity(), None);
        }
    }

    fn circle(&mut self, cx: f32, cy: f32, r: f32, c: Rgba) {
        let s = self.s;
        let Some(pm) = self.pm.as_mut() else { return };
        if let Some(path) = PathBuilder::from_circle(cx * s, cy * s, r * s) {
            pm.fill_path(&path, &paint(c), FillRule::Winding, Transform::identity(), None);
        }
    }

    /// Advance width of `text` in units.
    fn measure(&self, size: f32, bold: bool, text: &str) -> f32 {
        let font = self.font(bold).as_scaled(PxScale::from(size * 1.36));
        let mut w = 0.0;
        let mut prev: Option<GlyphId> = None;
        for ch in text.chars() {
            let id = font.glyph_id(ch);
            if let Some(p) = prev {
                w += font.kern(p, id);
            }
            w += font.h_advance(id);
            prev = Some(id);
        }
        w
    }

    /// Draws text with its top at `y`; returns its width in units.
    fn text(&mut self, x: f32, y: f32, size: f32, bold: bool, c: Rgba, text: &str) -> f32 {
        let width = self.measure(size, bold, text);
        let s = self.s;
        let fonts = self.fonts;
        let font = if bold { &fonts.bold } else { &fonts.regular };
        let Some(pm) = self.pm.as_mut() else { return width };
        let scale = PxScale::from(size * 1.36 * s);
        let sf = font.as_scaled(scale);
        let baseline = (y + size * 0.08) * s + sf.ascent();
        let mut caret = x * s;
        let mut prev: Option<GlyphId> = None;
        let (pw, ph) = (pm.width() as i32, pm.height() as i32);
        let pixels = pm.pixels_mut();
        for ch in text.chars() {
            let id = sf.glyph_id(ch);
            if let Some(p) = prev {
                caret += sf.kern(p, id);
            }
            let glyph = id.with_scale_and_position(scale, point(caret, baseline));
            if let Some(og) = font.outline_glyph(glyph) {
                let b = og.px_bounds();
                og.draw(|gx, gy, cov| {
                    let px = b.min.x as i32 + gx as i32;
                    let py = b.min.y as i32 + gy as i32;
                    if px < 0 || py < 0 || px >= pw || py >= ph {
                        return;
                    }
                    let i = (py * pw + px) as usize;
                    pixels[i] = blend(pixels[i], c, cov);
                });
            }
            caret += sf.h_advance(id);
            prev = Some(id);
        }
        width
    }

    /// Draws `img` into the given box (bilinear), fading in over `fade` units
    /// from the left edge and clipped to the card's rounded top-right corner.
    #[allow(clippy::too_many_arguments)]
    fn image(&mut self, img: &Pixmap, x: f32, y: f32, w: f32, h: f32, fade: f32, corner: f32) {
        let s = self.s;
        let Some(pm) = self.pm.as_mut() else { return };
        let (pw, ph) = (pm.width() as i32, pm.height() as i32);
        let (x0, y0) = ((x * s).floor() as i32, (y * s).floor() as i32);
        let (dw, dh) = ((w * s).round() as i32, (h * s).round() as i32);
        let src = img.pixels();
        let (sw, sh) = (img.width() as i32, img.height() as i32);
        // Crop the source to a centred square so any aspect fits the box.
        let side = sw.min(sh) as f32;
        let (ox, oy) = ((sw as f32 - side) / 2.0, (sh as f32 - side) / 2.0);
        let sample = |fx: f32, fy: f32| -> [f32; 4] {
            let fx = (fx - 0.5).clamp(0.0, sw as f32 - 1.0);
            let fy = (fy - 0.5).clamp(0.0, sh as f32 - 1.0);
            let (ix, iy) = (fx as i32, fy as i32);
            let (tx, ty) = (fx - ix as f32, fy - iy as f32);
            let at = |xx: i32, yy: i32| {
                let p = src[(yy.min(sh - 1) * sw + xx.min(sw - 1)) as usize];
                [p.red() as f32, p.green() as f32, p.blue() as f32, p.alpha() as f32]
            };
            let (a, b, c, d) = (at(ix, iy), at(ix + 1, iy), at(ix, iy + 1), at(ix + 1, iy + 1));
            let mut out = [0.0; 4];
            for k in 0..4 {
                out[k] = (a[k] * (1.0 - tx) + b[k] * tx) * (1.0 - ty) + (c[k] * (1.0 - tx) + d[k] * tx) * ty;
            }
            out
        };
        let card_right = WIDTH * s;
        let r = corner * s;
        let pixels = pm.pixels_mut();
        for dy in 0..dh {
            for dx in 0..dw {
                let (px, py) = (x0 + dx, y0 + dy);
                if px < 0 || py < 0 || px >= pw || py >= ph {
                    continue;
                }
                // Coverage: left fade × rounded corner.
                let mut k = ((dx as f32 + 0.5) / (fade * s)).min(1.0);
                k = k * k * (3.0 - 2.0 * k);
                let (cx, cy) = (card_right - r, r);
                let (fx, fy) = (px as f32 + 0.5, py as f32 + 0.5);
                if fx > cx && fy < cy {
                    let dist = ((fx - cx).powi(2) + (fy - cy).powi(2)).sqrt();
                    k *= (r - dist + 0.5).clamp(0.0, 1.0);
                }
                if k <= 0.0 {
                    continue;
                }
                let c = sample(
                    ox + (dx as f32 + 0.5) / dw as f32 * side,
                    oy + (dy as f32 + 0.5) / dh as f32 * side,
                );
                let a = c[3] / 255.0 * k;
                let inv = 1.0 - a;
                let d = pixels[py as usize * pw as usize + px as usize];
                // Source is premultiplied: scale its colour by k as well.
                let ch = |sc: f32, dc: u8| (sc * k + dc as f32 * inv).round().clamp(0.0, 255.0) as u8;
                let oa = (a * 255.0 + d.alpha() as f32 * inv).round().clamp(0.0, 255.0) as u8;
                let (rr, gg, bb) = (ch(c[0], d.red()).min(oa), ch(c[1], d.green()).min(oa), ch(c[2], d.blue()).min(oa));
                if let Some(v) = PremultipliedColorU8::from_rgba(rr, gg, bb, oa) {
                    pixels[py as usize * pw as usize + px as usize] = v;
                }
            }
        }
    }

    /// Draws a spec icon tinted in its profession colour; falls back to a
    /// short text label when the icon isn't available.
    fn spec_icon(&mut self, specs: &Specs, spec: &str, x: f32, y: f32, size: f32) {
        let info = specs.get(spec);
        let prof = info.map(|i| i.profession.as_str()).unwrap_or(spec);
        let col = profession_color(prof);
        let Some(icon) = info.and_then(|i| i.icon.as_ref()) else {
            let abbr: String = spec.chars().take(3).collect();
            let w = self.measure(9.0, true, &abbr);
            self.text(x + (size - w) / 2.0, y + 2.0, 9.0, true, col, &abbr);
            return;
        };
        let s = self.s;
        let Some(pm) = self.pm.as_mut() else { return };
        let (pw, ph) = (pm.width() as i32, pm.height() as i32);
        let (dx0, dy0) = ((x * s).round() as i32, (y * s).round() as i32);
        let d = (size * s).round().max(1.0) as i32;
        let pixels = pm.pixels_mut();
        // Area-average downsampling of the mask into the d×d target.
        for ty in 0..d {
            for tx in 0..d {
                let (px, py) = (dx0 + tx, dy0 + ty);
                if px < 0 || py < 0 || px >= pw || py >= ph {
                    continue;
                }
                let sx0 = tx as u32 * icon.w / d as u32;
                let sx1 = (((tx + 1) as u32 * icon.w).div_ceil(d as u32)).max(sx0 + 1).min(icon.w);
                let sy0 = ty as u32 * icon.h / d as u32;
                let sy1 = (((ty + 1) as u32 * icon.h).div_ceil(d as u32)).max(sy0 + 1).min(icon.h);
                let (mut sum, mut n) = (0u32, 0u32);
                for sy in sy0..sy1 {
                    for sx in sx0..sx1 {
                        sum += icon.mask[(sy * icon.w + sx) as usize] as u32;
                        n += 1;
                    }
                }
                let cov = sum as f32 / (n.max(1) as f32 * 255.0);
                let i = (py * pw + px) as usize;
                pixels[i] = blend(pixels[i], col, cov);
            }
        }
    }

    fn text_right(&mut self, right: f32, y: f32, size: f32, bold: bool, c: Rgba, text: &str) {
        let w = self.measure(size, bold, text);
        self.text(right - w, y, size, bold, c, text);
    }

    /// Like `text`, but truncates with an ellipsis to fit `max_w`.
    #[allow(clippy::too_many_arguments)]
    fn text_fit(&mut self, x: f32, y: f32, size: f32, bold: bool, c: Rgba, text: &str, max_w: f32) {
        if self.measure(size, bold, text) <= max_w {
            self.text(x, y, size, bold, c, text);
            return;
        }
        let mut chars: Vec<char> = text.chars().collect();
        while !chars.is_empty() {
            chars.pop();
            let candidate: String = chars.iter().collect::<String>().trim_end().to_string() + "…";
            if self.measure(size, bold, &candidate) <= max_w {
                self.text(x, y, size, bold, c, &candidate);
                return;
            }
        }
    }

    /// Word-wrapped paragraph; returns the height used.
    fn wrapped(&mut self, x: f32, y: f32, size: f32, c: Rgba, text: &str, max_w: f32) -> f32 {
        let line_h = size * 1.5;
        let mut lines: Vec<String> = Vec::new();
        let mut cur = String::new();
        for word in text.split(' ') {
            let candidate = if cur.is_empty() { word.to_string() } else { format!("{cur} {word}") };
            if self.measure(size, false, &candidate) > max_w && !cur.is_empty() {
                lines.push(std::mem::take(&mut cur));
                cur = word.to_string();
            } else {
                cur = candidate;
            }
        }
        if !cur.is_empty() {
            lines.push(cur);
        }
        for (i, l) in lines.iter().enumerate() {
            self.text(x, y + i as f32 * line_h, size, false, c, l);
        }
        lines.len() as f32 * line_h
    }
}

/// Source-over blend of `c` at coverage `cov` onto a premultiplied pixel.
fn blend(dst: PremultipliedColorU8, c: Rgba, cov: f32) -> PremultipliedColorU8 {
    let a = (c[3] as f32 / 255.0) * cov.clamp(0.0, 1.0);
    let inv = 1.0 - a;
    let ch = |src: u8, d: u8| ((src as f32 * a) + d as f32 * inv).round().clamp(0.0, 255.0) as u8;
    let out_a = ((a * 255.0) + dst.alpha() as f32 * inv).round().clamp(0.0, 255.0) as u8;
    let r = ch(c[0], dst.red()).min(out_a);
    let g = ch(c[1], dst.green()).min(out_a);
    let b = ch(c[2], dst.blue()).min(out_a);
    PremultipliedColorU8::from_rgba(r, g, b, out_a).unwrap_or(dst)
}
