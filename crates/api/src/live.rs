use serde::Deserialize;

use crate::SseEvent;

/// What a live-stream event means for the addon.
#[derive(Debug, PartialEq, Eq)]
pub enum LiveEvent {
    /// A new try was uploaded.
    NewTry { id: u64, token: Option<String>, boss_id: Option<u64> },
    /// Known but irrelevant (ping, `log.*`, `uploader.connected`, …).
    Ignore,
    /// Unrecognised shape: the caller should refresh the list.
    Unknown,
}

/// Payload of a live event (`data:` line). New stream:
/// `{"type":"try.created","id":77310,"token":"…","bossId":133122,"bossName":…,
///   "accounts":[…],"seq":48214}`; the legacy stream used `"type":"encounter"`
/// with the same fields. Pings are `event: ping` with no data.
#[derive(Deserialize)]
struct Message {
    #[serde(rename = "type", default)]
    kind: String,
    #[serde(default)]
    id: Option<u64>,
    /// Share link token: https://quagorious.gg/share/<token>
    #[serde(default)]
    token: Option<String>,
    #[serde(default, rename = "bossId")]
    boss_id: Option<u64>,
}

pub fn parse_live_event(ev: &SseEvent) -> LiveEvent {
    if ev.event == "ping" || ev.data.trim().is_empty() {
        return LiveEvent::Ignore;
    }
    match serde_json::from_str::<Message>(&ev.data) {
        Ok(Message { kind, id: Some(id), token, boss_id })
            if kind == "try.created" || kind == "encounter" =>
        {
            LiveEvent::NewTry { id, token, boss_id }
        }
        // Any other typed message (`log.*`, `try.reparse_requested`, …).
        Ok(_) => LiveEvent::Ignore,
        Err(_) => LiveEvent::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(event: &str, data: &str) -> SseEvent {
        SseEvent { event: event.into(), data: data.into() }
    }

    #[test]
    fn try_created() {
        let d = r#"{"type":"try.created","id":77310,"token":"k3VqP9xw","bossId":133122,"bossName":"Decima","cm":0,"success":false,"bossHpLeft":31.4,"timeStart":1795301822000,"durationMs":268140,"accounts":["Quaggan.1234"],"seq":48214}"#;
        assert_eq!(
            parse_live_event(&ev("", d)),
            LiveEvent::NewTry { id: 77310, token: Some("k3VqP9xw".into()), boss_id: Some(133122) }
        );
    }

    #[test]
    fn legacy_encounter() {
        let d = r#"{"type":"encounter","id":24499,"token":"abc","bossId":5,"seq":4}"#;
        assert_eq!(
            parse_live_event(&ev("", d)),
            LiveEvent::NewTry { id: 24499, token: Some("abc".into()), boss_id: Some(5) }
        );
    }

    #[test]
    fn ignored_types() {
        for t in [
            "log.reserved",
            "log.received",
            "log.parsing",
            "log.parsed",
            "log.failed",
            "log.rejected",
            "log.expired",
            "try.reparse_requested",
            "uploader.connected",
        ] {
            let d = format!(r#"{{"type":"{t}","id":1,"seq":2}}"#);
            assert_eq!(parse_live_event(&ev("", &d)), LiveEvent::Ignore, "{t}");
        }
    }

    #[test]
    fn ping_and_garbage() {
        assert_eq!(parse_live_event(&ev("ping", "")), LiveEvent::Ignore);
        assert_eq!(parse_live_event(&ev("ping", "1")), LiveEvent::Ignore);
        assert_eq!(parse_live_event(&ev("", "not json")), LiveEvent::Unknown);
        assert_eq!(parse_live_event(&ev("x", "[1,2]")), LiveEvent::Unknown);
    }
}
