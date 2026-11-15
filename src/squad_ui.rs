//! The squad panel: a second window with the whole squad's view of the try
//! shown in the main window (result, timeline, phase times of the day, squad
//! table, boons per group, downs, recurring mistakes, mechanics).
//!
//! The numbers come ready-made from `quagorious_card::squad`; this file only
//! draws them.

use std::{cell::Cell, collections::HashMap, fs, sync::atomic::Ordering};

use nexus::{
    imgui::{
        ChildWindow, Condition, StyleColor, TableColumnFlags, TableColumnSetup, TableFlags, Ui,
        Window,
    },
    texture::get_texture_or_create_from_memory,
};
use quagorious_card::squad::{SquadView, Tile};

use crate::{Cmd, SETTINGS, SHARED, SQUAD_OPEN, data_path, lock, send, set_message, shell};

thread_local! {
    /// Text size factor for this frame (Nexus > Options > Quagorious).
    static SCALE: Cell<f32> = const { Cell::new(1.0) };
}

/// The text size factor; fixed sizes are multiplied by it too.
fn s() -> f32 {
    SCALE.get()
}

/// Sets the font scale of the current window (or child) to `k` × text size.
fn font(ui: &Ui, k: f32) {
    ui.set_window_font_scale(k * s());
}

/// The text size setting, clamped to what the slider allows.
pub fn text_scale() -> f32 {
    lock(&SETTINGS).text_scale.clamp(0.8, 2.0)
}

/// A squad view plus what the render thread needs to draw it.
pub struct SquadPanel {
    pub view: SquadView,
    pub url: Option<String>,
    /// Spec name → icon PNG (texture id is derived from the name).
    icons: HashMap<String, Vec<u8>>,
    /// Boss artwork PNG, cached by the card code under `bosses/`.
    boss: Option<Vec<u8>>,
}

impl SquadPanel {
    /// Reads the spec icons the card code cached under `icons/`.
    pub fn new(view: SquadView, url: Option<String>) -> Self {
        let mut icons = HashMap::new();
        for spec in view.players.iter().map(|p| &p.spec) {
            if !icons.contains_key(spec)
                && let Some(bytes) =
                    data_path(&format!("icons/{spec}.png")).and_then(|p| fs::read(p).ok())
            {
                icons.insert(spec.clone(), bytes);
            }
        }
        let boss = view.boss_id.and_then(|id| data_path(&format!("bosses/{id}.png"))).and_then(|p| fs::read(p).ok());
        Self { view, url, icons, boss }
    }
}

type Col = [f32; 4];

const TEXT: Col = [0.94, 0.93, 0.96, 1.0];
const MUTED: Col = [0.59, 0.57, 0.65, 1.0];
const DIM: Col = [0.43, 0.41, 0.49, 1.0];
const GREEN: Col = [0.29, 0.87, 0.5, 1.0];
const RED: Col = [0.97, 0.44, 0.44, 1.0];
const YELLOW: Col = [0.98, 0.75, 0.14, 1.0];
const PURPLE: Col = [0.65, 0.55, 0.98, 1.0];
const BLUE: Col = [0.38, 0.65, 0.98, 1.0];
const PANEL: Col = [0.14, 0.13, 0.17, 1.0];
const TRACK: Col = [0.2, 0.19, 0.24, 1.0];
const PHASE_A: Col = [0.25, 0.24, 0.31, 1.0];
const PHASE_B: Col = [0.36, 0.26, 0.63, 1.0];
/// Header gradient (left, right), as on the cards: purple for a kill, red for a wipe.
fn head_cols(success: bool) -> (Col, Col) {
    if success {
        ([0.44, 0.25, 0.84, 1.0], [0.54, 0.17, 0.44, 1.0])
    } else {
        ([0.75, 0.16, 0.19, 1.0], [0.46, 0.12, 0.24, 1.0])
    }
}

fn rgba(c: [u8; 4]) -> Col {
    [
        c[0] as f32 / 255.0,
        c[1] as f32 / 255.0,
        c[2] as f32 / 255.0,
        c[3] as f32 / 255.0,
    ]
}

