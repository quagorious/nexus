//! The squad view: everything the in-game squad panel shows for one try,
//! computed from the full analysis of that try and of today's other tries at
//! the same boss. Pure data (strings, numbers, colours); the addon draws it
//! with imgui.
//!
//! Every string here stays within Latin-1: the in-game font has nothing
//! beyond that.

use std::collections::HashMap;

use crate::model::{FullEncounter, Phase, Player};
use crate::render::{Rgba, Specs, profession_color};
use crate::text::{Lang, fmt_dec, fmt_duration, fmt_hp, fmt_int};

/// One of today's tries at this boss + mode.
pub struct TodayTry<'a> {
    pub id: u64,
    pub success: bool,
    pub boss_hp_left: f64,
    /// The full analysis, when the try's share token is known.
    pub full: Option<&'a FullEncounter>,
}

pub struct SquadInput<'a> {
    pub id: u64,
    pub full: &'a FullEncounter,
    /// Localized boss name (from the module API).
    pub boss: &'a str,
    pub mode: &'a str,
    /// Your account.
    pub account: &'a str,
    /// Today's tries at this boss + mode, oldest first, including this one.
    pub today: &'a [TodayTry<'a>],
    /// Local start date and time, e.g. "15.11.2026 · 20:56" (empty if unknown).
    pub stamp: &'a str,
    /// Mechanic name → localized text, where known.
    pub texts: &'a HashMap<String, String>,
    pub specs: &'a Specs,
    pub lang: Lang,
}

#[derive(Clone, Debug, Default)]
pub struct SquadView {
    pub id: u64,
    pub boss_id: Option<u64>,
    pub labels: Labels,
    pub success: bool,
    /// "KILL" / "WIPE"
    pub result: String,
    /// "6:35" / "0,40 %"
    pub result_detail: String,
    /// "Erntetempel · CM · Try 5 heute · 6:32"
    pub subtitle: String,
    pub you_commander: bool,
    /// Header figures on the right: (value, label).
    pub stats: Vec<(String, String)>,
    pub tiles: Vec<Tile>,
    pub duration_ms: u64,
    pub phases: Vec<PhaseSpan>,
    pub markers: Vec<Marker>,
    pub today_phases: PhaseTable,
    /// Sorted by group, then DPS.
    pub players: Vec<SquadRow>,
    pub groups: Vec<GroupBoons>,
    /// "3 Downs · 0 Tode"
    pub downs_summary: String,
    pub downs: Vec<DownRow>,
    /// Empty until the second try of the day.
    pub recurring: Vec<Recurring>,
    /// How many of today's tries `recurring` compares.
    pub recurring_tries: usize,
    pub mechanics: Vec<MechanicCount>,
    pub chips: Vec<Chip>,
}

#[derive(Clone, Debug, Default)]
pub struct Tile {
    pub label: String,
    pub value: String,
    pub sub: String,
    /// Green for good news, red for bad, none for neutral.
    pub tone: Option<bool>,
}

#[derive(Clone, Debug, Default)]
pub struct PhaseSpan {
    pub name: String,
    pub start: u64,
    pub end: u64,
    /// The try was lost in this phase.
    pub lost: bool,
}

#[derive(Clone, Debug, Default)]
pub struct Marker {
    pub ms: u64,
    pub death: bool,
    pub who: String,
}

#[derive(Clone, Debug, Default)]
pub struct PhaseTable {
    pub columns: Vec<String>,
    pub rows: Vec<PhaseRow>,
}

#[derive(Clone, Debug, Default)]
pub struct PhaseRow {
    pub label: String,
    pub current: bool,
    pub success: bool,
    /// `None`: phase not reached (or no analysis for this try).
    pub cells: Vec<Option<PhaseCell>>,
    /// Total duration, or "-" without analysis.
    pub end: String,
}

#[derive(Clone, Debug, Default)]
pub struct PhaseCell {
    pub text: String,
    /// Fastest completed time in this column today.
    pub fastest: bool,
    /// The try was lost in this phase.
    pub lost: bool,
}

