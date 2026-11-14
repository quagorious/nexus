//! Turning an encounter into rendered card variants, plus the two small
//! on-disk stores they need: share tokens and GW2 spec icons.

use std::{
    collections::{HashMap, HashSet},
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, SystemTime},
};

use chrono::{Datelike, Local, TimeZone};
use quagorious_card::{
    BossImage, CardContext, CardInput, Encounter, FullEncounter, IconMask, Lang, SpecInfo, Specs, TrySummary, Variant,
    render, summary_text, today_chips, variants,
};

use crate::net;

static CARD_GEN: AtomicU64 = AtomicU64::new(1);

/// One rendered variant of a card.
pub struct VariantView {
    /// Stable kind, e.g. "overview", "fight", "boon:alacrity" (to keep the
    /// carousel on the same kind when the next try arrives).
    pub key: String,
    pub label: String,
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    pub png: Vec<u8>,
    pub preview_png: Vec<u8>,
    pub preview_size: [f32; 2],
}

/// All variants of one try, ready for display and copying.
pub struct CardView {
    pub id: u64,
    pub generation: u64,
    pub variants: Vec<VariantView>,
    pub text: String,
    pub url: Option<String>,
}

pub fn variant_key(v: &Variant) -> String {
    match v {
        Variant::Overview => "overview".into(),
        Variant::Fight => "fight".into(),
        Variant::Boon(b) => format!("boon:{b}"),
    }
}

pub fn local_day(ms: i64) -> i64 {
    Local
        .timestamp_millis_opt(ms)
        .single()
        .map(|d| d.date_naive().num_days_from_ce() as i64)
        .unwrap_or(ms.div_euclid(86_400_000))
}

pub fn build(
    enc: &Encounter,
    full: Option<&FullEncounter>,
    tries: &[TrySummary],
    lang: Lang,
    url: Option<String>,
    specs: &Specs,
    boss_image: Option<BossImage>,
) -> CardView {
    let mut text = summary_text(enc, lang);
    if let Some(url) = &url {
        text = format!("{text}\n{url}");
    }
    let stamp = tries
        .iter()
        .find(|t| t.id == enc.id)
        .and_then(|t| Local.timestamp_millis_opt(t.time_start).single())
        .map(|d| match lang {
            Lang::De => d.format("%d.%m.%Y · %H:%M").to_string(),
            Lang::En => d.format("%Y-%m-%d · %H:%M").to_string(),
        })
        .unwrap_or_default();
    let ctx = CardContext { today: today_chips(tries, enc, local_day), stamp, boss_image };
    let inp = CardInput { enc, full, ctx: &ctx, specs, lang };
    let variants = variants(enc, full)
        .iter()
        .map(|v| {
            let big = render(&inp, v, 2.0);
            let preview = render(&inp, v, 1.0);
            VariantView {
                key: variant_key(v),
                label: v.label(lang),
                width: big.width,
                height: big.height,
                rgba: big.rgba,
                png: big.png,
                preview_size: [preview.width as f32, preview.height as f32],
                preview_png: preview.png,
            }
        })
        .collect();
    CardView { id: enc.id, generation: CARD_GEN.fetch_add(1, Ordering::Relaxed), variants, text, url }
}

// ---------------------------------------------------------------------------
// Share tokens (id → token), learned from live events, kept across restarts.
// ---------------------------------------------------------------------------

fn data_path(file: &str) -> Option<PathBuf> {
    let dir = nexus::paths::get_addon_dir(crate::ADDON_NAME).ok()?;
    let _ = fs::create_dir_all(&dir);
    Some(dir.join(file))
}

