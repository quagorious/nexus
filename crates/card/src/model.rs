//! Shapes of the Quagorious `/api/blish/*` responses.
//!
//! Everything optional is `#[serde(default)]` so a field the server adds,
//! drops or nulls later doesn't break the whole card.

use serde::Deserialize;

/// `GET /api/blish/encounters` → `{ "tries": [...] }` (newest first).
#[derive(Deserialize, Clone, Debug, Default)]
pub struct TryList {
    #[serde(default)]
    pub tries: Vec<TrySummary>,
}

#[derive(Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TrySummary {
    pub id: u64,
    #[serde(default)]
    pub boss: String,
    #[serde(default)]
    pub mode: String,
    #[serde(default)]
    pub success: bool,
    /// Boss HP left in percent (0–100).
    #[serde(default)]
    pub boss_hp_left: f64,
    /// Unix epoch milliseconds.
    #[serde(default)]
    pub time_start: i64,
}

/// `GET /api/blish/encounters/<id>?locale=..`
#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Encounter {
    pub id: u64,
    #[serde(default)]
    pub boss: String,
    #[serde(default)]
    pub mode: String,
    #[serde(default)]
    pub success: bool,
    #[serde(default)]
    pub duration_ms: u64,
    /// Boss HP left in percent (0–100).
    #[serde(default)]
    pub boss_hp_left: f64,
    #[serde(default)]
    pub try_number: u32,
    #[serde(default)]
    pub account: String,
    #[serde(default)]
    pub profession: String,
    #[serde(default)]
    pub role: String,
    #[serde(default)]
    pub commander: bool,
    #[serde(default)]
    pub target_dps: f64,
    #[serde(default)]
    pub downs: u32,
    #[serde(default)]
    pub deaths: u32,
    #[serde(default)]
    pub fail_points: u32,
    #[serde(default)]
    pub tasks: u32,
    #[serde(default)]
    pub reference: Option<Reference>,
    #[serde(default)]
    pub mechanics: Vec<Mechanic>,
    #[serde(default)]
    pub boons: Option<Boons>,
}

/// Your earlier tries on this boss with the same profession + role.
#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Reference {
    #[serde(default)]
    pub tries: u32,
    #[serde(default)]
    pub avg_dps: f64,
    #[serde(default)]
    pub best_dps: f64,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Mechanic {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub fails: u32,
    #[serde(default)]
    pub fail_points: u32,
    #[serde(default)]
    pub tasks: u32,
    /// Localized description (per `?locale=`).
    #[serde(default)]
    pub text: String,
}

/// Present when you were the boon provider (e.g. alacrity heal).
#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Boons {
    #[serde(default)]
    pub boon: String,
    /// Percent, 0–100.
    #[serde(default)]
    pub squad_uptime: f64,
    #[serde(default)]
    pub gaps: Vec<BoonGap>,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct BoonGap {
    #[serde(default)]
    pub cause: String,
    #[serde(default)]
    pub active_ms: u64,
}

// ---------------------------------------------------------------------------
// Full analysis: `GET /api/encounters/<id>?share=<token>` (website API, public
// with the try's share token). Only the parts the cards use are modelled.
// ---------------------------------------------------------------------------

use std::collections::HashMap;

use serde::Deserializer;

/// Treats `null` like a missing field.
fn nd<'de, D: Deserializer<'de>, T: Default + Deserialize<'de>>(d: D) -> Result<T, D::Error> {
    Ok(Option::<T>::deserialize(d)?.unwrap_or_default())
}

#[derive(Deserialize, Clone, Debug, Default)]
pub struct FullEncounter {
    #[serde(default, deserialize_with = "nd")]
    pub encounter: FullHeader,
    #[serde(default, deserialize_with = "nd")]
    pub players: Vec<Player>,
}

#[derive(Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct FullHeader {
    #[serde(default)]
    pub boss_id: Option<u64>,
    #[serde(default, deserialize_with = "nd")]
    pub duration_ms: u64,
    #[serde(default, deserialize_with = "nd")]
    pub phases: Vec<Phase>,
    /// From here on the try was lost (half the squad never stood again).
    #[serde(default)]
    pub lost_at_ms: Option<u64>,
    /// Account whose death started the loss.
    #[serde(default)]
    pub lost_by: Option<String>,
    /// Deaths/downs after `lost_at_ms`.
    #[serde(default, deserialize_with = "nd")]
    pub lost_deaths: u32,
    #[serde(default, deserialize_with = "nd")]
    pub lost_downs: u32,
    /// Deaths/downs before `lost_at_ms` (whole fight on a kill).
    #[serde(default, deserialize_with = "nd")]
    pub deaths: u32,
    #[serde(default, deserialize_with = "nd")]
    pub downs: u32,
}

#[derive(Deserialize, Clone, Debug, Default)]
pub struct Phase {
    #[serde(default, deserialize_with = "nd")]
    pub name: String,
    #[serde(default, deserialize_with = "nd")]
    pub start: u64,
    #[serde(default, deserialize_with = "nd")]
    pub end: u64,
}

#[derive(Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct Player {
    #[serde(default, deserialize_with = "nd")]
    pub account: String,
    #[serde(default, deserialize_with = "nd")]
    pub character: String,
    #[serde(default, deserialize_with = "nd")]
    pub profession: String,
    #[serde(default, deserialize_with = "nd")]
    pub role: String,
    #[serde(default, deserialize_with = "nd")]
    pub target_dps: f64,
    /// Party (1–15); boons like alacrity/quickness go to your own party.
    #[serde(default, deserialize_with = "nd")]
    pub subgroup: u32,
    /// Before the loss, on a wipe.
    #[serde(default, deserialize_with = "nd")]
    pub downs: u32,
    #[serde(default, deserialize_with = "nd")]
    pub deaths: u32,
    #[serde(default, deserialize_with = "nd")]
    pub uptime_weighted: HashMap<String, Option<f64>>,
    #[serde(default, deserialize_with = "nd")]
    pub boon_gaps: HashMap<String, BoonGapInfo>,
}

impl Player {
    pub fn display_name(&self) -> &str {
        if self.character.is_empty() { &self.account } else { &self.character }
    }
}

#[derive(Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct BoonGapInfo {
    /// Who was supposed to provide this boon to the player.
    #[serde(default)]
    pub provider: Option<String>,
    #[serde(default, deserialize_with = "nd")]
    pub gap_ms: u64,
    #[serde(default, deserialize_with = "nd")]
    pub gaps: Vec<BoonGap>,
}
