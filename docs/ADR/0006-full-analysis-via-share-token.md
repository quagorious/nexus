# 0006. Get the full analysis through the try's share token

- Status: Accepted, fragile
- Date: 2026-11-14

## Context

The two extra card designs from the Blish HUD module need data the module
API (`/api/blish/encounters/<id>`) doesn't have: phases, where the try was
lost, every player's DPS, subgroups and per-player boon uptime and gaps.

Reading the website's JavaScript showed that the encounter page works
without a login when given a share token. `GET
/api/encounters/<id>?share=<token>` returns the full analysis (about 70 KB).
The share token is the same one used in the web link
(`/app/encounters/<token>`), and the live `encounter` event carries it.
`/api/encounters/<id>` without a token needs a website login. The module
session isn't accepted there. The response's previous/next links are empty
in shared mode, so there's no way to walk to other tries.

## Decision

- Store every share token from the live feed in `tokens.json` (id → token,
  newest 2000) so it survives restarts.
- For a try with a known token, fetch the full analysis (retry once after
  2 s, since a fresh upload may still be processing) and cache it in
  memory. It doesn't depend on the language.
- Without a token, show only the overview card (from the module API).
- The token also gives the "Open in browser" button, and the link at the end
  of "Copy text".

## Consequences

- Tries from before the addon was installed, or that weren't seen live,
  have only the overview card and no boss image.
- This depends on an API that isn't part of the module contract. If it
  changes, the extra cards disappear, but the overview keeps working.
- The clean fix belongs in the module API: include `bossId` and the share
  token in `/api/blish/encounters[/<id>]`, or serve the squad, phase and
  boon data there directly.
