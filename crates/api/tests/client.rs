//! Client tests against a tiny in-process HTTP server.

use std::{
    io::{BufRead, BufReader, Read, Write},
    net::TcpListener,
    sync::{Arc, Mutex},
    thread,
};

use quagorious_api::{Client, Endpoints, NetError, Session};

#[derive(Debug, Clone)]
struct Req {
    method: String,
    /// Path including query.
    path: String,
    auth: Option<String>,
    body: String,
}

/// Answers each request via `route(method, path) -> (status, content-type, body)`
/// and records it. Always closes the connection after the response.
struct Mock {
    url: String,
    reqs: Arc<Mutex<Vec<Req>>>,
}

impl Mock {
    fn start(route: impl Fn(&str, &str) -> (u16, &'static str, String) + Send + 'static) -> Mock {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let reqs = Arc::new(Mutex::new(Vec::new()));
        let log = reqs.clone();
        thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { break };
                let mut r = BufReader::new(stream.try_clone().unwrap());
                let mut line = String::new();
                r.read_line(&mut line).unwrap();
                let mut it = line.split_whitespace();
                let (method, path) = (it.next().unwrap().to_string(), it.next().unwrap().to_string());
                let (mut len, mut auth) = (0usize, None);
                loop {
                    let mut h = String::new();
                    r.read_line(&mut h).unwrap();
                    let h = h.trim_end();
                    if h.is_empty() {
                        break;
                    }
                    let (k, v) = h.split_once(':').unwrap();
                    match k.to_ascii_lowercase().as_str() {
                        "content-length" => len = v.trim().parse().unwrap(),
                        "authorization" => auth = Some(v.trim().to_string()),
                        _ => {}
                    }
                }
                let mut body = vec![0; len];
                r.read_exact(&mut body).unwrap();
                let (status, ct, resp) = route(&method, &path);
                log.lock().unwrap().push(Req {
                    method,
                    path,
                    auth,
                    body: String::from_utf8(body).unwrap(),
                });
                let _ = write!(
                    stream,
                    "HTTP/1.1 {status} X\r\nContent-Type: {ct}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{resp}",
                    resp.len()
                );
            }
        });
        Mock { url, reqs }
    }

    fn paths(&self) -> Vec<String> {
        self.reqs.lock().unwrap().iter().map(|r| format!("{} {}", r.method, r.path)).collect()
    }
}

fn client(m: &Mock) -> Client {
    Client::new(Endpoints { site: m.url.clone(), gw2: m.url.clone() })
}

const SESSION: &str = r#"{"token":"S1","expiresAt":1795301822000,"liveUrl":"http://live.invalid","account":"Quaggan.1234","extra":1}"#;

fn session(live_url: &str) -> Session {
    Session { token: "S1".into(), expires_at: 0, live_url: live_url.into(), account: String::new() }
}

fn json(body: &str) -> (u16, &'static str, String) {
    (200, "application/json", body.into())
}

#[test]
fn open_session_new_path() {
    let m = Mock::start(|_, p| if p == "/api/auth/token" { json(SESSION) } else { (500, "text/plain", String::new()) });
    let s = client(&m).open_session("SUB").unwrap();
    assert_eq!((s.token.as_str(), s.expires_at, s.account.as_str()), ("S1", 1795301822000, "Quaggan.1234"));
    assert_eq!(s.live_url, "http://live.invalid");
    let reqs = m.reqs.lock().unwrap();
    assert_eq!(reqs.len(), 1);
    assert_eq!((reqs[0].method.as_str(), reqs[0].path.as_str()), ("POST", "/api/auth/token"));
    let body: serde_json::Value = serde_json::from_str(&reqs[0].body).unwrap();
    assert_eq!(body, serde_json::json!({"grant_type":"subtoken","client_id":"blish","subtoken":"SUB"}));
}

