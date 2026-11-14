//! Quagorious encounter model and share-card renderer.
//!
//! Kept free of any Windows/Nexus dependency so the card can be rendered and
//! eyeballed on the Linux host (`cargo run -p quagorious_card --example render`).

pub mod model;
pub mod render;
pub mod text;

pub use model::{Encounter, FullEncounter, TryList, TrySummary};
pub use render::{
    BossImage, CardContext, CardInput, IconMask, MATTE, RenderedCard, SpecInfo, Specs, TodayChip, Variant, render,
};
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn enc(v: serde_json::Value) -> Encounter {
        serde_json::from_value(v).unwrap()
    }

    fn full(v: serde_json::Value) -> FullEncounter {
        serde_json::from_value(v).unwrap()
    }

    fn squad() -> FullEncounter {
        full(json!({
            "encounter": { "bossId": 17154, "durationMs": 200000, "phases": [{ "name": "P1", "start": 0, "end": 200000 }] },
            "players": [
                { "account": "me.1234", "role": "DPS", "subgroup": 1 },
                { "account": "a.1", "subgroup": 1, "boonGaps": {
                    "might": { "provider": "me.1234", "gapMs": 1000 },
                    "fury": { "provider": "me.1234" } } },
                { "account": "b.2", "subgroup": 1, "boonGaps": { "might": { "provider": "me.1234" } } },
                { "account": "c.3", "subgroup": 2, "boonGaps": { "swiftness": { "provider": "me.1234" } } }
            ]
        }))
    }

    #[test]
    fn missing_and_null_fields_fall_back_to_defaults() {
        let e = enc(json!({ "id": 7, "bossHpLeft": 12.5 }));
        assert_eq!((e.id, e.boss_hp_left, e.boss.as_str(), e.reference.is_none()), (7, 12.5, "", true));

        let f = full(json!({
            "encounter": { "durationMs": null, "phases": null, "lostAtMs": null },
            "players": [{ "account": "x.1", "character": null, "uptimeWeighted": { "alacrity": null }, "boonGaps": null }]
        }));
        assert_eq!(f.encounter.duration_ms, 0);
        assert!(f.encounter.phases.is_empty() && f.encounter.lost_at_ms.is_none());
        assert_eq!(f.players[0].display_name(), "x.1");
        assert_eq!(f.players[0].uptime_weighted["alacrity"], None);
        assert!(full(json!({ "players": null })).players.is_empty());
    }

    #[test]
    fn only_the_overview_without_full_analysis() {
        assert_eq!(variants(&enc(json!({ "id": 1 })), None), [Variant::Overview]);
    }

    #[test]
    fn fight_card_needs_phases_or_a_lost_wipe() {
        let wipe = enc(json!({ "id": 1, "success": false }));
        let bare = full(json!({ "encounter": {} }));
        assert_eq!(variants(&wipe, Some(&bare)), [Variant::Overview]);
        let lost = full(json!({ "encounter": { "lostAtMs": 90000 } }));
        assert_eq!(variants(&wipe, Some(&lost)), [Variant::Overview, Variant::Fight]);
        let kill = enc(json!({ "id": 1, "success": true }));
        assert_eq!(variants(&kill, Some(&lost)), [Variant::Overview]);
    }

    #[test]
    fn role_decides_the_provided_boon() {
        let e = enc(json!({ "id": 1, "account": "me.1234", "role": "DPS" }));
        let f = full(json!({ "players": [{ "account": "me.1234", "role": "qHeal", "subgroup": 1 }] }));
        assert_eq!(provided_boons(&e, &f), ["quickness"]);
        // Without a player entry the module API's role counts.
        let e = enc(json!({ "id": 1, "account": "me.1234", "role": "aDPS" }));
        assert_eq!(provided_boons(&e, &full(json!({}))), ["alacrity"]);
    }

    #[test]
    fn other_boons_come_from_your_subgroup_most_common_first() {
        let e = enc(json!({ "id": 1, "account": "me.1234" }));
        let f = squad();
        assert_eq!(party(&e, &f).iter().map(|p| p.account.as_str()).collect::<Vec<_>>(), ["a.1", "b.2"]);
        assert_eq!(provided_boons(&e, &f), ["might", "fury"]);
        assert_eq!(
            variants(&e, Some(&f)),
            [Variant::Overview, Variant::Fight, Variant::Boon("might".into()), Variant::Boon("fury".into())]
        );
    }

    #[test]
    fn today_chips_are_same_boss_mode_and_day_oldest_first() {
        const DAY: i64 = 86_400_000;
        let tries: Vec<TrySummary> = serde_json::from_value(json!([
            { "id": 5, "boss": "Cairn", "mode": "cm", "timeStart": DAY + 3000, "success": true },
            { "id": 4, "boss": "Cairn", "mode": "", "timeStart": DAY + 2500 },
            { "id": 3, "boss": "Deimos", "mode": "cm", "timeStart": DAY + 2000 },
            { "id": 2, "boss": "Cairn", "mode": "cm", "timeStart": DAY + 1000, "bossHpLeft": 40.0 },
            { "id": 1, "boss": "Cairn", "mode": "cm", "timeStart": 1000 }
        ]))
        .unwrap();
        let chips = today_chips(&tries, &enc(json!({ "id": 5 })), |ms| ms.div_euclid(DAY));
        let seen: Vec<_> = chips.iter().map(|c| (c.success, c.boss_hp_left, c.is_current)).collect();
        assert_eq!(seen, [(false, 40.0, false), (true, 0.0, true)]);
        assert!(today_chips(&tries, &enc(json!({ "id": 99 })), |ms| ms.div_euclid(DAY)).is_empty());
    }

    #[test]
    fn summary_text_for_a_wipe() {
        let e = enc(json!({
            "id": 1, "boss": "Cairn", "mode": "cm", "bossHpLeft": 0.5, "tryNumber": 3,
            "targetDps": 31234.0, "profession": "Virtuoso", "role": "DPS",
            "failPoints": 2, "tasks": 1, "downs": 1,
            "mechanics": [{ "name": "Green", "text": "Grün (verpasst)", "fails": 2 }, { "name": "Port", "tasks": 1 }]
        }));
        assert_eq!(
            summary_text(&e, Lang::De),
            "**WIPE 0,50 %** · Cairn CM · Try 3\n31.234 DPS · Virtuoso · DPS\n\
             2 Fehler: 2× Grün · 1 Beitrag · 1 Down"
        );
    }

    #[test]
    fn every_variant_renders() {
        let e = enc(json!({ "id": 1, "account": "me.1234", "boss": "Cairn", "success": false, "durationMs": 200000 }));
        let f = squad();
        let (ctx, specs) = (CardContext::default(), Specs::default());
        let inp = CardInput { enc: &e, full: Some(&f), ctx: &ctx, specs: &specs, lang: Lang::En };
        for v in variants(&e, Some(&f)) {
            let card = render(&inp, &v, 1.0);
            assert_eq!(card.width, 420);
            assert_eq!(card.rgba.len(), (card.width * card.height * 4) as usize);
            assert!(card.png.starts_with(b"\x89PNG"), "{v:?}");
        }
    }
}
