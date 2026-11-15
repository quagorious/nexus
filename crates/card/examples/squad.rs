//! Print the squad view for a series of full analyses (today's tries at one
//! boss, oldest first; the last file is the try shown) and render it as the
//! "Squad" card.
//!
//! cargo run -p quagorious_card --example squad -- <de|en> <account> <out.png> [addon_dir] -- <full.json>...
//!
//! `addon_dir` is the addon's data folder (`gw2_specs.json`, `icons/`,
//! `bosses/`), for spec icons and boss artwork.

use std::collections::HashMap;

use quagorious_card::{
    BossImage, CardContext, CardInput, Encounter, FullEncounter, IconMask, Lang, SpecInfo, Specs, Variant, render,
    squad::{SquadInput, TodayTry, build},
};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let split = args.iter().position(|a| a == "--").expect("-- before the analyses");
    let lang = Lang::from_code(&args[1]);
    let account = &args[2];
    let out = &args[3];
    let dir = args.get(4).filter(|_| split > 4);
    let fulls: Vec<FullEncounter> = args[split + 1..]
        .iter()
        .map(|p| serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap())
        .collect();

    let mut specs = Specs::new();
    if let Some(dir) = dir {
        let index: HashMap<String, (String, String)> =
            serde_json::from_str(&std::fs::read_to_string(format!("{dir}/gw2_specs.json")).unwrap()).unwrap();
        for (name, (profession, _)) in index {
            let icon = std::fs::read(format!("{dir}/icons/{name}.png")).ok().and_then(|b| IconMask::from_png(&b));
            specs.insert(name, SpecInfo { profession, icon });
        }
    }

    let today: Vec<TodayTry> = fulls
        .iter()
        .enumerate()
        .map(|(i, f)| TodayTry {
            id: i as u64 + 1,
            success: f.encounter.success,
            boss_hp_left: f.encounter.boss_hp_left,
            full: Some(f),
        })
        .collect();
    let full = fulls.last().unwrap();
    let view = build(&SquadInput {
        id: fulls.len() as u64,
        full,
        boss: "Erntetempel",
        mode: "cm",
        account,
        today: &today,
        stamp: "15.11.2026 · 20:56",
        texts: &HashMap::new(),
        specs: &specs,
        lang,
    });
    println!("{view:#?}");

    let boss_image = dir
        .zip(full.encounter.boss_id)
        .and_then(|(d, id)| std::fs::read(format!("{d}/bosses/{id}.png")).ok())
        .and_then(|b| BossImage::from_png(&b));
    let ctx = CardContext { today: Vec::new(), stamp: "15.11.2026 · 20:56".into(), boss_image };
    let enc = Encounter { account: account.clone(), ..Encounter::default() };
    let inp = CardInput { enc: &enc, full: Some(full), ctx: &ctx, specs: &specs, lang, squad: Some(&view) };
    let card = render(&inp, &Variant::Squad, 2.0);
    std::fs::write(out, card.png).unwrap();
    eprintln!("wrote {out} ({}x{})", card.width, card.height);
}
