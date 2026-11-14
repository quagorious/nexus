//! Render every card variant of a sample encounter to PNGs on the host.
//!
//! cargo run -p quagorious_card --example render -- \
//!   <encounter.json> <tries.json> <de|en> <out_prefix> [full.json] [specs_dir]
//!
//! `specs_dir` holds `specs.json` (GW2 /v2/specializations?ids=all) and
//! `icons/<Spec>.png`.

use std::collections::HashMap;

use quagorious_card::{
    CardContext, CardInput, Encounter, FullEncounter, IconMask, Lang, SpecInfo, TryList, render,
    today_chips, variants,
};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let read = |p: &str| std::fs::read_to_string(p).unwrap();
    let enc: Encounter = serde_json::from_str(&read(&args[1])).unwrap();
    let tries: TryList = serde_json::from_str(&read(&args[2])).unwrap_or_default();
    let lang = Lang::from_code(&args[3]);
    let prefix = &args[4];
    let full: Option<FullEncounter> = args.get(5).map(|p| serde_json::from_str(&read(p)).unwrap());

    let mut specs = HashMap::new();
    if let Some(dir) = args.get(6) {
        let list: Vec<serde_json::Value> = serde_json::from_str(&read(&format!("{dir}/specs.json"))).unwrap();
        for s in list {
            let (Some(name), Some(prof)) = (s["name"].as_str(), s["profession"].as_str()) else { continue };
            let icon = std::fs::read(format!("{dir}/icons/{name}.png")).ok().and_then(|b| IconMask::from_png(&b));
            // Base specs share their profession's name; elites win on clashes.
            if s["elite"].as_bool() == Some(true) || !specs.contains_key(name) {
                specs.insert(name.to_string(), SpecInfo { profession: prof.to_string(), icon });
            }
        }
    }

    let today = today_chips(&tries.tries, &enc, |ms| ms.div_euclid(86_400_000));
    let boss_image = args.get(6).zip(full.as_ref().and_then(|f| f.encounter.boss_id)).and_then(|(dir, id)| {
        std::fs::read(format!("{dir}/bosses/{id}.png")).ok().and_then(|b| quagorious_card::BossImage::from_png(&b))
    });
    let ctx = CardContext { today, stamp: "14.11.2026 · 20:31".into(), boss_image };
    let inp = CardInput { enc: &enc, full: full.as_ref(), ctx: &ctx, specs: &specs, lang };
    for (i, v) in variants(&enc, full.as_ref()).iter().enumerate() {
        let card = render(&inp, v, 2.0);
        let out = format!("{prefix}_{i}.png");
        std::fs::write(&out, &card.png).unwrap();
        println!("{out}: {} ({}x{})", v.label(lang), card.width, card.height);
    }
}