#[derive(Clone, Debug, Default)]
pub struct SquadRow {
    pub group: u32,
    pub spec: String,
    pub color: Rgba,
    pub name: String,
    pub account: String,
    pub role: String,
    pub dps: String,
    /// DPS relative to the best player (0–1).
    pub dps_frac: f32,
    pub downs: u32,
    pub deaths: u32,
    pub fails: u32,
    /// Percent over active time.
    pub quickness: Option<f64>,
    pub alacrity: Option<f64>,
    pub you: bool,
    pub commander: bool,
}

#[derive(Clone, Debug, Default)]
pub struct GroupBoons {
    pub group: u32,
    pub quickness: Option<f64>,
    pub alacrity: Option<f64>,
}

#[derive(Clone, Debug, Default)]
pub struct DownRow {
    pub name: String,
    pub spec: String,
    pub color: Rgba,
    /// "2 Downs, 1 Tod"
    pub counts: String,
    /// "3:47, 4:12 (Tod)"
    pub times: String,
    pub death: bool,
}

#[derive(Clone, Debug, Default)]
pub struct Recurring {
    pub text: String,
    /// Hits per try, oldest first, e.g. "3 · 1 · 2".
    pub per_try: String,
}

#[derive(Clone, Debug, Default)]
pub struct MechanicCount {
    pub text: String,
    pub count: u32,
}

#[derive(Clone, Debug, Default)]
pub struct Chip {
    pub id: u64,
    pub label: String,
    pub success: bool,
    pub current: bool,
}

/// Fixed texts of the panel, in the card language.
#[derive(Clone, Debug, Default)]
pub struct Labels {
    pub commander: &'static str,
    pub fight: &'static str,
    pub downs_and_deaths: &'static str,
    pub down: &'static str,
    pub death: &'static str,
    pub phases_today: &'static str,
    pub phases_hint: &'static str,
    pub lost_phase: &'static str,
    pub end: &'static str,
    pub squad: &'static str,
    pub by_group: &'static str,
    pub col_group: &'static str,
    pub col_player: &'static str,
    pub col_role: &'static str,
    pub col_dps: &'static str,
    pub col_downs: &'static str,
    pub col_deaths: &'static str,
    pub col_fails: &'static str,
    pub col_quick: &'static str,
    pub col_alac: &'static str,
    pub boons_by_group: &'static str,
    pub active_time: &'static str,
    pub quickness: &'static str,
    pub alacrity: &'static str,
    pub downs_list: &'static str,
    pub no_downs: &'static str,
    pub recurring: &'static str,
    pub per_try: &'static str,
    pub from_second_try: &'static str,
    pub nothing_recurring: &'static str,
    pub mechanics: &'static str,
    pub this_try: &'static str,
    pub no_mechanics: &'static str,
    pub today: &'static str,
    pub open_browser: &'static str,
}

impl Labels {
    pub fn new(lang: Lang) -> Self {
        let de = lang == Lang::De;
        let t = |d: &'static str, e: &'static str| if de { d } else { e };
        Self {
            commander: t("Commander", "Commander"),
            fight: t("Kampfverlauf", "Fight"),
            downs_and_deaths: t("Downs und Tode", "Downs and deaths"),
            down: "Down",
            death: t("Tod", "Death"),
            phases_today: t("Phasen heute", "Phases today"),
            phases_hint: t(
                "Dauer je Try, schnellste grün",
                "Duration per try, fastest in green",
            ),
            lost_phase: t(
                "* Phase, in der der Try verloren ging",
                "* Phase in which the try was lost",
            ),
            end: t("Ende", "End"),
            squad: "Squad",
            by_group: t("nach Gruppe", "by group"),
            col_group: t("GR", "GR"),
            col_player: t("Spieler", "Player"),
            col_role: t("Rolle", "Role"),
            col_dps: "DPS",
            col_downs: "Downs",
            col_deaths: t("Tode", "Deaths"),
            col_fails: t("Fehler", "Fails"),
            col_quick: "Quick",
            col_alac: "Alac",
            boons_by_group: t("Boons je Gruppe", "Boons by group"),
            active_time: t("Schnitt der aktiven Zeit", "Average over active time"),
            quickness: t("Schnelligkeit", "Quickness"),
            alacrity: t("Alacrität", "Alacrity"),
            downs_list: t("Wer wann down war", "Who went down when"),
            no_downs: t("keine Downs, keine Tode", "no downs, no deaths"),
            recurring: t("Kommt jeden Try wieder", "Back every try"),
            per_try: t("Treffer je Try", "Hits per try"),
            from_second_try: t("Ab dem zweiten Try heute", "From the second try today"),
            nothing_recurring: t(
                "nichts kam in jedem Try vor",
                "nothing happened in every try",
            ),
            mechanics: t("Mechaniken im Squad", "Mechanics in the squad"),
            this_try: t("Treffer", "Hits"),
            no_mechanics: t("keine Mechanik-Fehler", "no mechanic fails"),
            today: t("Heute", "Today"),
            open_browser: t("Im Browser öffnen", "Open in browser"),
        }
    }
}