pub fn load_tokens() -> HashMap<u64, String> {
    data_path("tokens.json")
        .and_then(|p| fs::read_to_string(p).ok())
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

pub fn save_tokens(tokens: &HashMap<u64, String>) {
    const KEEP: usize = 2000;
    let mut ids: Vec<u64> = tokens.keys().copied().collect();
    ids.sort_unstable_by(|a, b| b.cmp(a));
    ids.truncate(KEEP);
    let kept: HashMap<u64, &String> = ids.iter().map(|id| (*id, &tokens[id])).collect();
    if let (Some(path), Ok(text)) = (data_path("tokens.json"), serde_json::to_string(&kept)) {
        let _ = fs::write(path, text);
    }
}

// ---------------------------------------------------------------------------
// Spec icons from the GW2 API, cached under `icons/`.
// ---------------------------------------------------------------------------

#[derive(Default)]
pub struct SpecStore {
    pub specs: Specs,
    /// Spec/profession name → (profession, icon URL).
    index: HashMap<String, (String, String)>,
    index_loaded: bool,
    tried: HashSet<String>,
}

impl SpecStore {
    fn load_index(&mut self) {
        if self.index_loaded {
            return;
        }
        self.index_loaded = true;
        let cache = data_path("gw2_specs.json");
        let fresh = cache
            .as_ref()
            .and_then(|p| fs::metadata(p).ok())
            .and_then(|m| m.modified().ok())
            .and_then(|t| SystemTime::now().duration_since(t).ok())
            .is_some_and(|age| age < Duration::from_secs(14 * 86_400));
        let raw: Option<(String, String)> = if fresh {
            cache
                .as_ref()
                .and_then(|p| fs::read_to_string(p).ok())
                .and_then(|t| serde_json::from_str(&t).ok())
        } else {
            None
        };
        let raw = match raw {
            Some(r) => r,
            None => match net::gw2_specs_raw() {
                Ok(r) => {
                    if let (Some(p), Ok(t)) = (cache.as_ref(), serde_json::to_string(&r)) {
                        let _ = fs::write(p, t);
                    }
                    r
                }
                Err(e) => {
                    log::warn!("could not load GW2 specializations: {e}");
                    self.index_loaded = false; // try again next card
                    return;
                }
            },
        };
        let (specs, profs) = raw;
        for s in serde_json::from_str::<Vec<serde_json::Value>>(&specs).unwrap_or_default() {
            if s["elite"].as_bool() != Some(true) {
                continue;
            }
            if let (Some(n), Some(p), Some(u)) =
                (s["name"].as_str(), s["profession"].as_str(), s["profession_icon_big"].as_str())
            {
                self.index.insert(n.into(), (p.into(), u.into()));
            }
        }
        for p in serde_json::from_str::<Vec<serde_json::Value>>(&profs).unwrap_or_default() {
            if let (Some(n), Some(u)) = (p["name"].as_str(), p["icon_big"].as_str()) {
                self.index.insert(n.into(), (n.into(), u.into()));
            }
        }
    }

    /// Makes sure every spec in `names` has an entry (icon if available).
    pub fn ensure<'a>(&mut self, names: impl IntoIterator<Item = &'a str>) {
        let missing: Vec<&str> = names.into_iter().filter(|n| !n.is_empty() && !self.tried.contains(*n)).collect();
        if missing.is_empty() {
            return;
        }
        self.load_index();
        if !self.index_loaded {
            return;
        }
        let icon_dir = data_path("icons");
        if let Some(d) = &icon_dir {
            let _ = fs::create_dir_all(d);
        }
        for name in missing {
            self.tried.insert(name.to_string());
            let Some((profession, url)) = self.index.get(name).cloned() else {
                continue;
            };
            let file = icon_dir.as_ref().map(|d| d.join(format!("{name}.png")));
            let bytes = file.as_ref().and_then(|f| fs::read(f).ok()).or_else(|| {
                let b = net::fetch_bytes(&url).ok()?;
                if let Some(f) = &file {
                    let _ = fs::write(f, &b);
                }
                Some(b)
            });
            let icon = bytes.and_then(|b| IconMask::from_png(&b));
            self.specs.insert(name.to_string(), SpecInfo { profession, icon });
        }
    }
}

// ---------------------------------------------------------------------------
// Boss artwork: boss id → image URL (same table quagorious.gg uses), cached
// under `bosses/`.
// ---------------------------------------------------------------------------

#[derive(Default)]
pub struct BossImages {
    urls: Option<HashMap<u64, String>>,
    cache: HashMap<u64, Option<BossImage>>,
}

impl BossImages {
    pub fn get(&mut self, boss_id: u64) -> Option<BossImage> {
        if let Some(img) = self.cache.get(&boss_id) {
            return img.clone();
        }
        let urls = self.urls.get_or_insert_with(|| {
            serde_json::from_str::<HashMap<String, String>>(include_str!("../assets/boss_images.json"))
                .unwrap_or_default()
                .into_iter()
                .filter_map(|(k, v)| Some((k.parse().ok()?, v)))
                .collect()
        });
        let img = urls.get(&boss_id).cloned().and_then(|url| {
            let file = data_path("bosses").map(|d| {
                let _ = fs::create_dir_all(&d);
                d.join(format!("{boss_id}.png"))
            });
            let bytes = file.as_ref().and_then(|f| fs::read(f).ok()).or_else(|| match net::fetch_bytes(&url) {
                Ok(b) => {
                    if let Some(f) = &file {
                        let _ = fs::write(f, &b);
                    }
                    Some(b)
                }
                Err(e) => {
                    log::warn!("boss image {boss_id}: {e}");
                    None
                }
            })?;
            BossImage::from_png(&bytes)
        });
        self.cache.insert(boss_id, img.clone());
        img
    }
}
