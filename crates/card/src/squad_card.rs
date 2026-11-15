//! The "Squad" card: the squad view (see [`crate::squad`]) as a wide PNG,
//! so it can be copied like the other cards. Same palette and primitives as
//! `render.rs`, laid out in two columns.

use crate::render::{
    BG, CardInput, DIM, GREEN, MUTED, ORANGE, PANEL, PHASE_A, PHASE_B, PURPLE, Painter, RED, Rgba, TEXT, TRACK,
    WHITE_SOFT, PAD,
};
use crate::squad::SquadView;
use crate::text::fmt_duration;

/// Card width in units (the other cards are 420).
pub const WIDTH: f32 = 900.0;
const INNER: f32 = WIDTH - 2.0 * PAD;
const GAP: f32 = 24.0;
/// Left column.
const LW: f32 = (INNER - GAP) * 0.62;
/// Right column.
const RX: f32 = PAD + LW + GAP;
const RW: f32 = INNER - LW - GAP;
const BLUE: Rgba = [96, 165, 250, 255];
const ROW: f32 = 17.0;

fn alpha(c: Rgba, a: u8) -> Rgba {
    [c[0], c[1], c[2], a]
}

/// Green from 95 %, orange from 85 %, red below.
fn uptime_color(v: f64) -> Rgba {
    if v >= 95.0 {
        GREEN
    } else if v >= 85.0 {
        ORANGE
    } else {
        RED
    }
}

fn pct(v: f64) -> String {
    format!("{v:.1} %")
}

/// Section caption in a column.
fn cap(p: &mut Painter, x: f32, w: f32, y: f32, left: &str, right: &str, right_col: Rgba) {
    let rw = if right.is_empty() { 0.0 } else { p.measure(10.5, true, right) };
    p.text_fit(x, y, 10.5, true, MUTED, &left.to_uppercase(), w - rw - 12.0);
    if !right.is_empty() {
        p.text_right(x + w, y, 10.5, true, right_col, right);
    }
}

/// Lays out (and, with a pixmap, draws) the squad card; returns its height.
pub(crate) fn layout(p: &mut Painter, inp: &CardInput, v: &SquadView, total_h: f32) -> f32 {
    if total_h > 0.0 {
        p.rrect(0.0, 0.0, WIDTH, total_h, 14.0, BG);
    }
    let mut y = header(p, inp, v);
    y = tiles(p, v, y);

    let mut ly = timeline(p, v, y);
    ly = phases_today(p, v, ly);
    ly = squad_table(p, inp, v, ly);
    ly = group_boons(p, v, ly);

    let mut ry = downs(p, inp, v, y);
    ry = recurring(p, v, ry);
    ry = mechanics(p, v, ry);

    y = ly.max(ry);
    y = chips(p, v, y);
    footer(p, inp, y)
}

fn header(p: &mut Painter, inp: &CardInput, v: &SquadView) -> f32 {
    let hh = 74.0;
    let (c0, c1) = if v.success {
        ([112, 64, 214, 255], [138, 44, 112, 255])
    } else {
        ([190, 42, 48, 255], [118, 30, 62, 255])
    };
    p.gradient_rrect(0.0, 0.0, WIDTH, hh + 14.0, 14.0, c0, c1);
    p.rect(0.0, hh, WIDTH, 14.0, BG);

    let mut right = WIDTH - PAD;
    if let Some(img) = &inp.ctx.boss_image {
        p.image(&img.0, WIDTH - hh, 0.0, hh, hh, 34.0, 14.0);
        right = WIDTH - hh - 4.0;
    }
    // Figures, right to left.
    for (value, label) in v.stats.iter().rev() {
        let vw = p.measure(21.0, true, value);
        let lw = p.measure(11.0, false, label);
        let cw = vw.max(lw);
        let x = right - cw;
        p.text(x + (cw - vw) / 2.0, 14.0, 21.0, true, TEXT, value);
        p.text(x + (cw - lw) / 2.0, 46.0, 11.0, false, WHITE_SOFT, label);
        right = x - 28.0;
    }

    let x = PAD + p.text(PAD, 12.0, 27.0, true, TEXT, &v.result);
    let mut x2 = x + 8.0 + p.text(x + 8.0, 12.0, 27.0, false, TEXT, &v.result_detail);
    if v.you_commander {
        x2 += 10.0;
        let tw = p.measure(11.0, true, v.labels.commander);
        p.rrect(x2, 20.0, tw + 12.0, 20.0, 4.0, alpha([0, 0, 0, 255], 70));
        p.text(x2 + 6.0, 22.0, 11.0, true, BLUE, v.labels.commander);
    }
    p.text_fit(PAD, 49.0, 13.0, false, WHITE_SOFT, &v.subtitle, right - PAD - 12.0);
    hh + 14.0
}