/// Phases that aren't nested in another one (e.g. "Void Time Caster" inside
/// "Purification 2" is dropped), in fight order.
pub fn top_level_phases(phases: &[Phase]) -> Vec<&Phase> {
    let mut out: Vec<&Phase> = Vec::new();
    for (i, p) in phases.iter().enumerate() {
        if p.end <= p.start {
            continue;
        }
        let nested = phases.iter().enumerate().any(|(j, o)| {
            j != i
                && o.start <= p.start
                && p.end <= o.end
                // Identical spans: keep the first one.
                && (o.start != p.start || o.end != p.end || j < i)
        });
        if !nested {
            out.push(p);
        }
    }
    out.sort_by_key(|p| p.start);
    out
}

/// Index (into `top`) of the phase the try was lost in; `None` on a kill.
fn lost_phase(full: &FullEncounter, top: &[&Phase]) -> Option<usize> {
    if full.encounter.success {
        return None;
    }
    let at = full
        .encounter
        .lost_at_ms
        .unwrap_or(full.encounter.duration_ms);
    top.iter().rposition(|p| p.start <= at)
}

/// Fight-time events "Downed"/"Dead" per account, in time order.
fn down_events(full: &FullEncounter) -> Vec<(u64, bool, &str)> {
    let mut ev: Vec<(u64, bool, &str)> = full
        .mechanics
        .iter()
        .filter(|m| m.kind == "ok" && (m.name == "Downed" || m.name == "Dead"))
        .flat_map(|m| {
            m.times
                .iter()
                .map(move |t| (*t, m.name == "Dead", m.account.as_str()))
        })
        .collect();
    // At equal times the down comes before the death.
    ev.sort_by_key(|e| (e.0, e.1));
    ev
}

/// Hits per fail mechanic in a try: name → (count, English description).
fn fail_counts(full: &FullEncounter) -> HashMap<&str, (u32, &str)> {
    let mut out: HashMap<&str, (u32, &str)> = HashMap::new();
    for m in full.mechanics.iter().filter(|m| m.kind == "fail") {
        let e = out
            .entry(m.name.as_str())
            .or_insert((0, m.description.as_str()));
        e.0 += m.count.max(m.fails).max(1);
    }
    out
}

fn mechanic_text(texts: &HashMap<String, String>, name: &str, description: &str) -> String {
    texts.get(name).cloned().unwrap_or_else(|| {
        if description.is_empty() {
            name.to_string()
        } else {
            description.to_string()
        }
    })
}

fn spec_color(specs: &Specs, spec: &str) -> Rgba {
    let profession = specs
        .get(spec)
        .map(|s| s.profession.as_str())
        .unwrap_or(spec);
    profession_color(profession)
}

fn uptime(p: &Player, boon: &str) -> Option<f64> {
    p.uptime_active.get(boon).copied().flatten()
}

fn avg(values: impl Iterator<Item = f64>) -> Option<f64> {
    let (sum, n) = values.fold((0.0, 0u32), |(s, n), v| (s + v, n + 1));
    (n > 0).then(|| sum / n as f64)
}

fn result_label(success: bool, boss_hp_left: f64, lang: Lang) -> String {
    if success {
        "Kill".into()
    } else {
        format!("{} %", fmt_hp(boss_hp_left, lang))
    }
}