/// Uptime colour: green from 95 %, yellow from 85 %, red below.
fn uptime_col(v: f64) -> Col {
    if v >= 95.0 {
        GREEN
    } else if v >= 85.0 {
        YELLOW
    } else {
        RED
    }
}

fn pct(v: Option<f64>) -> String {
    v.map(|v| format!("{v:.1} %")).unwrap_or_else(|| "-".into())
}

/// `text`, cut to `width` pixels with "...".
fn fit(ui: &Ui, text: &str, width: f32) -> String {
    if ui.calc_text_size(text)[0] <= width {
        return text.to_string();
    }
    let mut s: String = text.to_string();
    while !s.is_empty() && ui.calc_text_size(format!("{s}..."))[0] > width {
        s.pop();
    }
    format!("{}...", s.trim_end())
}

/// Continues the current line at screen x `x`. (`same_line_with_pos`
/// counts from the table column's start inside a table, so screen
/// coordinates are used throughout.)
fn goto_x(ui: &Ui, x: f32) {
    ui.same_line();
    let y = ui.cursor_screen_pos()[1];
    ui.set_cursor_screen_pos([x, y]);
}

/// Text at the right edge of the current line/cell; `x0` is the screen x
/// where the line started, `avail` the width available from there.
fn right_text(ui: &Ui, x0: f32, avail: f32, text: &str, col: Col) {
    let w = ui.calc_text_size(text)[0];
    goto_x(ui, (x0 + avail - w).max(x0));
    ui.text_colored(col, text);
}

/// A muted section title with optional right-aligned note.
fn section(ui: &Ui, title: &str, note: &str) {
    ui.spacing();
    let x0 = ui.cursor_screen_pos()[0];
    let avail = ui.content_region_avail()[0];
    ui.text_colored(MUTED, title.to_uppercase());
    if !note.is_empty() {
        right_text(ui, x0, avail, note, MUTED);
    }
}

/// The spec icon, tinted with the profession colour, as a `size` square.
fn spec_icon(ui: &Ui, panel: &SquadPanel, spec: &str, color: Col, size: f32) {
    let p = ui.cursor_screen_pos();
    let tex = panel
        .icons
        .get(spec)
        .and_then(|b| get_texture_or_create_from_memory(format!("QUAGORIOUS_SPEC_{spec}"), b));
    if let Some(tex) = tex {
        ui.get_window_draw_list()
            .add_image(tex.id(), p, [p[0] + size, p[1] + size])
            .col(color)
            .build();
    } else {
        let c = [p[0] + size / 2.0, p[1] + size / 2.0];
        ui.get_window_draw_list()
            .add_circle(c, size / 3.0, color)
            .filled(true)
            .build();
    }
    ui.dummy([size, size]);
    if ui.is_item_hovered() {
        ui.tooltip_text(spec);
    }
}

pub fn draw(ui: &Ui) {
    if !SQUAD_OPEN.load(Ordering::Relaxed) {
        return;
    }
    let panel = lock(&SHARED).squad.clone();
    SCALE.set(text_scale());
    let mut open = true;
    Window::new("Quagorious Squad##QUAGORIOUS_SQUAD")
        .opened(&mut open)
        .size([920.0, 720.0], Condition::FirstUseEver)
        .build(ui, || {
            font(ui, 1.0);
            match &panel {
                Some(p) => draw_panel(ui, p),
                None => ui.text_colored(
                    MUTED,
                    crate::tr(
                        "Keine Squad-Daten für diesen Try (braucht den Share-Link: nur live gesehene Tries).",
                        "No squad data for this try (needs the share link: tries seen live).",
                    ),
                ),
            }
        });
    if !open {
        SQUAD_OPEN.store(false, Ordering::Relaxed);
    }
}