#[test]
fn open_session_accepts_access_token_alias() {
    let m = Mock::start(|_, _| json(r#"{"access_token":"A","expires_at":5,"live_url":"http://l"}"#));
    let s = client(&m).open_session("SUB").unwrap();
    assert_eq!((s.token.as_str(), s.expires_at, s.live_url.as_str()), ("A", 5, "http://l"));
}

#[test]
fn open_session_falls_back_on_404_and_remembers() {
    let m = Mock::start(|_, p| {
        if p == "/api/blish/session" { json(SESSION) } else { (404, "text/plain", "nope".into()) }
    });
    let c = client(&m);
    assert_eq!(c.open_session("SUB").unwrap().token, "S1");
    assert_eq!(c.open_session("SUB").unwrap().token, "S1");
    assert_eq!(
        m.paths(),
        ["POST /api/auth/token", "POST /api/blish/session", "POST /api/blish/session"]
    );
    let reqs = m.reqs.lock().unwrap();
    assert_eq!(reqs[1].body, r#"{"subtoken":"SUB"}"#);
}

#[test]
fn open_session_falls_back_on_405() {
    let m = Mock::start(|_, p| if p == "/api/blish/session" { json(SESSION) } else { (405, "text/plain", String::new()) });
    assert_eq!(client(&m).open_session("SUB").unwrap().token, "S1");
}

#[test]
fn open_session_no_fallback_on_401_or_500() {
    let m = Mock::start(|_, _| (401, "application/json", r#"{"statusMessage":"bad subtoken"}"#.into()));
    match client(&m).open_session("SUB") {
        Err(NetError::Unauthorized(msg)) => assert!(msg.contains("bad subtoken"), "{msg}"),
        r => panic!("{r:?}"),
    }
    assert_eq!(m.paths(), ["POST /api/auth/token"]);

    let m = Mock::start(|_, _| (500, "text/plain", "boom".into()));
    assert!(matches!(client(&m).open_session("SUB"), Err(NetError::Other(_))));
    assert_eq!(m.paths(), ["POST /api/auth/token"]);
}

#[test]
fn live_token_new_path() {
    let m = Mock::start(|_, p| if p == "/api/auth/live-token" { json(r#"{"token":"LT"}"#) } else { (500, "text/plain", String::new()) });
    assert_eq!(client(&m).live_token(&session("")).unwrap(), "LT");
    let reqs = m.reqs.lock().unwrap();
    assert_eq!(reqs[0].auth.as_deref(), Some("Bearer S1"));
}

#[test]
fn live_token_falls_back_on_404() {
    let m = Mock::start(|_, p| if p == "/api/blish/live-token" { json(r#"{"token":"LT"}"#) } else { (404, "text/plain", String::new()) });
    let c = client(&m);
    assert_eq!(c.live_token(&session("")).unwrap(), "LT");
    assert_eq!(c.live_token(&session("")).unwrap(), "LT");
    assert_eq!(
        m.paths(),
        ["GET /api/auth/live-token", "GET /api/blish/live-token", "GET /api/blish/live-token"]
    );
    assert_eq!(m.reqs.lock().unwrap()[1].auth.as_deref(), Some("Bearer S1"));
}

#[test]
fn live_token_no_fallback_on_403() {
    let m = Mock::start(|_, _| (403, "text/plain", "no".into()));
    assert!(matches!(client(&m).live_token(&session("")), Err(NetError::Unauthorized(_))));
    assert_eq!(m.paths().len(), 1);
}

const STREAM: &str = ": hello\n\nevent: ping\n\ndata: {\"type\":\"try.created\",\ndata: \"id\":1}\n\ndata: {\"id\":1}\ndata: second\n\n";

fn collect(c: &Client, m: &Mock) -> (Vec<(String, String)>, bool) {
    let mut events = Vec::new();
    let mut opened = false;
    c.listen(&session(&m.url), "LT", || opened = true, |e| {
        events.push((e.event, e.data));
        true
    })
    .unwrap();
    (events, opened)
}

#[test]
fn listen_new_path_parses_sse() {
    let m = Mock::start(|_, p| {
        if p == "/events?token=LT" { (200, "text/event-stream", STREAM.into()) } else { (500, "text/plain", p.into()) }
    });
    let (events, opened) = collect(&client(&m), &m);
    assert!(opened);
    assert_eq!(
        events,
        [
            ("ping".to_string(), String::new()),
            (String::new(), "{\"type\":\"try.created\",\n\"id\":1}".to_string()),
            (String::new(), "{\"id\":1}\nsecond".to_string()),
        ]
    );
    assert_eq!(m.paths(), ["GET /events?token=LT"]);
}

#[test]
fn listen_falls_back_to_legacy_events() {
    let m = Mock::start(|_, p| {
        if p == "/api/blish/events?token=LT" {
            (200, "text/event-stream", "data: {\"type\":\"encounter\",\"id\":3}\n\n".into())
        } else {
            (404, "text/plain", String::new())
        }
    });
    let c = client(&m);
    let (events, opened) = collect(&c, &m);
    assert!(opened);
    assert_eq!(events, [(String::new(), "{\"type\":\"encounter\",\"id\":3}".to_string())]);
    collect(&c, &m);
    assert_eq!(
        m.paths(),
        ["GET /events?token=LT", "GET /api/blish/events?token=LT", "GET /api/blish/events?token=LT"]
    );
}

#[test]
fn listen_stops_when_callback_returns_false() {
    let m = Mock::start(|_, _| (200, "text/event-stream", STREAM.into()));
    let mut n = 0;
    client(&m)
        .listen(&session(&m.url), "LT", || {}, |_| {
            n += 1;
            false
        })
        .unwrap();
    assert_eq!(n, 1);
}

// --- Discord webhooks -------------------------------------------------------

use quagorious_api::discord::{Attachment, DiscordError, NewMessage, Webhook};

#[test]
fn discord_post_sends_multipart_with_payload_and_file() {
    let m = Mock::start(|_, _| json(r#"{"id":"m1","channel_id":"t1","content":"x"}"#));
    let hook = Webhook::with_base(&m.url, "42", "tok");
    let msg = client(&m)
        .discord_post(
            &hook,
            &NewMessage {
                content: "**Kill** · 7:42",
                thread_id: Some("t1"),
                file: Some(Attachment { filename: "card.png", content_type: "image/png", bytes: b"PNGDATA" }),
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!((msg.id.as_str(), msg.channel_id.as_str()), ("m1", "t1"));
    let reqs = m.reqs.lock().unwrap();
    assert_eq!(reqs[0].method, "POST");
    assert_eq!(reqs[0].path, "/api/webhooks/42/tok?wait=true&thread_id=t1");
    let body = &reqs[0].body;
    assert!(body.contains("name=\"payload_json\""), "{body}");
    assert!(body.contains(r#""attachments":[{"filename":"card.png","id":0}]"#), "{body}");
    assert!(body.contains(r#""allowed_mentions":{"parse":[]}"#), "{body}");
    assert!(body.contains("name=\"files[0]\"; filename=\"card.png\"\r\nContent-Type: image/png\r\n\r\nPNGDATA\r\n"), "{body}");
}

#[test]
fn discord_creates_forum_post_with_json_and_edits() {
    let m = Mock::start(|_, _| json(r#"{"id":"m1","channel_id":"t1"}"#));
    let hook = Webhook::with_base(&m.url, "42", "tok");
    let c = client(&m);
    c.discord_post(&hook, &NewMessage { content: "summary", thread_name: Some("1125_Wed"), ..Default::default() })
        .unwrap();
    c.discord_edit(&hook, Some("t1"), "m1", "new summary").unwrap();
    let reqs = m.reqs.lock().unwrap();
    let first: serde_json::Value = serde_json::from_str(&reqs[0].body).unwrap();
    assert_eq!(first["thread_name"], "1125_Wed");
    assert_eq!(first["content"], "summary");
    assert_eq!(reqs[1].method, "PATCH");
    assert_eq!(reqs[1].path, "/api/webhooks/42/tok/messages/m1?thread_id=t1");
    let edit: serde_json::Value = serde_json::from_str(&reqs[1].body).unwrap();
    assert_eq!(edit["content"], "new summary");
}

#[test]
fn discord_retries_once_after_rate_limit_and_maps_404() {
    let hits = Arc::new(Mutex::new(0));
    let h = hits.clone();
    let m = Mock::start(move |_, _| {
        let mut n = h.lock().unwrap();
        *n += 1;
        match *n {
            1 => (429, "application/json", r#"{"message":"slow down","retry_after":0.05,"global":false}"#.into()),
            2 => json(r#"{"id":"m1","channel_id":"t1"}"#),
            _ => (404, "application/json", r#"{"message":"Unknown Channel","code":10003}"#.into()),
        }
    });
    let hook = Webhook::with_base(&m.url, "42", "tok");
    let c = client(&m);
    let msg = NewMessage { content: "x", ..Default::default() };
    assert_eq!(c.discord_post(&hook, &msg).unwrap().id, "m1");
    assert_eq!(*hits.lock().unwrap(), 2);
    match c.discord_post(&hook, &msg) {
        Err(DiscordError::NotFound(e)) => assert!(e.contains("Unknown Channel"), "{e}"),
        other => panic!("{other:?}"),
    }
}

#[test]
fn discord_info_reads_guild_and_channel() {
    let m = Mock::start(|_, _| json(r#"{"id":"42","type":1,"guild_id":"7","channel_id":"9","name":"x"}"#));
    let info = client(&m).discord_info(&Webhook::with_base(&m.url, "42", "tok")).unwrap();
    assert_eq!((info.guild_id.as_deref(), info.channel_id.as_deref()), (Some("7"), Some("9")));
    assert_eq!(m.paths(), ["GET /api/webhooks/42/tok"]);
}

#[test]
fn discord_edit_keeps_listed_attachments_and_adds_the_new_one() {
    use quagorious_api::discord::Edit;
    let m = Mock::start(|_, _| {
        json(r#"{"id":"m1","channel_id":"t1","attachments":[{"id":"5","filename":"thumb-1.png"},{"id":"6","filename":"thumb-2.png"}]}"#)
    });
    let hook = Webhook::with_base(&m.url, "42", "tok");
    let msg = client(&m)
        .discord_edit_message(
            &hook,
            Some("t1"),
            "m1",
            &Edit {
                content: "summary",
                embeds: Some(serde_json::json!([{ "thumbnail": { "url": "attachment://thumb-2.png" } }])),
                keep: Some(vec!["5".into()]),
                file: Some(Attachment { filename: "thumb-2.png", content_type: "image/png", bytes: b"PNG" }),
            },
        )
        .unwrap();
    assert_eq!(msg.attachments.len(), 2);
    let reqs = m.reqs.lock().unwrap();
    assert_eq!((reqs[0].method.as_str(), reqs[0].path.as_str()), ("PATCH", "/api/webhooks/42/tok/messages/m1?thread_id=t1"));
    let body = &reqs[0].body;
    assert!(body.contains(r#""attachments":[{"id":"5"},{"filename":"thumb-2.png","id":0}]"#), "{body}");
    assert!(body.contains(r#""embeds":[{"thumbnail":{"url":"attachment://thumb-2.png"}}]"#), "{body}");
    assert!(body.contains("filename=\"thumb-2.png\"\r\nContent-Type: image/png\r\n\r\nPNG\r\n"), "{body}");
}