fn tiles(p: &mut Painter, v: &SquadView, y: f32) -> f32 {
    let n = v.tiles.len().max(1) as f32;
    let gap = 10.0;
    let tw = (INNER - gap * (n - 1.0)) / n;
    let h = 58.0;
    for (i, t) in v.tiles.iter().enumerate() {
        let x = PAD + i as f32 * (tw + gap);
        let border = match t.tone {
            Some(true) => alpha(GREEN, 120),
            Some(false) => alpha(RED, 120),
            None => TRACK,
        };
        p.rrect(x, y, tw, h, 7.0, border);
        p.rrect(x + 1.0, y + 1.0, tw - 2.0, h - 2.0, 6.0, PANEL);
        p.text_fit(x + 10.0, y + 7.0, 10.0, true, MUTED, &t.label.to_uppercase(), tw - 20.0);
        p.text_fit(x + 10.0, y + 20.0, 15.0, true, TEXT, &t.value, tw - 20.0);
        p.text_fit(x + 10.0, y + 39.0, 11.0, false, MUTED, &t.sub, tw - 20.0);
    }
    y + h + 16.0
}

fn timeline(p: &mut Painter, v: &SquadView, mut y: f32) -> f32 {
    let l = &v.labels;
    // Legend on the right: ■ Down ■ Tod
    let (dw, kw) = (p.measure(10.5, true, l.down), p.measure(10.5, true, l.death));
    let lx = PAD + LW - (dw + kw + 8.0 + 2.0 * 10.0 + 12.0);
    cap(p, PAD, lx - PAD, y, l.fight, "", MUTED);
    p.rect(lx, y + 4.0, 7.0, 7.0, TEXT);
    p.text(lx + 10.0, y, 10.5, true, MUTED, l.down);
    let kx = lx + 10.0 + dw + 12.0;
    p.rect(kx, y + 4.0, 7.0, 7.0, RED);
    p.text(kx + 10.0, y, 10.5, true, MUTED, l.death);
    y += 20.0;

    let total = v.duration_ms.max(1) as f32;
    let tx = |ms: u64| PAD + LW * (ms as f32 / total).clamp(0.0, 1.0);
    let top = y + 10.0;
    let bar_h = 22.0;
    p.rrect(PAD, top, LW, bar_h, 5.0, PANEL);
    for (i, ph) in v.phases.iter().enumerate() {
        let (x1, x2) = (tx(ph.start), tx(ph.end));
        let w = (x2 - x1 - 2.0).max(1.0);
        p.rrect(x1, top, w, bar_h, 4.0, if i % 2 == 0 { PHASE_A } else { PHASE_B });
        if ph.lost {
            p.rrect(x1, top, w, bar_h, 4.0, alpha(RED, 90));
        }
        if w > 26.0 {
            p.text_fit(x1 + 5.0, top + 4.0, 10.5, false, TEXT, &ph.name, w - 8.0);
        }
    }
    for m in &v.markers {
        let x = tx(m.ms);
        p.rect(x - 3.0, y, 6.0, 6.0, if m.death { RED } else { TEXT });
    }
    let ty = top + bar_h + 4.0;
    let step = if v.duration_ms > 360_000 { 120_000 } else { 60_000 };
    let mut t = 0;
    while t <= v.duration_ms {
        let label = fmt_duration(t);
        let x = tx(t);
        if x + p.measure(10.0, false, &label) <= PAD + LW + 1.0 {
            p.text(x, ty, 10.0, false, DIM, &label);
        }
        t += step;
    }
    ty + 22.0
}