fn draw_panel(ui: &Ui, p: &SquadPanel) {
    let v = &p.view;
    header(ui, p);
    tiles(ui, &v.tiles);

    let flags = TableFlags::SIZING_STRETCH_PROP
        | TableFlags::BORDERS_INNER_V
        | TableFlags::NO_SAVED_SETTINGS;
    if let Some(_t) = ui.begin_table_with_flags("##squad_cols", 2, flags) {
        let mut left = TableColumnSetup::new("##left");
        left.flags = TableColumnFlags::WIDTH_STRETCH;
        left.init_width_or_weight = 0.62;
        ui.table_setup_column_with(left);
        let mut right = TableColumnSetup::new("##right");
        right.flags = TableColumnFlags::WIDTH_STRETCH;
        right.init_width_or_weight = 0.38;
        ui.table_setup_column_with(right);
        ui.table_next_row();

        ui.table_next_column();
        timeline(ui, v);
        phases_today(ui, v);
        squad_table(ui, p);
        group_boons(ui, v);

        ui.table_next_column();
        downs(ui, p);
        recurring(ui, v);
        mechanics(ui, v);
    }

    footer(ui, p);
}

fn header(ui: &Ui, p: &SquadPanel) {
    let v = &p.view;
    let h = 66.0 * s();
    let p0 = ui.cursor_screen_pos();
    let w = ui.content_region_avail()[0];
    let (cl, cr) = head_cols(v.success);
    let mut stats_right = w - 12.0 * s();
    {
        let dl = ui.get_window_draw_list();
        dl.add_rect_filled_multicolor(p0, [p0[0] + w, p0[1] + h], cl, cr, cr, cl);
        // Boss artwork at the right end, faded into the gradient.
        let tex =
            p.boss.as_ref().zip(v.boss_id).and_then(|(b, id)| get_texture_or_create_from_memory(format!("QUAGORIOUS_BOSS_{id}"), b));
        if let Some(tex) = tex {
            let x = p0[0] + w - h;
            dl.add_image(tex.id(), [x, p0[1]], [x + h, p0[1] + h]).build();
            let clear = [cr[0], cr[1], cr[2], 0.0];
            dl.add_rect_filled_multicolor([x, p0[1]], [x + h * 0.55, p0[1] + h], cr, clear, clear, cr);
            stats_right = w - h - 4.0 * s();
        }
    }
    let start = ui.cursor_pos();

    ui.set_cursor_pos([start[0] + 12.0 * s(), start[1] + 6.0 * s()]);
    font(ui, 1.8);
    ui.text_colored(TEXT, &v.result);
    ui.same_line();
    ui.text_colored([1.0, 1.0, 1.0, 0.85], &v.result_detail);
    font(ui, 1.0);
    if v.you_commander {
        ui.same_line();
        ui.text_colored(BLUE, format!("[{}]", v.labels.commander));
    }
    ui.set_cursor_pos([start[0] + 12.0 * s(), ui.cursor_pos()[1]]);

    // Figures on the right, right to left.
    let mut x = start[0] + stats_right;
    for (value, label) in v.stats.iter().rev() {
        font(ui, 1.4);
        let vw = ui.calc_text_size(value)[0];
        font(ui, 1.0);
        let lw = ui.calc_text_size(label)[0];
        let cw = vw.max(lw);
        x -= cw;
        ui.set_cursor_pos([x + (cw - vw) / 2.0, start[1] + 8.0 * s()]);
        font(ui, 1.4);
        ui.text_colored(TEXT, value);
        font(ui, 1.0);
        ui.set_cursor_pos([x + (cw - lw) / 2.0, start[1] + 40.0 * s()]);
        ui.text_colored([1.0, 1.0, 1.0, 0.75], label);
        x -= 28.0 * s();
    }
    // Subtitle (boss, mode, try, duration, date and start time), cut before the figures.
    ui.set_cursor_pos([start[0] + 12.0 * s(), start[1] + 40.0 * s()]);
    let room = x - start[0] - 12.0 * s();
    ui.text_colored([1.0, 1.0, 1.0, 0.8], fit(ui, &v.subtitle, room.max(40.0)));
    ui.set_cursor_pos([start[0], start[1] + h + 6.0 * s()]);
}

