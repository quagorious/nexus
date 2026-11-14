//! Quagorious encounter model and share-card renderer.
//!
//! Kept free of any Windows/Nexus dependency so the card can be rendered and
//! eyeballed on the Linux host (`cargo run -p quagorious_card --example render`).

pub mod model;
pub mod render;
pub mod text;

pub use model::{Encounter, FullEncounter, TryList, TrySummary};
pub use render::{BossImage, CardContext, CardInput, IconMask, RenderedCard, SpecInfo, Specs, TodayChip, Variant, render};
pub use text::Lang;

/// The cards available for a try, in carousel order. The extra variants need
/// the full analysis (`full`), i.e. the try's share token.
pub fn variants(enc: &Encounter, full: Option<&FullEncounter>) -> Vec<Variant> {
    let mut out = vec![Variant::Overview];
    let Some(full) = full else { return out };
    if !full.encounter.phases.is_empty() || (!enc.success && full.encounter.lost_at_ms.is_some()) {
        out.push(Variant::Fight);
    }
    out.extend(provided_boons(enc, full).into_iter().map(Variant::Boon));
    out
}

/// Boons you supplied to your party: from your role (aHeal/aDPS → alacrity,
/// qHeal/qDPS → quickness), else wherever the analysis names you as the
/// provider for someone in your subgroup.
pub fn provided_boons(enc: &Encounter, full: &FullEncounter) -> Vec<String> {
    let role = full.players.iter().find(|p| p.account == enc.account).map(|p| p.role.as_str()).unwrap_or(&enc.role);
    match role {
        "aHeal" | "aDPS" => return vec!["alacrity".into()],
        "qHeal" | "qDPS" => return vec!["quickness".into()],
        _ => {}
    }
    let mut boons: Vec<(String, usize)> = Vec::new();
    for pl in party(enc, full) {
        for (boon, info) in &pl.boon_gaps {
            if info.provider.as_deref() == Some(enc.account.as_str()) {
                match boons.iter_mut().find(|(b, _)| b == boon) {
                    Some((_, n)) => *n += 1,
                    None => boons.push((boon.clone(), 1)),
                }
            }
        }
    }
    boons.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    boons.into_iter().map(|(b, _)| b).collect()
}

/// The other players in your subgroup (your party, max. 4 others).
pub fn party<'a>(enc: &Encounter, full: &'a FullEncounter) -> Vec<&'a model::Player> {
    let Some(me) = full.players.iter().find(|p| p.account == enc.account) else { return Vec::new() };
    full.players.iter().filter(|p| p.account != me.account && p.subgroup == me.subgroup).collect()
}

/// Today's tries at the same boss + mode as `current`, oldest first.
///
/// `day_of` maps an epoch-ms timestamp to a local calendar day key; the caller
/// owns timezone handling.
pub fn today_chips(
    tries: &[TrySummary],
    current: &Encounter,
    day_of: impl Fn(i64) -> i64,
) -> Vec<TodayChip> {
    let Some(cur) = tries.iter().find(|t| t.id == current.id) else {
        return Vec::new();
    };
    let day = day_of(cur.time_start);
    let mut same: Vec<&TrySummary> = tries
        .iter()
        .filter(|t| t.boss == cur.boss && t.mode == cur.mode && day_of(t.time_start) == day)
        .collect();
    same.sort_by_key(|t| t.time_start);
    same.iter()
        .map(|t| TodayChip { success: t.success, boss_hp_left: t.boss_hp_left, is_current: t.id == cur.id })
        .collect()
}

/// A Discord-friendly plain-text summary.
pub fn summary_text(e: &Encounter, lang: Lang) -> String {
    use text::*;
    let head = if e.success {
        format!("**KILL {}**", fmt_duration(e.duration_ms))
    } else {
        format!("**WIPE {} %**", fmt_hp(e.boss_hp_left, lang))
    };
    let mode = mode_label(&e.mode).map(|m| format!(" {m}")).unwrap_or_default();
    let mut out = format!(
        "{head} · {}{mode} · Try {}\n{} DPS · {} · {}\n",
        e.boss,
        e.try_number,
        fmt_int(e.target_dps, lang),
        e.profession,
        lang.role(&e.role)
    );
    if let Some(b) = &e.boons {
        out += &format!("{}: {}\n", lang.boon_name(&b.boon), lang.squad_uptime(b.squad_uptime));
    }
    let fails: Vec<String> = e
        .mechanics
        .iter()
        .filter(|m| m.fails > 0)
        .map(|m| format!("{}× {}", m.fails, short_mechanic(if m.text.is_empty() { &m.name } else { &m.text })))
        .collect();
    out += &format!("{} {}", e.fail_points, lang.errors(e.fail_points));
    if !fails.is_empty() {
        out += &format!(": {}", fails.join(", "));
    }
    out += &format!(" · {} {}", e.tasks, lang.contributions(e.tasks));
    if e.downs == 0 && e.deaths == 0 {
        out += &format!(" · {}", lang.no_downs_deaths());
    } else {
        out += &format!(" · {}", lang.downs_deaths(e.downs, e.deaths));
    }
    out
}