fn phases_today(p: &mut Painter, v: &SquadView, mut y: f32) -> f32 {
    let t = &v.today_phases;
    if t.columns.is_empty() {
        return y;
    }
    let l = &v.labels;
    cap(p, PAD, LW, y, l.phases_today, l.phases_hint, MUTED);
    y += 20.0;
    let label_w = 72.0;
    let end_w = 40.0;
    let cw = (LW - label_w - end_w) / t.columns.len() as f32;
    for (i, c) in t.columns.iter().enumerate() {
        p.text_fit(PAD + label_w + i as f32 * cw, y, 10.0, true, MUTED, c, cw - 6.0);
    }
    p.text(PAD + LW - end_w, y, 10.0, true, MUTED, l.end);
    y += 16.0;
    for row in &t.rows {
        if row.current {
            p.rrect(PAD - 4.0, y - 2.0, LW + 8.0, ROW, 3.0, PANEL);
        }
        let col = if row.current {
            PURPLE
        } else if row.success {
            GREEN
        } else {
            TEXT
        };
        p.text_fit(PAD, y, 11.0, row.current, col, &row.label, label_w - 6.0);
        for (i, cell) in row.cells.iter().enumerate() {
            let x = PAD + label_w + i as f32 * cw;
            match cell {
                Some(c) => {
                    let col = if c.fastest {
                        GREEN
                    } else if c.lost {
                        RED
                    } else {
                        TEXT
                    };
                    p.text(x, y, 11.0, c.fastest, col, &c.text);
                }
                None => {
                    p.text(x, y, 11.0, false, DIM, "-");
                }
            }
        }
        p.text(PAD + LW - end_w, y, 11.0, false, TEXT, &row.end);
        y += ROW;
    }
    p.text(PAD, y + 2.0, 10.0, false, DIM, l.lost_phase);
    y + 24.0
}