fn tiles(ui: &Ui, tiles: &[Tile]) {
    let n = tiles.len().max(1) as f32;
    let gap = 8.0 * s();
    let w = (ui.content_region_avail()[0] - gap * (n - 1.0)) / n;
    for (i, t) in tiles.iter().enumerate() {
        if i > 0 {
            ui.same_line_with_spacing(0.0, gap);
        }
        let border = match t.tone {
            Some(true) => [GREEN[0], GREEN[1], GREEN[2], 0.45],
            Some(false) => [RED[0], RED[1], RED[2], 0.45],
            None => [1.0, 1.0, 1.0, 0.08],
        };
        let _bg = ui.push_style_color(StyleColor::ChildBg, PANEL);
        let _bd = ui.push_style_color(StyleColor::Border, border);
        ChildWindow::new(i as i32 + 7100)
            .size([w, ui.text_line_height() * 3.0 + 12.0 * s() + ui.text_line_height() * 0.3])
            .border(true)
            .scrollable(false)
            .build(ui, || {
                font(ui, 1.0);
                ui.text_colored(MUTED, t.label.to_uppercase());
                font(ui, 1.25);
                ui.text_colored(TEXT, fit(ui, &t.value, w - 12.0));
                font(ui, 1.0);
                ui.text_colored(MUTED, fit(ui, &t.sub, w - 12.0));
            });
    }
}

fn timeline(ui: &Ui, v: &SquadView) {
    section(ui, v.labels.fight, v.labels.downs_and_deaths);
    let w = ui.content_region_avail()[0];
    let marker_h = 8.0 * s();
    let bar_h = 22.0 * s();
    let p0 = ui.cursor_screen_pos();
    let top = p0[1] + marker_h + 2.0;
    let x_at = |ms: u64| p0[0] + w * (ms as f32 / v.duration_ms as f32).clamp(0.0, 1.0);
    let mouse = ui.io().mouse_pos;
    let mut tip: Option<String> = None;
    {
        let dl = ui.get_window_draw_list();
        dl.add_rect([p0[0], top], [p0[0] + w, top + bar_h], TRACK)
            .filled(true)
            .rounding(3.0)
            .build();
        for (i, ph) in v.phases.iter().enumerate() {
            let (x1, x2) = (x_at(ph.start), x_at(ph.end));
            let col = if i % 2 == 0 { PHASE_A } else { PHASE_B };
            dl.add_rect([x1, top], [(x2 - 1.0).max(x1 + 1.0), top + bar_h], col)
                .filled(true)
                .rounding(2.0)
                .build();
            if ph.lost {
                dl.add_rect([x1, top], [(x2 - 1.0).max(x1 + 1.0), top + bar_h], RED)
                    .thickness(1.5)
                    .rounding(2.0)
                    .build();
            }
            let label = fit(ui, &ph.name, (x2 - x1 - 8.0).max(0.0));
            if label.len() > 3 {
                dl.with_clip_rect_intersect([x1, top], [x2, top + bar_h], || {
                    dl.add_text(
                        [x1 + 4.0, top + (bar_h - ui.text_line_height()) / 2.0],
                        TEXT,
                        &label,
                    );
                });
            }
            if mouse[0] >= x1 && mouse[0] < x2 && mouse[1] >= top && mouse[1] <= top + bar_h {
                tip = Some(format!(
                    "{}\n{} - {} ({})",
                    ph.name,
                    fmt(ph.start),
                    fmt(ph.end),
                    fmt(ph.end.saturating_sub(ph.start))
                ));
            }
        }
        for m in &v.markers {
            let x = x_at(m.ms);
            let col = if m.death { RED } else { TEXT };
            dl.add_rect([x - 3.0, p0[1]], [x + 3.0, p0[1] + marker_h - 2.0], col)
                .filled(true)
                .build();
            if (mouse[0] - x).abs() <= 4.0 && mouse[1] >= p0[1] && mouse[1] <= top {
                let what = if m.death {
                    v.labels.death
                } else {
                    v.labels.down
                };
                tip = Some(format!("{} {} {}", fmt(m.ms), what, m.who));
            }
        }
        // Axis ticks: every minute, or every two from 6 minutes on.
        let step = if v.duration_ms > 360_000 {
            120_000
        } else {
            60_000
        };
        let mut t = 0;
        while t <= v.duration_ms {
            let x = x_at(t);
            let label = fmt(t);
            let lw = ui.calc_text_size(&label)[0];
            if x + lw <= p0[0] + w + 1.0 {
                dl.add_text([x, top + bar_h + 3.0], DIM, &label);
            }
            t += step;
        }
    }
    ui.dummy([w, marker_h + 2.0 + bar_h + 3.0 + ui.text_line_height()]);
    if let Some(tip) = tip {
        ui.tooltip_text(tip);
    }
    ui.text_colored(TEXT, "#");
    ui.same_line();
    ui.text_colored(MUTED, v.labels.down);
    ui.same_line();
    ui.text_colored(RED, "#");
    ui.same_line();
    ui.text_colored(MUTED, v.labels.death);
}

