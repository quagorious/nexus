# 0003. Follow the live feed over SSE, react to `encounter` events

- Status: Accepted
- Date: 2026-11-14

## Context

The live endpoint is a server-sent-events stream. The event format wasn't
documented, so we recorded it during a raid evening (`tools/capture.sh`):

- `event: ping` with empty data, every 25 seconds.
- Per upload, an unnamed event with an `id:` counter and one JSON line:

  ```json
  {"type":"encounter","id":24499,"seq":4,"token":"…","bossId":132097,
   "bossName":"Cairn","cm":0,"success":true,"bossHpLeft":0,
   "durationMs":122064,"timeStart":…,"accounts":[…10 squad accounts…]}
  ```

- The try's details were ready about one second after the event.

The first version refreshed the try list on every event, and logged every
event including pings (144 lines an hour, with the share token in them).

## Decision

- Parse each event. `type == "encounter"` → `Cmd::LiveEncounter { id,
  token, bossId }`, which shows that try right away. Pings are ignored
  silently. Unrecognised JSON falls back to a list refresh, so nothing is
  missed if the format changes.
- Log only `live: new encounter <id>`, never the payload.
- Read timeout 75 s (three missed pings). On timeout, error or end of
  stream, reconnect with backoff (2 s doubling to 60 s, reset after a
  connection that lasted a minute). After each reconnect, reload the try
  list to catch up on what was missed.
- A custom TLS connector keeps a clone of the SSE socket. `unload` shuts it
  down, so the blocked reader returns immediately. `unload` then waits for
  both threads (up to 25 s), so no code runs after the DLL is gone.
- `bossHpLeft` is a **percentage** (0–100). The samples confirmed it
  (0.74 % and 10.54 % on wipes).

## Consequences

- New tries reach the addon within about a second of the upload.
- The try list (`/api/blish/encounters`, max 20 entries) is still used for
  the "today" chips and the try picker.
- If the server changes the event shape, the fallback still triggers a list
  refresh. The worker then detects new tries by id.