fn squad_table(p: &mut Painter, inp: &CardInput, v: &SquadView, mut y: f32) -> f32 {
    let l = &v.labels;
    cap(p, PAD, LW, y, l.squad, l.by_group, MUTED);
    y += 20.0;
    // Right edges of the right-aligned columns, from the right.
    let r_alac = PAD + LW;
    let r_quick = r_alac - 52.0;
    let r_fails = r_quick - 52.0;
    let r_deaths = r_fails - 40.0;
    let r_downs = r_deaths - 36.0;
    let r_dps = r_downs - 44.0;
    let bar_w = 44.0;
    let bar_x = r_dps - 50.0 - bar_w;
    let role_x = bar_x - 74.0;
    let name_x = PAD + 38.0;

    let head = |p: &mut Painter, r: f32, s: &str| p.text_right(r, y, 10.0, true, MUTED, s);
    p.text(PAD, y, 10.0, true, MUTED, l.col_group);
    p.text(name_x, y, 10.0, true, MUTED, l.col_player);
    p.text(role_x, y, 10.0, true, MUTED, l.col_role);
    head(p, r_dps, l.col_dps);
    head(p, r_downs, l.col_downs);
    head(p, r_deaths, l.col_deaths);
    head(p, r_fails, l.col_fails);
    head(p, r_quick, l.col_quick);
    head(p, r_alac, l.col_alac);
    y += 16.0;

    let mut last_group = None;
    for (i, r) in v.players.iter().enumerate() {
        if i % 2 == 1 {
            p.rrect(PAD - 4.0, y - 2.0, LW + 8.0, ROW, 3.0, alpha(PANEL, 170));
        }
        if last_group != Some(r.group) {
            p.text(PAD, y, 11.0, false, MUTED, &r.group.to_string());
            last_group = Some(r.group);
        }
        p.spec_icon(inp.specs, &r.spec, PAD + 18.0, y, 14.0);
        let cmd_w = if r.commander { p.measure(10.0, true, "(C)") + 6.0 } else { 0.0 };
        let name_col = if r.you { PURPLE } else { TEXT };
        let nw = role_x - name_x - 8.0 - cmd_w;
        p.text_fit(name_x, y, 11.0, r.you, name_col, &r.name, nw);
        if r.commander {
            let used = p.measure(11.0, r.you, &r.name).min(nw);
            p.text(name_x + used + 5.0, y + 1.0, 10.0, true, BLUE, "(C)");
        }
        p.text_fit(role_x, y, 11.0, false, MUTED, &r.role, bar_x - role_x - 6.0);
        p.rrect(bar_x, y + 5.0, bar_w, 6.0, 3.0, TRACK);
        p.rrect(bar_x, y + 5.0, (bar_w * r.dps_frac).max(2.0), 6.0, 3.0, alpha(r.color, 220));
        p.text_right(r_dps, y, 11.0, false, TEXT, &r.dps);
        p.text_right(r_downs, y, 11.0, false, if r.downs > 0 { ORANGE } else { DIM }, &r.downs.to_string());
        p.text_right(r_deaths, y, 11.0, false, if r.deaths > 0 { RED } else { DIM }, &r.deaths.to_string());
        p.text_right(r_fails, y, 11.0, false, TEXT, &r.fails.to_string());
        for (right, boon) in [(r_quick, r.quickness), (r_alac, r.alacrity)] {
            match boon {
                Some(b) => p.text_right(right, y, 11.0, false, uptime_color(b), &pct(b)),
                None => p.text_right(right, y, 11.0, false, DIM, "-"),
            }
        }
        y += ROW;
    }
    y + 12.0
}

fn group_boons(p: &mut Painter, v: &SquadView, mut y: f32) -> f32 {
    if v.groups.is_empty() {
        return y;
    }
    let l = &v.labels;
    cap(p, PAD, LW, y, l.boons_by_group, l.active_time, MUTED);
    y += 20.0;
    let label_w = 30.0;
    let pct_w = 52.0;
    let col_w = (LW - label_w) / 2.0;
    let bar_w = col_w - pct_w - 14.0;
    for (i, name) in [l.quickness, l.alacrity].iter().enumerate() {
        p.text(PAD + label_w + i as f32 * col_w, y, 10.0, true, MUTED, name);
    }
    y += 16.0;
    for g in &v.groups {
        p.text(PAD, y, 11.0, true, MUTED, &format!("G{}", g.group));
        for (i, val) in [g.quickness, g.alacrity].into_iter().enumerate() {
            let x = PAD + label_w + i as f32 * col_w;
            p.rrect(x, y + 5.0, bar_w, 6.0, 3.0, TRACK);
            if let Some(val) = val {
                p.rrect(x, y + 5.0, bar_w * (val / 100.0).clamp(0.0, 1.0) as f32, 6.0, 3.0, uptime_color(val));
                p.text_right(x + bar_w + pct_w, y, 11.0, false, uptime_color(val), &pct(val));
            } else {
                p.text_right(x + bar_w + pct_w, y, 11.0, false, DIM, "-");
            }
        }
        y += ROW + 2.0;
    }
    y + 10.0
}