fn fmt(ms: u64) -> String {
    quagorious_card::text::fmt_duration(ms)
}

fn phases_today(ui: &Ui, v: &SquadView) {
    let t = &v.today_phases;
    if t.columns.is_empty() {
        return;
    }
    section(ui, v.labels.phases_today, v.labels.phases_hint);
    let n = t.columns.len() + 2;
    let flags = TableFlags::SIZING_STRETCH_SAME
        | TableFlags::BORDERS_INNER_H
        | TableFlags::NO_SAVED_SETTINGS;
    let Some(_t) = ui.begin_table_with_flags("##phases_today", n, flags) else {
        return;
    };
    let mut first = TableColumnSetup::new("##try");
    first.flags = TableColumnFlags::WIDTH_FIXED;
    first.init_width_or_weight = ui.calc_text_size("00 · 00,00 %")[0];
    ui.table_setup_column_with(first);
    for (i, _) in t.columns.iter().enumerate() {
        ui.table_setup_column(format!("##ph{i}"));
    }
    ui.table_setup_column("##end");

    ui.table_next_row();
    ui.table_next_column();
    for c in &t.columns {
        ui.table_next_column();
        let w = ui.content_region_avail()[0];
        ui.text_colored(MUTED, fit(ui, c, w));
        if ui.is_item_hovered() {
            ui.tooltip_text(c);
        }
    }
    ui.table_next_column();
    ui.text_colored(MUTED, v.labels.end);

    for row in &t.rows {
        ui.table_next_row();
        ui.table_next_column();
        let col = if row.current {
            PURPLE
        } else if row.success {
            GREEN
        } else {
            TEXT
        };
        ui.text_colored(col, &row.label);
        for cell in &row.cells {
            ui.table_next_column();
            match cell {
                Some(c) => {
                    let col = if c.fastest {
                        GREEN
                    } else if c.lost {
                        RED
                    } else {
                        TEXT
                    };
                    ui.text_colored(col, &c.text);
                }
                None => ui.text_colored(DIM, "-"),
            }
        }
        ui.table_next_column();
        ui.text_colored(TEXT, &row.end);
    }
    drop(_t);
    ui.text_colored(DIM, v.labels.lost_phase);
}

