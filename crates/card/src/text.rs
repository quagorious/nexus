//! Labels and number formatting for the two supported locales.

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Lang {
    #[default]
    De,
    En,
}

impl Lang {
    /// The `?locale=` value the Quagorious API understands.
    pub fn code(self) -> &'static str {
        match self {
            Lang::De => "de",
            Lang::En => "en",
        }
    }

    pub fn from_code(code: &str) -> Self {
        if code.eq_ignore_ascii_case("en") { Lang::En } else { Lang::De }
    }

    fn de(self) -> bool {
        self == Lang::De
    }

    pub fn errors(self, n: u32) -> &'static str {
        match (self, n) {
            (Lang::De, _) => "Fehler",
            (Lang::En, 1) => "mistake",
            (Lang::En, _) => "mistakes",
        }
    }
    pub fn contributions(self, n: u32) -> &'static str {
        match (self, n) {
            (Lang::De, 1) => "Beitrag",
            (Lang::De, _) => "Beiträge",
            (Lang::En, 1) => "contribution",
            (Lang::En, _) => "contributions",
        }
    }
    pub fn no_downs_deaths(self) -> &'static str {
        if self.de() { "keine Downs, keine Tode" } else { "no downs, no deaths" }
    }
    /// "1 Tod, 6 Downs"; zero counts are left out.
    pub fn downs_deaths(self, downs: u32, deaths: u32) -> String {
        let (death, deaths_w, down, downs_w) = if self.de() {
            ("Tod", "Tode", "Down", "Downs")
        } else {
            ("death", "deaths", "down", "downs")
        };
        let mut parts = Vec::new();
        if deaths > 0 {
            parts.push(format!("{deaths} {}", if deaths == 1 { death } else { deaths_w }));
        }
        if downs > 0 {
            parts.push(format!("{downs} {}", if downs == 1 { down } else { downs_w }));
        }
        if parts.is_empty() { self.no_downs_deaths().to_string() } else { parts.join(", ") }
    }
    pub fn your_tries_as(self, prof: &str, role: &str) -> String {
        if self.de() {
            format!("Deine Tries hier als {prof} · {role}")
        } else {
            format!("Your tries here as {prof} · {role}")
        }
    }
    pub fn vs_avg(self, pct: f64, tries: u32) -> String {
        let sign = if pct >= 0.0 { "+" } else { "−" };
        let p = fmt_dec(pct.abs(), 0, self);
        if self.de() {
            format!("{sign}{p} % ggü. Ø · {tries} Tries")
        } else {
            format!("{sign}{p} % vs. avg · {tries} tries")
        }
    }
    pub fn boon_name(self, boon: &str) -> String {
        match (boon, self) {
            ("alacrity", Lang::De) => "Alacrität".into(),
            ("alacrity", Lang::En) => "Alacrity".into(),
            ("quickness", Lang::De) => "Schnelligkeit".into(),
            ("quickness", Lang::En) => "Quickness".into(),
            (other, _) => {
                let mut c = other.chars();
                match c.next() {
                    Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                    None => String::new(),
                }
            }
        }
    }
    pub fn your_boon(self, boon: &str) -> String {
        let name = self.boon_name(boon);
        if self.de() { format!("{name} von dir") } else { format!("Your {name}") }
    }
    pub fn squad_uptime(self, pct: f64) -> String {
        let p = fmt_dec(pct, 1, self);
        if self.de() { format!("Ø {p} % im Squad") } else { format!("Ø {p} % squad uptime") }
    }
    pub fn gaps(self) -> &'static str {
        if self.de() { "Lücken" } else { "Gaps" }
    }
    pub fn gap_cause(self, cause: &str) -> String {
        let s = match (cause, self) {
            ("removed", Lang::De) => "vom Gegner entfernt",
            ("removed", Lang::En) => "stripped",
            ("durationShort", Lang::De) => "zu kurz gehalten",
            ("durationShort", Lang::En) => "duration too short",
            ("outOfRange", Lang::De) => "außer Reichweite",
            ("outOfRange", Lang::En) => "out of range",
            ("notReached", Lang::De) => "nicht erreicht",
            ("notReached", Lang::En) => "not reached",
            ("providerDown", Lang::De) => "Quelle down",
            ("providerDown", Lang::En) => "provider down",
            ("sourceIdle", Lang::De) => "Quelle zu selten",
            ("sourceIdle", Lang::En) => "provider idle",
            ("pulseOff", Lang::De) => "Puls aus",
            ("pulseOff", Lang::En) => "pulse off",
            ("opener", Lang::De) => "Opener",
            ("opener", Lang::En) => "opener",
            ("unknown", Lang::De) => "unbekannt",
            ("unknown", Lang::En) => "unknown",
            (other, _) => other,
        };
        s.to_string()
    }
    pub fn today_at_boss(self) -> &'static str {
        if self.de() { "Heute an diesem Boss" } else { "Today at this boss" }
    }
    pub fn n_tries(self, n: usize) -> String {
        match (n, self) {
            (1, Lang::De) => "1 Try".into(),
            (1, Lang::En) => "1 try".into(),
            (_, Lang::De) => format!("{n} Tries"),
            (_, Lang::En) => format!("{n} tries"),
        }
    }
    pub fn commander(self) -> &'static str {
        if self.de() { "Kommandeur" } else { "Commander" }
    }
    pub fn role(self, role: &str) -> String {
        match role {
            "aHeal" => "Alac-Heal".into(),
            "qHeal" => "Quick-Heal".into(),
            "aDPS" => "Alac-DPS".into(),
            "qDPS" => "Quick-DPS".into(),
            other => other.into(),
        }
    }
}

