//! Quagorious HTTP + live-events client.
//!
//! Auth flow (see README):
//! 1. GW2 API key → `createsubtoken?permissions=account` (done once; only the
//!    subtoken is stored, never the key)
//! 2. subtoken → `POST /api/auth/token` (`grant_type: subtoken`, `client_id:
//!    blish`) → 24h session token
//! 3. session → `GET /api/auth/live-token` → short-lived live token
//! 4. live token → SSE `GET {liveUrl}/events?token=..`
//! 5. session → `GET /api/blish/encounters[/<id>]`
//!
//! Steps 2–4 fall back to the legacy `/api/blish/{session,live-token,events}`
//! paths when the new one answers 404/405; the working variant is remembered
//! per [`Client`].

mod client;
mod live;

pub use client::{Client, Endpoints, NetError, NetResult, Session, SseEvent};
pub use live::{LiveEvent, parse_live_event};