fn squad_table(ui: &Ui, p: &SquadPanel) {
    let v = &p.view;
    let l = &v.labels;
    section(ui, l.squad, l.by_group);
    let flags = TableFlags::SIZING_FIXED_FIT
        | TableFlags::ROW_BG
        | TableFlags::NO_SAVED_SETTINGS
        | TableFlags::PAD_OUTER_X;
    let Some(_t) = ui.begin_table_with_flags("##squad", 10, flags) else {
        return;
    };
    let heads = [
        l.col_group,
        l.col_player,
        l.col_role,
        "",
        l.col_dps,
        l.col_downs,
        l.col_deaths,
        l.col_fails,
        l.col_quick,
        l.col_alac,
    ];
    for (i, h) in heads.iter().enumerate() {
        let mut c = TableColumnSetup::new(format!("{h}##sq{i}"));
        c.flags = if i == 1 {
            TableColumnFlags::WIDTH_STRETCH
        } else {
            TableColumnFlags::WIDTH_FIXED
        };
        if i == 3 {
            c.init_width_or_weight = 60.0 * s();
        }
        ui.table_setup_column_with(c);
    }
    ui.table_next_row();
    for h in heads {
        ui.table_next_column();
        ui.text_colored(MUTED, h);
    }

    let icon = ui.text_line_height();
    let mut last_group = None;
    for r in &v.players {
        ui.table_next_row();
        ui.table_next_column();
        if last_group != Some(r.group) {
            ui.text_colored(MUTED, r.group.to_string());
            last_group = Some(r.group);
        }
        ui.table_next_column();
        spec_icon(ui, p, &r.spec, rgba(r.color), icon);
        ui.same_line();
        let name_col = if r.you { PURPLE } else { TEXT };
        let w = ui.content_region_avail()[0] - if r.commander { 30.0 } else { 0.0 };
        ui.text_colored(name_col, fit(ui, &r.name, w));
        if ui.is_item_hovered() {
            ui.tooltip_text(&r.account);
        }
        if r.commander {
            ui.same_line();
            ui.text_colored(BLUE, "(C)");
        }
        ui.table_next_column();
        ui.text_colored(MUTED, &r.role);
        ui.table_next_column();
        {
            let p0 = ui.cursor_screen_pos();
            let bw = ui.content_region_avail()[0];
            let y = p0[1] + ui.text_line_height() / 2.0 - 3.0;
            let dl = ui.get_window_draw_list();
            dl.add_rect(
                [p0[0], y],
                [p0[0] + bw * r.dps_frac.max(0.02), y + 6.0],
                r_col(r.color),
            )
            .filled(true)
            .rounding(2.0)
            .build();
            ui.dummy([bw, ui.text_line_height()]);
        }
        ui.table_next_column();
        ui.text_colored(TEXT, &r.dps);
        ui.table_next_column();
        ui.text_colored(if r.downs > 0 { YELLOW } else { DIM }, r.downs.to_string());
        ui.table_next_column();
        ui.text_colored(if r.deaths > 0 { RED } else { DIM }, r.deaths.to_string());
        ui.table_next_column();
        ui.text_colored(TEXT, r.fails.to_string());
        for boon in [r.quickness, r.alacrity] {
            ui.table_next_column();
            match boon {
                Some(b) => ui.text_colored(uptime_col(b), pct(Some(b))),
                None => ui.text_colored(DIM, "-"),
            }
        }
    }
}

fn r_col(c: [u8; 4]) -> Col {
    let mut c = rgba(c);
    c[3] = 0.85;
    c
}

fn group_boons(ui: &Ui, v: &SquadView) {
    if v.groups.is_empty() {
        return;
    }
    section(ui, v.labels.boons_by_group, v.labels.active_time);
    let label_w = ui.calc_text_size("G00")[0] + 8.0;
    let pct_w = ui.calc_text_size("100.0 %")[0] + 8.0;
    let avail = ui.content_region_avail()[0];
    let bar_w = ((avail - label_w - 2.0 * pct_w - 16.0) / 2.0).max(40.0);
    let x0 = ui.cursor_screen_pos()[0];
    ui.text_colored(DIM, "");
    goto_x(ui, x0 + label_w);
    ui.text_colored(MUTED, v.labels.quickness);
    goto_x(ui, x0 + label_w + bar_w + pct_w + 16.0);
    ui.text_colored(MUTED, v.labels.alacrity);
    for g in &v.groups {
        ui.text_colored(MUTED, format!("G{}", g.group));
        for (i, val) in [g.quickness, g.alacrity].into_iter().enumerate() {
            let x = x0 + label_w + i as f32 * (bar_w + pct_w + 16.0);
            goto_x(ui, x);
            let p0 = ui.cursor_screen_pos();
            let y = p0[1] + ui.text_line_height() / 2.0 - 3.0;
            {
                let dl = ui.get_window_draw_list();
                dl.add_rect([p0[0], y], [p0[0] + bar_w, y + 6.0], TRACK)
                    .filled(true)
                    .rounding(2.0)
                    .build();
                if let Some(val) = val {
                    let f = (val / 100.0).clamp(0.0, 1.0) as f32;
                    dl.add_rect([p0[0], y], [p0[0] + bar_w * f, y + 6.0], uptime_col(val))
                        .filled(true)
                        .rounding(2.0)
                        .build();
                }
            }
            ui.dummy([bar_w, ui.text_line_height()]);
            goto_x(ui, x + bar_w + 6.0);
            ui.text_colored(val.map(uptime_col).unwrap_or(DIM), pct(val));
        }
    }
}