impl Lang {
    pub fn variant_overview(self) -> &'static str {
        if self.de() { "Übersicht" } else { "Overview" }
    }
    pub fn fight_timeline(self) -> &'static str {
        if self.de() { "Kampfverlauf" } else { "Fight timeline" }
    }
    pub fn squad_dps(self) -> &'static str {
        if self.de() { "Squad-DPS" } else { "Squad DPS" }
    }
    /// `dps_only`: ranked among pure DPS players only.
    pub fn rank(self, rank: usize, of: usize, dps_only: bool) -> String {
        match (self, dps_only) {
            (Lang::De, true) => format!("Platz {rank} von {of} DPS"),
            (Lang::De, false) => format!("Platz {rank} von {of}"),
            (Lang::En, true) => format!("#{rank} of {of} DPS"),
            (Lang::En, false) => format!("#{rank} of {of}"),
        }
    }
    pub fn lost_from(self, at: &str) -> String {
        if self.de() { format!("Try verloren ab {at}") } else { format!("Try lost from {at}") }
    }
    pub fn first(self) -> &'static str {
        if self.de() { "zuerst " } else { "first " }
    }
    pub fn then_deaths(self, n: u32) -> String {
        match (self, n) {
            (Lang::De, 1) => " · danach 1 Tod".into(),
            (Lang::De, _) => format!(" · danach {n} Tode"),
            (Lang::En, 1) => " · then 1 death".into(),
            (Lang::En, _) => format!(" · then {n} deaths"),
        }
    }
    pub fn before_that(self, deaths: u32, downs: u32) -> String {
        let dd = self.downs_deaths(downs, deaths);
        if self.de() { format!("im Kampf davor {dd}") } else { format!("before that: {dd}") }
    }
    pub fn before_loss(self, downs: u32, deaths: u32) -> String {
        let dd = self.downs_deaths(downs, deaths);
        if self.de() { format!("{dd} vor dem Verlust") } else { format!("{dd} before the loss") }
    }
    pub fn boon_avg(self, pct: f64, players: usize) -> String {
        let p = fmt_dec(pct, 1, self);
        match (self, players) {
            (Lang::De, 1) => format!("Ø {p} % bei 1 Spieler"),
            (Lang::De, _) => format!("Ø {p} % bei {players} Spielern"),
            (Lang::En, 1) => format!("Ø {p} % on 1 player"),
            (Lang::En, _) => format!("Ø {p} % on {players} players"),
        }
    }
    /// Marks a player left out of the boon average (out of range most of the fight).
    pub fn not_in_avg(self) -> &'static str {
        if self.de() { "nicht im Ø" } else { "not in avg." }
    }
    pub fn you(self) -> &'static str {
        if self.de() { "du" } else { "you" }
    }
}

/// `12345` → `12.345` (de) / `12,345` (en).
pub fn fmt_int(v: f64, lang: Lang) -> String {
    let n = v.round().max(0.0) as u64;
    let digits = n.to_string();
    let sep = if lang == Lang::De { '.' } else { ',' };
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(sep);
        }
        out.push(c);
    }
    out
}

/// Fixed decimals with the locale's decimal separator.
pub fn fmt_dec(v: f64, decimals: usize, lang: Lang) -> String {
    let s = format!("{v:.decimals$}");
    if lang == Lang::De { s.replace('.', ",") } else { s }
}

/// Boss HP left: 2 decimals below 1 % (close wipes matter), else 1.
pub fn fmt_hp(v: f64, lang: Lang) -> String {
    fmt_dec(v, if v < 1.0 { 2 } else { 1 }, lang)
}

/// `192518` → `3:12`.
pub fn fmt_duration(ms: u64) -> String {
    let secs = ms / 1000;
    format!("{}:{:02}", secs / 60, secs % 60)
}

pub fn mode_label(mode: &str) -> Option<&'static str> {
    match mode {
        "cm" => Some("CM"),
        "lcm" => Some("LCM"),
        _ => None,
    }
}

/// `Giftwolke (in grünem Bodengift gestanden)` → `Giftwolke`.
pub fn short_mechanic(text: &str) -> &str {
    match text.find(" (") {
        Some(i) if i > 0 => &text[..i],
        _ => text,
    }
}