pub fn build(inp: &SquadInput) -> SquadView {
    let lang = inp.lang;
    let de = lang == Lang::De;
    let full = inp.full;
    let head = &full.encounter;
    let labels = Labels::new(lang);

    let name_of = |account: &str| -> (String, String) {
        full.players
            .iter()
            .find(|p| p.account == account)
            .map(|p| (p.display_name().to_string(), p.profession.clone()))
            .unwrap_or_else(|| (account.to_string(), String::new()))
    };

    // Today's position of this try.
    let pos = inp.today.iter().position(|t| t.id == inp.id);
    let try_no = pos.map(|i| i + 1).unwrap_or(1);

    // ---- Header ------------------------------------------------------------
    let (result, result_detail) = if head.success {
        ("KILL".to_string(), fmt_duration(head.duration_ms))
    } else {
        (
            "WIPE".to_string(),
            format!("{} %", fmt_hp(head.boss_hp_left, lang)),
        )
    };
    let mut sub = vec![inp.boss.to_string()];
    if let Some(m) = crate::text::mode_label(inp.mode) {
        sub.push(m.to_string());
    }
    sub.push(if de {
        format!("Try {try_no} heute")
    } else {
        format!("try {try_no} today")
    });
    sub.push(fmt_duration(head.duration_ms));
    if !inp.stamp.is_empty() {
        sub.push(inp.stamp.to_string());
    }
    let subtitle = sub.join(" · ");

    let events = down_events(full);
    let (ev_downs, ev_deaths) =
        events.iter().fold(
            (0u32, 0u32),
            |(d, k), e| if e.1 { (d, k + 1) } else { (d + 1, k) },
        );
    let (all_downs, all_deaths) = if full.mechanics.is_empty() {
        (head.downs + head.lost_downs, head.deaths + head.lost_deaths)
    } else {
        (ev_downs, ev_deaths)
    };
    let squad_fails: u32 = full.players.iter().map(|p| p.fails).sum();
    let stats = vec![
        (fmt_int(head.squad_dps, lang), "Squad-DPS".to_string()),
        (
            format!("{all_downs} · {all_deaths}"),
            if de {
                "Downs · Tode".into()
            } else {
                "downs · deaths".into()
            },
        ),
        (
            squad_fails.to_string(),
            if de {
                "Fehler im Squad".into()
            } else {
                "squad fails".into()
            },
        ),
    ];

    // ---- Tiles -------------------------------------------------------------
    let earlier: Vec<&TodayTry> = pos
        .map(|i| inp.today[..i].iter().collect())
        .unwrap_or_default();
    let result_tile = if head.success {
        Tile {
            label: if de {
                "Ergebnis".into()
            } else {
                "Result".into()
            },
            value: match (de, try_no) {
                (true, 1) => "Kill nach 1 Try".into(),
                (true, n) => format!("Kill nach {n} Tries"),
                (false, 1) => "Kill after 1 try".into(),
                (false, n) => format!("Kill after {n} tries"),
            },
            sub: match (de, try_no) {
                (true, 1) => "erster Try heute".into(),
                (true, n) => format!("{n}. Try heute"),
                (false, 1) => "first try today".into(),
                (false, n) => format!("try {n} today"),
            },
            tone: Some(true),
        }
    } else {
        let best = earlier
            .iter()
            .filter(|t| !t.success)
            .map(|t| t.boss_hp_left)
            .fold(f64::INFINITY, f64::min);
        let sub = if earlier.is_empty() {
            if de {
                "erster Try heute".into()
            } else {
                "first try today".into()
            }
        } else if head.boss_hp_left <= best {
            if de {
                "bester Wipe heute".into()
            } else {
                "best wipe today".into()
            }
        } else if de {
            format!("bisher bestes heute {} %", fmt_hp(best, lang))
        } else {
            format!("best today so far {} %", fmt_hp(best, lang))
        };
        Tile {
            label: if de {
                "Ergebnis".into()
            } else {
                "Result".into()
            },
            value: if de {
                format!("Wipe bei {} %", fmt_hp(head.boss_hp_left, lang))
            } else {
                format!("Wipe at {} %", fmt_hp(head.boss_hp_left, lang))
            },
            sub,
            tone: Some(false),
        }
    };

    let first_death = full
        .players
        .iter()
        .filter_map(|p| p.first_death_ms.map(|ms| (ms, p)))
        .filter(|(ms, _)| head.lost_at_ms.is_none_or(|l| *ms <= l))
        .min_by_key(|(ms, _)| *ms);
    let deaths_tile = Tile {
        label: if de {
            "Tode im Kampf".into()
        } else {
            "Deaths in the fight".into()
        },
        value: match (head.deaths, de) {
            (0, true) => "keine".into(),
            (0, false) => "none".into(),
            (1, true) => "1 Tod".into(),
            (1, false) => "1 death".into(),
            (n, true) => format!("{n} Tode"),
            (n, false) => format!("{n} deaths"),
        },
        sub: match first_death {
            None if head.lost_deaths > 0 => {
                if de {
                    format!("{} erst nach dem Verlust", head.lost_deaths)
                } else {
                    format!("{} only after the loss", head.lost_deaths)
                }
            }
            None => {
                if de {
                    "alle bis zum Ende stehend".into()
                } else {
                    "everyone standing to the end".into()
                }
            }
            Some((ms, p)) => {
                if de {
                    format!("zuerst {} bei {}", p.display_name(), fmt_duration(ms))
                } else {
                    format!("first {} at {}", p.display_name(), fmt_duration(ms))
                }
            }
        },
        tone: Some(head.deaths == 0),
    };

    let best_dps = earlier
        .iter()
        .filter_map(|t| t.full.map(|f| f.encounter.squad_dps))
        .fold(None, |m: Option<f64>, v| Some(m.map_or(v, |m| m.max(v))));
    let dps_tile = Tile {
        label: "Squad-DPS".into(),
        value: fmt_int(head.squad_dps, lang),
        sub: match best_dps {
            None => {
                if de {
                    "erster Try heute".into()
                } else {
                    "first try today".into()
                }
            }
            Some(b) if head.squad_dps >= b => {
                if de {
                    "bester Try heute".into()
                } else {
                    "best try today".into()
                }
            }
            Some(b) => {
                let pct = fmt_dec((b - head.squad_dps) / b * 100.0, 0, lang);
                if de {
                    format!("{pct} % unter dem besten heute")
                } else {
                    format!("{pct} % below today's best")
                }
            }
        },
        tone: None,
    };

    let mut groups_set: Vec<u32> = full.players.iter().map(|p| p.subgroup).collect();
    groups_set.sort_unstable();
    groups_set.dedup();
    let n_players = if head.squad_size > 0 {
        head.squad_size as usize
    } else {
        full.players.len()
    };
    let squad_tile = Tile {
        label: "Squad".into(),
        value: if de {
            format!("{n_players} Spieler")
        } else {
            format!("{n_players} players")
        },
        sub: match (groups_set.len(), de) {
            (1, true) => "1 Gruppe".into(),
            (1, false) => "1 group".into(),
            (n, true) => format!("{n} Gruppen"),
            (n, false) => format!("{n} groups"),
        },
        tone: None,
    };
    let tiles = vec![result_tile, deaths_tile, dps_tile, squad_tile];

    // ---- Timeline ----------------------------------------------------------
    let top = top_level_phases(&head.phases);
    let lost = lost_phase(full, &top);
    let phases = top
        .iter()
        .enumerate()
        .map(|(i, p)| PhaseSpan {
            name: p.name.clone(),
            start: p.start,
            end: p.end,
            lost: lost == Some(i),
        })
        .collect();
    let markers = events
        .iter()
        .map(|(ms, death, acc)| Marker {
            ms: *ms,
            death: *death,
            who: name_of(acc).0,
        })
        .collect();

    // ---- Phases today ------------------------------------------------------
    let today_phases = phase_table(inp);

    // ---- Squad table -------------------------------------------------------
    let max_dps = full
        .players
        .iter()
        .map(|p| p.target_dps)
        .fold(0.0, f64::max)
        .max(1.0);
    let mut sorted: Vec<&Player> = full.players.iter().collect();
    sorted.sort_by(|a, b| {
        a.subgroup
            .cmp(&b.subgroup)
            .then(b.target_dps.total_cmp(&a.target_dps))
    });
    let players = sorted
        .iter()
        .map(|p| SquadRow {
            group: p.subgroup,
            spec: p.profession.clone(),
            color: spec_color(inp.specs, &p.profession),
            name: p.display_name().to_string(),
            account: p.account.clone(),
            role: lang.role(&p.role),
            dps: fmt_int(p.target_dps, lang),
            dps_frac: (p.target_dps / max_dps).clamp(0.0, 1.0) as f32,
            downs: p.downs,
            deaths: p.deaths,
            fails: p.fails,
            quickness: uptime(p, "quickness"),
            alacrity: uptime(p, "alacrity"),
            you: p.account == inp.account,
            commander: p.commander,
        })
        .collect();
    let groups = groups_set
        .iter()
        .map(|g| {
            let members = || full.players.iter().filter(move |p| p.subgroup == *g);
            GroupBoons {
                group: *g,
                quickness: avg(members().filter_map(|p| uptime(p, "quickness"))),
                alacrity: avg(members().filter_map(|p| uptime(p, "alacrity"))),
            }
        })
        .collect();

    // ---- Downs list (instead of a map: the analysis has no positions) -----
    let mut order: Vec<&str> = Vec::new();
    for (_, _, acc) in &events {
        if !order.contains(acc) {
            order.push(acc);
        }
    }
    let downs = order
        .iter()
        .map(|acc| {
            let mine: Vec<&(u64, bool, &str)> = events.iter().filter(|e| e.2 == *acc).collect();
            let d = mine.iter().filter(|e| !e.1).count() as u32;
            let k = mine.iter().filter(|e| e.1).count() as u32;
            // A down that turns into a death right away is shown once, as the death.
            let shown: Vec<&&(u64, bool, &str)> = mine
                .iter()
                .enumerate()
                .filter(|(i, e)| e.1 || !mine.get(i + 1).is_some_and(|n| n.1 && n.0 <= e.0 + 3000))
                .map(|(_, e)| e)
                .collect();
            let mut times: Vec<String> = shown
                .iter()
                .take(5)
                .map(|(ms, death, _)| {
                    if *death {
                        format!("{} ({})", fmt_duration(*ms), labels.death)
                    } else {
                        fmt_duration(*ms)
                    }
                })
                .collect();
            if shown.len() > 5 {
                times.push("...".into());
            }
            let (name, spec) = name_of(acc);
            DownRow {
                color: spec_color(inp.specs, &spec),
                name,
                spec,
                counts: lang.downs_deaths(d, k),
                times: times.join(", "),
                death: k > 0,
            }
        })
        .collect();
    let downs_summary = if de {
        format!(
            "{ev_downs} {} · {ev_deaths} {}",
            if ev_downs == 1 { "Down" } else { "Downs" },
            if ev_deaths == 1 { "Tod" } else { "Tode" }
        )
    } else {
        format!(
            "{ev_downs} {} · {ev_deaths} {}",
            if ev_downs == 1 { "down" } else { "downs" },
            if ev_deaths == 1 { "death" } else { "deaths" }
        )
    };

    // ---- Recurring mistakes (every analysed try today up to this one) -----
    let upto: Vec<&FullEncounter> = match pos {
        Some(i) => inp.today[..=i].iter().filter_map(|t| t.full).collect(),
        None => vec![full],
    };
    let mut recurring = Vec::new();
    if upto.len() >= 2 {
        let counts: Vec<HashMap<&str, (u32, &str)>> = upto.iter().map(|f| fail_counts(f)).collect();
        let mut names: Vec<(&str, &str, u32)> = counts
            .last()
            .into_iter()
            .flat_map(|c| c.iter())
            .filter(|(name, _)| counts.iter().all(|c| c.contains_key(*name)))
            .map(|(name, (_, desc))| (*name, *desc, counts.iter().map(|c| c[*name].0).sum()))
            .collect();
        names.sort_by(|a, b| b.2.cmp(&a.2).then(a.0.cmp(b.0)));
        recurring = names
            .iter()
            .take(6)
            .map(|(name, desc, _)| Recurring {
                text: mechanic_text(inp.texts, name, desc),
                per_try: counts
                    .iter()
                    .map(|c| c[*name].0.to_string())
                    .collect::<Vec<_>>()
                    .join(" · "),
            })
            .collect();
    }

    // ---- Mechanics in this try ---------------------------------------------
    let mut mech: Vec<(&str, (u32, &str))> = fail_counts(full).into_iter().collect();
    mech.sort_by(|a, b| b.1.0.cmp(&a.1.0).then(a.0.cmp(b.0)));
    let mechanics = mech
        .iter()
        .take(10)
        .map(|(name, (count, desc))| MechanicCount {
            text: mechanic_text(inp.texts, name, desc),
            count: *count,
        })
        .collect();

    // ---- Today's chips -----------------------------------------------------
    let chips = inp
        .today
        .iter()
        .enumerate()
        .map(|(i, t)| Chip {
            id: t.id,
            label: format!(
                "{} · {}",
                i + 1,
                result_label(t.success, t.boss_hp_left, lang)
            ),
            success: t.success,
            current: t.id == inp.id,
        })
        .collect();

    SquadView {
        id: inp.id,
        boss_id: head.boss_id,
        labels,
        success: head.success,
        result,
        result_detail,
        subtitle,
        you_commander: full
            .players
            .iter()
            .any(|p| p.account == inp.account && p.commander),
        stats,
        tiles,
        duration_ms: head.duration_ms.max(1),
        phases,
        markers,
        today_phases,
        players,
        groups,
        downs_summary,
        downs,
        recurring,
        recurring_tries: upto.len(),
        mechanics,
        chips,
    }
}