fn downs(ui: &Ui, p: &SquadPanel) {
    let v = &p.view;
    section(ui, v.labels.downs_list, &v.downs_summary);
    if v.downs.is_empty() {
        ui.text_colored(GREEN, v.labels.no_downs);
        return;
    }
    let icon = ui.text_line_height();
    for d in &v.downs {
        spec_icon(ui, p, &d.spec, rgba(d.color), icon);
        ui.same_line();
        ui.text_colored(TEXT, &d.name);
        ui.same_line();
        ui.text_colored(if d.death { RED } else { YELLOW }, &d.counts);
        let w = ui.content_region_avail()[0];
        ui.set_cursor_pos([ui.cursor_pos()[0] + icon + 8.0, ui.cursor_pos()[1]]);
        ui.text_colored(MUTED, fit(ui, &d.times, w - icon - 8.0));
    }
}

fn recurring(ui: &Ui, v: &SquadView) {
    section(ui, v.labels.recurring, v.labels.per_try);
    if v.recurring_tries < 2 {
        ui.text_colored(DIM, v.labels.from_second_try);
        return;
    }
    if v.recurring.is_empty() {
        ui.text_colored(GREEN, v.labels.nothing_recurring);
        return;
    }
    for r in &v.recurring {
        let x0 = ui.cursor_screen_pos()[0];
        let avail = ui.content_region_avail()[0];
        let nw = ui.calc_text_size(&r.per_try)[0];
        let text = fit(ui, &r.text, avail - nw - 12.0);
        ui.text_colored(TEXT, &text);
        if text != r.text && ui.is_item_hovered() {
            ui.tooltip_text(&r.text);
        }
        right_text(ui, x0, avail, &r.per_try, YELLOW);
    }
}

fn mechanics(ui: &Ui, v: &SquadView) {
    section(ui, v.labels.mechanics, v.labels.this_try);
    if v.mechanics.is_empty() {
        ui.text_colored(GREEN, v.labels.no_mechanics);
        return;
    }
    for m in &v.mechanics {
        let x0 = ui.cursor_screen_pos()[0];
        let avail = ui.content_region_avail()[0];
        let n = m.count.to_string();
        let nw = ui.calc_text_size(&n)[0] + 10.0;
        let text = fit(ui, &m.text, avail - nw - 12.0);
        ui.text_colored(TEXT, &text);
        if text != m.text && ui.is_item_hovered() {
            ui.tooltip_text(&m.text);
        }
        goto_x(ui, x0 + avail - nw);
        let p0 = ui.cursor_screen_pos();
        {
            let dl = ui.get_window_draw_list();
            let h = ui.text_line_height();
            dl.add_rect(
                [p0[0], p0[1]],
                [p0[0] + nw, p0[1] + h],
                [RED[0], RED[1], RED[2], 0.18],
            )
            .filled(true)
            .rounding(3.0)
            .build();
            dl.add_rect(
                [p0[0], p0[1]],
                [p0[0] + nw, p0[1] + h],
                [RED[0], RED[1], RED[2], 0.6],
            )
            .rounding(3.0)
            .build();
        }
        ui.set_cursor_screen_pos([p0[0] + 5.0, p0[1]]);
        ui.text_colored(RED, &n);
    }
}

fn footer(ui: &Ui, p: &SquadPanel) {
    let v = &p.view;
    ui.spacing();
    ui.separator();
    ui.text_colored(MUTED, v.labels.today.to_uppercase());
    for c in &v.chips {
        ui.same_line();
        if c.current {
            ui.text_colored(
                if c.success { GREEN } else { PURPLE },
                format!("[{}]", c.label),
            );
        } else {
            let _col = ui.push_style_color(StyleColor::Text, if c.success { GREEN } else { TEXT });
            if ui.small_button(format!("{}##chip{}", c.label, c.id)) {
                send(Cmd::Show(c.id));
            }
        }
    }
    if let Some(url) = &p.url {
        let label = v.labels.open_browser;
        let w = ui.calc_text_size(label)[0] + 16.0;
        let right = ui.window_pos()[0] + ui.content_region_max()[0];
        goto_x(ui, (right - w).max(ui.cursor_screen_pos()[0]));
        if ui.button(label)
            && let Err(e) = shell::open_url(url)
        {
            set_message(
                format!("{} {e}", crate::tr("Browser konnte nicht geöffnet werden:", "Could not open browser:")),
                true,
            );
        }
    }
}
