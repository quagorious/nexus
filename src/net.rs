//! Process-wide [`quagorious_api::Client`] with free-function shortcuts.

use std::sync::LazyLock;

use quagorious_api::Client;
pub use quagorious_api::{LiveEvent, NetError, NetResult, Session, SseEvent, parse_live_event};
use quagorious_card::{Encounter, FullEncounter, TryList};

static CLIENT: LazyLock<Client> = LazyLock::new(Client::default);

pub fn shutdown_live_socket() {
    CLIENT.shutdown_live_socket();
}

pub fn create_subtoken(api_key: &str) -> NetResult<String> {
    CLIENT.create_subtoken(api_key)
}

pub fn open_session(subtoken: &str) -> NetResult<Session> {
    CLIENT.open_session(subtoken)
}

pub fn live_token(session: &Session) -> NetResult<String> {
    CLIENT.live_token(session)
}

pub fn list_tries(session: &Session, limit: u32) -> NetResult<TryList> {
    CLIENT.list_tries(session, limit)
}

pub fn encounter(session: &Session, id: u64, locale: &str) -> NetResult<Encounter> {
    CLIENT.encounter(session, id, locale)
}

pub fn listen(
    session: &Session,
    live_token: &str,
    on_open: impl FnOnce(),
    on_event: impl FnMut(SseEvent) -> bool,
) -> NetResult<()> {
    CLIENT.listen(session, live_token, on_open, on_event)
}

pub fn full_encounter(id: u64, share_token: &str) -> NetResult<FullEncounter> {
    CLIENT.full_encounter(id, share_token)
}

pub fn gw2_all<T: serde::de::DeserializeOwned>(path: &str) -> NetResult<Vec<T>> {
    CLIENT.gw2_all(path)
}

pub fn fetch_bytes(url: &str) -> NetResult<Vec<u8>> {
    CLIENT.fetch_bytes(url)
}

pub fn client() -> &'static Client {
    &CLIENT
}