/// Phase durations of today's tries: columns from the try that got furthest.
fn phase_table(inp: &SquadInput) -> PhaseTable {
    let lang = inp.lang;
    let analysed: Vec<(usize, &TodayTry, &FullEncounter)> = inp
        .today
        .iter()
        .enumerate()
        .filter_map(|(i, t)| t.full.map(|f| (i, t, f)))
        .collect();
    let columns: Vec<String> = analysed
        .iter()
        .map(|(_, _, f)| top_level_phases(&f.encounter.phases))
        .max_by_key(|p| p.len())
        .unwrap_or_default()
        .iter()
        .map(|p| p.name.clone())
        .collect();

    // (duration, lost) per row and column.
    let mut grid: Vec<Vec<Option<(u64, bool)>>> = Vec::new();
    for t in inp.today {
        let row = match t.full {
            None => vec![None; columns.len()],
            Some(f) => {
                let top = top_level_phases(&f.encounter.phases);
                let lost = lost_phase(f, &top);
                columns
                    .iter()
                    .map(|c| {
                        let i = top.iter().position(|p| &p.name == c)?;
                        Some((top[i].end - top[i].start, lost == Some(i)))
                    })
                    .collect()
            }
        };
        grid.push(row);
    }
    let fastest: Vec<Option<u64>> = (0..columns.len())
        .map(|c| {
            let done: Vec<u64> = grid
                .iter()
                .filter_map(|r| r[c])
                .filter(|(_, lost)| !lost)
                .map(|(d, _)| d)
                .collect();
            (done.len() >= 2).then(|| done.into_iter().min()).flatten()
        })
        .collect();

    let rows = inp
        .today
        .iter()
        .zip(&grid)
        .enumerate()
        .map(|(i, (t, cells))| PhaseRow {
            label: format!(
                "{} · {}",
                i + 1,
                result_label(t.success, t.boss_hp_left, lang)
            ),
            current: t.id == inp.id,
            success: t.success,
            cells: cells
                .iter()
                .enumerate()
                .map(|(c, cell)| {
                    cell.map(|(d, lost)| PhaseCell {
                        text: if lost {
                            format!("{}*", fmt_duration(d))
                        } else {
                            fmt_duration(d)
                        },
                        fastest: !lost && fastest[c] == Some(d),
                        lost,
                    })
                })
                .collect(),
            end: t
                .full
                .map(|f| fmt_duration(f.encounter.duration_ms))
                .unwrap_or_else(|| "-".into()),
        })
        .collect();
    PhaseTable { columns, rows }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn full(json: &str) -> FullEncounter {
        serde_json::from_str(json).unwrap()
    }

    const A: &str = r#"{
      "encounter": {"bossId": 1, "success": 0, "cm": 1, "bossHpLeft": 0.4, "squadDps": 150000,
        "squadSize": 3, "durationMs": 300000, "lostAtMs": 250000, "deaths": 1, "downs": 2,
        "lostDeaths": 2, "lostDowns": 0,
        "phases": [{"name": "P1", "start": 0, "end": 100000}, {"name": "P2", "start": 100000, "end": 300000},
                   {"name": "Sub", "start": 120000, "end": 140000}]},
      "players": [
        {"account": "a.1", "character": "Anna", "profession": "Firebrand", "role": "qHeal", "subgroup": 1,
         "targetDps": 2000, "fails": 3, "commander": 1, "uptimeActive": {"quickness": 99.0, "alacrity": 90.0}},
        {"account": "b.2", "character": "Ben", "profession": "Harbinger", "role": "DPS", "subgroup": 1,
         "targetDps": 30000, "fails": 1, "commander": 0, "firstDeathMs": 200000,
         "uptimeActive": {"quickness": 97.0, "alacrity": null}},
        {"account": "c.3", "character": "Cid", "profession": "Weaver", "role": "DPS", "subgroup": 2,
         "targetDps": 25000, "fails": 0, "commander": 0}
      ],
      "mechanics": [
        {"name": "Hit.H", "description": "Hit by Hit", "account": "b.2", "count": 2, "times": [1, 2], "fails": 2, "kind": "fail"},
        {"name": "Hit.H", "description": "Hit by Hit", "account": "a.1", "count": 1, "times": [3], "fails": 1, "kind": "fail"},
        {"name": "Downed", "description": "Downed", "account": "b.2", "count": 1, "times": [150000], "kind": "ok"},
        {"name": "Dead", "description": "Dead", "account": "b.2", "count": 1, "times": [200000], "kind": "ok"}
      ]
    }"#;

    #[test]
    fn builds_a_wipe() {
        let f = full(A);
        let today = [TodayTry {
            id: 7,
            success: false,
            boss_hp_left: 0.4,
            full: Some(&f),
        }];
        let texts = HashMap::from([("Hit.H".to_string(), "Von Hit getroffen".to_string())]);
        let v = build(&SquadInput {
            id: 7,
            full: &f,
            boss: "Boss",
            mode: "cm",
            account: "a.1",
            today: &today,
            stamp: "",
            texts: &texts,
            specs: &Specs::new(),
            lang: Lang::De,
        });
        assert_eq!(v.result, "WIPE");
        assert_eq!(v.subtitle, "Boss · CM · Try 1 heute · 5:00");
        assert!(v.you_commander);
        assert_eq!(v.stats[1].0, "1 · 1");
        assert_eq!(v.stats[2].0, "4");
        // Nested "Sub" is dropped; lost in P2.
        assert_eq!(v.phases.len(), 2);
        assert!(v.phases[1].lost && !v.phases[0].lost);
        // Group 1 by DPS, then group 2.
        let names: Vec<&str> = v.players.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["Ben", "Anna", "Cid"]);
        assert_eq!(v.groups[0].quickness, Some(98.0));
        assert_eq!(v.groups[0].alacrity, Some(90.0));
        assert_eq!(v.mechanics[0].text, "Von Hit getroffen");
        assert_eq!(v.mechanics[0].count, 3);
        assert_eq!(v.downs.len(), 1);
        assert_eq!(v.downs[0].times, "2:30, 3:20 (Tod)");
        assert_eq!(v.tiles[1].sub, "zuerst Ben bei 3:20");
        assert!(v.recurring.is_empty());
        assert_eq!(
            v.today_phases.rows[0].cells[1].as_ref().unwrap().text,
            "3:20*"
        );
    }

    #[test]
    fn recurring_and_fastest_across_tries() {
        let a = full(A);
        let mut b = full(A);
        b.encounter.success = true;
        b.encounter.phases[0].end = 90_000;
        b.encounter.phases[1].start = 90_000;
        let today = [
            TodayTry {
                id: 7,
                success: false,
                boss_hp_left: 0.4,
                full: Some(&a),
            },
            TodayTry {
                id: 8,
                success: true,
                boss_hp_left: 0.0,
                full: Some(&b),
            },
        ];
        let v = build(&SquadInput {
            id: 8,
            full: &b,
            boss: "Boss",
            mode: "cm",
            account: "a.1",
            today: &today,
            stamp: "",
            texts: &HashMap::new(),
            specs: &Specs::new(),
            lang: Lang::En,
        });
        assert_eq!(v.tiles[0].value, "Kill after 2 tries");
        assert_eq!(v.recurring.len(), 1);
        assert_eq!(v.recurring[0].text, "Hit by Hit");
        assert_eq!(v.recurring[0].per_try, "3 · 3");
        let p1: Vec<bool> = v
            .today_phases
            .rows
            .iter()
            .map(|r| r.cells[0].as_ref().unwrap().fastest)
            .collect();
        assert_eq!(p1, [false, true]);
        assert_eq!(v.chips[1].label, "2 · Kill");
    }
}
