//! Shapes of the Quagorious `/api/blish/*` responses.
//!
//! Every struct is `#[serde(default)]`, so a field the server drops later
//! doesn't break the whole card.

use std::collections::HashMap;

use serde::{Deserialize, Deserializer};

/// `GET /api/blish/encounters` → `{ "tries": [...] }` (newest first).
#[derive(Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct TryList {
    pub tries: Vec<TrySummary>,
}

#[derive(Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct TrySummary {
    pub id: u64,
    pub boss: String,
    pub mode: String,
    pub success: bool,
    /// Boss HP left in percent (0–100).
    pub boss_hp_left: f64,
    /// Unix epoch milliseconds.
    pub time_start: i64,
}

/// `GET /api/blish/encounters/<id>?locale=..`
#[derive(Deserialize, Clone, Debug, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct Encounter {
    pub id: u64,
    pub boss: String,
    pub mode: String,
    pub success: bool,
    pub duration_ms: u64,
    /// Boss HP left in percent (0–100).
    pub boss_hp_left: f64,
    pub try_number: u32,
    pub account: String,
    pub profession: String,
    pub role: String,
    pub commander: bool,
    pub target_dps: f64,
    pub downs: u32,
    pub deaths: u32,
    pub fail_points: u32,
    pub tasks: u32,
    pub reference: Option<Reference>,
    pub mechanics: Vec<Mechanic>,
    pub boons: Option<Boons>,
}

/// Your earlier tries on this boss with the same profession + role.
#[derive(Deserialize, Clone, Debug, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct Reference {
    pub tries: u32,
    pub avg_dps: f64,
    pub best_dps: f64,
}

#[derive(Deserialize, Clone, Debug, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct Mechanic {
    pub name: String,
    pub fails: u32,
    pub tasks: u32,
    /// Localized description (per `?locale=`).
    pub text: String,
}

/// Present when you were the boon provider (e.g. alacrity heal).
#[derive(Deserialize, Clone, Debug, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct Boons {
    pub boon: String,
    /// Percent, 0–100.
    pub squad_uptime: f64,
    pub gaps: Vec<BoonGap>,
}

#[derive(Deserialize, Clone, Debug, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct BoonGap {
    pub cause: String,
    pub active_ms: u64,
}

// ---------------------------------------------------------------------------
// Full analysis: `GET /api/encounters/<id>?share=<token>` (website API, public
// with the try's share token). Only the parts the cards use are modelled.
// ---------------------------------------------------------------------------

/// Treats `null` like a missing field (the website API nulls freely).
fn nd<'de, D: Deserializer<'de>, T: Default + Deserialize<'de>>(d: D) -> Result<T, D::Error> {
    Ok(Option::<T>::deserialize(d)?.unwrap_or_default())
}

#[derive(Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct FullEncounter {
    #[serde(deserialize_with = "nd")]
    pub encounter: FullHeader,
    #[serde(deserialize_with = "nd")]
    pub players: Vec<Player>,
}

#[derive(Deserialize, Clone, Debug, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct FullHeader {
    pub boss_id: Option<u64>,
    #[serde(deserialize_with = "nd")]
    pub duration_ms: u64,
    #[serde(deserialize_with = "nd")]
    pub phases: Vec<Phase>,
    /// From here on the try was lost (half the squad never stood again).
    pub lost_at_ms: Option<u64>,
    /// Account whose death started the loss.
    pub lost_by: Option<String>,
    /// Deaths after `lost_at_ms`.
    #[serde(deserialize_with = "nd")]
    pub lost_deaths: u32,
    /// Deaths/downs before `lost_at_ms` (whole fight on a kill).
    #[serde(deserialize_with = "nd")]
    pub deaths: u32,
    #[serde(deserialize_with = "nd")]
    pub downs: u32,
}

#[derive(Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct Phase {
    #[serde(deserialize_with = "nd")]
    pub name: String,
    #[serde(deserialize_with = "nd")]
    pub start: u64,
    #[serde(deserialize_with = "nd")]
    pub end: u64,
}

#[derive(Deserialize, Clone, Debug, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct Player {
    #[serde(deserialize_with = "nd")]
    pub account: String,
    #[serde(deserialize_with = "nd")]
    pub character: String,
    #[serde(deserialize_with = "nd")]
    pub profession: String,
    #[serde(deserialize_with = "nd")]
    pub role: String,
    #[serde(deserialize_with = "nd")]
    pub target_dps: f64,
    /// Party (1–15); boons like alacrity/quickness go to your own party.
    #[serde(deserialize_with = "nd")]
    pub subgroup: u32,
    /// Before the loss, on a wipe.
    #[serde(deserialize_with = "nd")]
    pub downs: u32,
    #[serde(deserialize_with = "nd")]
    pub deaths: u32,
    #[serde(deserialize_with = "nd")]
    pub uptime_weighted: HashMap<String, Option<f64>>,
    #[serde(deserialize_with = "nd")]
    pub boon_gaps: HashMap<String, BoonGapInfo>,
}

impl Player {
    pub fn display_name(&self) -> &str {
        if self.character.is_empty() { &self.account } else { &self.character }
    }
}

#[derive(Deserialize, Clone, Debug, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct BoonGapInfo {
    /// Who was supposed to provide this boon to the player.
    pub provider: Option<String>,
    #[serde(deserialize_with = "nd")]
    pub gap_ms: u64,
    #[serde(deserialize_with = "nd")]
    pub gaps: Vec<BoonGap>,
}
