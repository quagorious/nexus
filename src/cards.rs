//! Turning an encounter into rendered card variants, plus the two small
//! on-disk stores they need: share tokens and GW2 spec icons.

use std::{
    collections::{HashMap, HashSet},
    fs,
    hash::{DefaultHasher, Hash, Hasher},
    sync::LazyLock,
    time::{Duration, SystemTime},
};

use chrono::{Datelike, Local, TimeZone};
use quagorious_card::{
    BossImage, CardContext, CardInput, Encounter, FullEncounter, IconMask, Lang, SpecInfo, Specs, TrySummary, Variant,
    render, summary_text, today_chips, variants,
};

use serde::Deserialize;

use crate::{data_path, net};

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
    /// Nexus texture identifier, derived from the preview's content so an
    /// unchanged card re-uses its texture instead of registering a new one.
    pub preview_id: String,
    pub preview_size: [f32; 2],
}

/// All variants of one try, ready for display and copying.
pub struct CardView {
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
            let mut hash = DefaultHasher::new();
            preview.png.hash(&mut hash);
            VariantView {
                key: variant_key(v),
                label: v.label(lang),
                width: big.width,
                height: big.height,
                rgba: big.rgba,
                png: big.png,
                preview_id: format!("QUAGORIOUS_CARD_{:016x}", hash.finish()),
                preview_size: [preview.width as f32, preview.height as f32],
                preview_png: preview.png,
            }
        })
        .collect();
    CardView { variants, text, url }
}

// ---------------------------------------------------------------------------
// Share tokens (id → token), learned from live events, kept across restarts.
// ---------------------------------------------------------------------------

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
// Downloaded images, cached in the addon directory.
// ---------------------------------------------------------------------------

/// Decodes `file` from the addon directory, downloading it from `url` when
/// it's missing or unreadable. Only a download that decodes is written.
fn cached<T>(file: &str, url: &str, decode: impl Fn(&[u8]) -> Option<T>) -> Option<T> {
    let path = data_path(file);
    if let Some(v) = path.as_ref().and_then(|p| fs::read(p).ok()).and_then(|b| decode(&b)) {
        return Some(v);
    }
    let bytes = net::fetch_bytes(url).map_err(|e| log::warn!("{file}: {e}")).ok()?;
    let v = decode(&bytes)?;
    if let Some(p) = path {
        let _ = fs::write(p, &bytes);
    }
    Some(v)
}

// ---------------------------------------------------------------------------
// Spec icons from the GW2 API, cached under `icons/`.
// ---------------------------------------------------------------------------

/// Spec/profession name → (profession, icon URL).
type SpecIndex = HashMap<String, (String, String)>;

#[derive(Default)]
pub struct SpecStore {
    pub specs: Specs,
    /// `None` until it could be loaded; retried with the next card.
    index: Option<SpecIndex>,
    tried: HashSet<String>,
}

/// The index from `gw2_specs.json`, or from the GW2 API when that is missing
/// or older than 14 days.
fn load_spec_index() -> Option<SpecIndex> {
    #[derive(Deserialize)]
    struct Spec {
        name: String,
        profession: String,
        #[serde(default)]
        elite: bool,
        profession_icon_big: Option<String>,
    }
    #[derive(Deserialize)]
    struct Profession {
        name: String,
        icon_big: String,
    }

    let cache = data_path("gw2_specs.json");
    let fresh = cache
        .as_ref()
        .and_then(|p| fs::metadata(p).ok())
        .and_then(|m| m.modified().ok())
        .and_then(|t| SystemTime::now().duration_since(t).ok())
        .is_some_and(|age| age < Duration::from_secs(14 * 86_400));
    if fresh
        && let Some(index) =
            cache.as_ref().and_then(|p| fs::read_to_string(p).ok()).and_then(|t| serde_json::from_str(&t).ok())
    {
        return Some(index);
    }

    let fetched = net::gw2_all::<Spec>("/v2/specializations")
        .and_then(|specs| Ok((specs, net::gw2_all::<Profession>("/v2/professions")?)));
    let (specs, professions) = match fetched {
        Ok(r) => r,
        Err(e) => {
            log::warn!("could not load GW2 specializations: {e}");
            return None;
        }
    };
    let mut index = SpecIndex::new();
    for s in specs.into_iter().filter(|s| s.elite) {
        if let Some(icon) = s.profession_icon_big {
            index.insert(s.name, (s.profession, icon));
        }
    }
    for p in professions {
        index.insert(p.name.clone(), (p.name, p.icon_big));
    }
    if let (Some(p), Ok(t)) = (cache, serde_json::to_string(&index)) {
        let _ = fs::write(p, t);
    }
    Some(index)
}

impl SpecStore {
    /// Makes sure every spec in `names` has an entry (icon if available).
    pub fn ensure<'a>(&mut self, names: impl IntoIterator<Item = &'a str>) {
        let missing: Vec<&str> = names.into_iter().filter(|n| !n.is_empty() && !self.tried.contains(*n)).collect();
        if missing.is_empty() {
            return;
        }
        if self.index.is_none() {
            self.index = load_spec_index();
        }
        let Some(index) = &self.index else { return };
        for name in missing {
            self.tried.insert(name.to_string());
            let Some((profession, url)) = index.get(name) else { continue };
            let icon = cached(&format!("icons/{name}.png"), url, IconMask::from_png);
            self.specs.insert(name.to_string(), SpecInfo { profession: profession.clone(), icon });
        }
    }
}

// ---------------------------------------------------------------------------
// Boss artwork: boss id → image URL (same table quagorious.gg uses), cached
// under `bosses/`.
// ---------------------------------------------------------------------------

static BOSS_URLS: LazyLock<HashMap<u64, String>> =
    LazyLock::new(|| serde_json::from_str(include_str!("../assets/boss_images.json")).unwrap_or_default());

#[derive(Default)]
pub struct BossImages {
    cache: HashMap<u64, Option<BossImage>>,
}

impl BossImages {
    pub fn get(&mut self, boss_id: u64) -> Option<BossImage> {
        self.cache
            .entry(boss_id)
            .or_insert_with(|| {
                cached(&format!("bosses/{boss_id}.png"), BOSS_URLS.get(&boss_id)?, BossImage::from_png)
            })
            .clone()
    }
}
