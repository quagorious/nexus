# 0002. Authenticate with an account-only subtoken; never store the API key

- Status: Accepted
- Date: 2026-11-14

## Context

The module API's auth flow, as the site's author described it:

1. GW2 API key → `GET /v2/createsubtoken?permissions=account&expire=…`
2. subtoken → `POST quagorious.gg/api/blish/session` → session token
   (`lushm_…`, valid 24 h) plus `liveUrl` and account
3. session → `GET /api/blish/live-token` → live token
4. live token → SSE `GET {liveUrl}/api/blish/events?token=…`

Detail requests (`/api/blish/encounters[/<id>]`) use the **session** token,
not the live token.

## Decision

- The user pastes an API key in Nexus → Options → Quagorious. The addon
  turns it into a subtoken with only the `account` permission, valid for
  one year, and stores only that subtoken (`settings.json`). The key is
  never written to disk.
- The session is kept in memory and renewed from the subtoken when it's
  missing, expires within 5 minutes, or a request returns 401/403
  (`ensure_session`, `with_session`: one retry with a fresh session).
- The live token is fetched fresh on every (re)connect.
- Error bodies (Nuxt's `statusMessage`) are shown as they are, so messages
  like "Modul-Sitzung ungültig oder abgelaufen" reach the user.
- "Disconnect" removes the subtoken and stops the live feed.

## Consequences

- A leaked settings file exposes only account-level read access, for at
  most a year.
- After a year, the user has to connect again with a key.
- Nothing logs tokens. The raw live events are no longer logged either,
  because they contain the share token (see
  [0003](0003-live-feed-over-sse.md)).