fn downs(p: &mut Painter, inp: &CardInput, v: &SquadView, mut y: f32) -> f32 {
    cap(p, RX, RW, y, v.labels.downs_list, &v.downs_summary, MUTED);
    y += 20.0;
    if v.downs.is_empty() {
        p.text(RX, y, 11.5, false, GREEN, v.labels.no_downs);
        return y + 28.0;
    }
    for d in &v.downs {
        p.spec_icon(inp.specs, &d.spec, RX, y, 14.0);
        let nw = p.measure(11.5, false, &d.name).min(RW * 0.55);
        p.text_fit(RX + 20.0, y, 11.5, false, TEXT, &d.name, nw);
        p.text_fit(RX + 28.0 + nw, y, 11.5, false, if d.death { RED } else { ORANGE }, &d.counts, RW - nw - 30.0);
        p.text_fit(RX + 20.0, y + 16.0, 10.5, false, MUTED, &d.times, RW - 20.0);
        y += 34.0;
    }
    y + 8.0
}

fn recurring(p: &mut Painter, v: &SquadView, mut y: f32) -> f32 {
    let l = &v.labels;
    cap(p, RX, RW, y, l.recurring, l.per_try, MUTED);
    y += 20.0;
    if v.recurring_tries < 2 {
        p.text(RX, y, 11.5, false, DIM, l.from_second_try);
        return y + 28.0;
    }
    if v.recurring.is_empty() {
        p.text(RX, y, 11.5, false, GREEN, l.nothing_recurring);
        return y + 28.0;
    }
    for r in &v.recurring {
        let nw = p.measure(11.0, true, &r.per_try);
        p.text_fit(RX, y, 11.5, false, TEXT, &r.text, RW - nw - 12.0);
        p.text_right(RX + RW, y, 11.0, true, ORANGE, &r.per_try);
        y += ROW + 2.0;
    }
    y + 10.0
}

fn mechanics(p: &mut Painter, v: &SquadView, mut y: f32) -> f32 {
    let l = &v.labels;
    cap(p, RX, RW, y, l.mechanics, l.this_try, MUTED);
    y += 20.0;
    if v.mechanics.is_empty() {
        p.text(RX, y, 11.5, false, GREEN, l.no_mechanics);
        return y + 28.0;
    }
    for m in &v.mechanics {
        let n = m.count.to_string();
        let bw = p.measure(11.0, true, &n) + 12.0;
        p.text_fit(RX, y, 11.5, false, TEXT, &m.text, RW - bw - 10.0);
        let bx = RX + RW - bw;
        p.rrect(bx, y - 1.0, bw, 17.0, 4.0, alpha(RED, 150));
        p.rrect(bx + 1.0, y, bw - 2.0, 15.0, 3.0, [62, 36, 44, 255]);
        p.text(bx + 6.0, y, 11.0, true, RED, &n);
        y += ROW + 3.0;
    }
    y + 10.0
}

fn chips(p: &mut Painter, v: &SquadView, mut y: f32) -> f32 {
    if v.chips.len() < 2 {
        return y;
    }
    p.text(PAD, y + 4.0, 10.5, true, MUTED, &v.labels.today.to_uppercase());
    let mut x = PAD + p.measure(10.5, true, &v.labels.today.to_uppercase()) + 12.0;
    for c in &v.chips {
        let w = p.measure(11.0, c.current, &c.label) + 16.0;
        if x + w > PAD + INNER {
            x = PAD + 60.0;
            y += 28.0;
        }
        let accent = if c.success { GREEN } else { PURPLE };
        if c.current {
            p.rrect(x, y, w, 22.0, 5.0, accent);
            p.rrect(x + 1.5, y + 1.5, w - 3.0, 19.0, 4.0, PANEL);
        } else {
            p.rrect(x, y, w, 22.0, 5.0, PANEL);
        }
        let col = if c.current {
            TEXT
        } else if c.success {
            GREEN
        } else {
            MUTED
        };
        p.text(x + 8.0, y + 4.0, 11.0, c.current, col, &c.label);
        x += w + 6.0;
    }
    y + 36.0
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
