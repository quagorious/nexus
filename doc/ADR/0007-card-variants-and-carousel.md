# 0007. Three card variants in a carousel; copy the focused one

- Status: Accepted
- Date: 2026-11-14

## Context

The reference module has three card designs: a kill/overview card, a wipe
analysis card and a boon-support card. Which ones make sense depends on the
try and on your role.

## Decision

`quagorious_card::variants()` decides which cards a try gets, in this order:

| Variant | Content | When |
|---|---|---|
| **Overview** | Result, your DPS against your earlier tries with the same spec/role, squad DPS (with full data), boon summary (without full data), mistakes/contributions, today's tries at this boss | always |
| **Fight timeline** | Phases (sub-phases nested in a bigger one are dropped), "Try verloren ab m:ss", who started the loss, deaths after / deaths and downs before, squad DPS with rank, your own downs before the loss | full data and (phases, or a wipe with `lostAtMs`) |
| **Your boon** | See [0008](0008-boon-card-uses-subgroup.md) | full data and you provided a boon |

The window shows ◀ tab tab tab ▶. The **focused** variant is what "Copy
image", the copy keybind and auto-copy put on the clipboard. The focus is
stored as a kind ("overview", "fight", "boon:alacrity"), not as an index, so
the next try opens on the same kind if it has one. That way support players
who stay on the boon card keep getting boon cards.

Wipe semantics from the analysis: `deaths`/`downs` count until `lostAtMs`
(the point after which half the squad never stood again), and
`lostDeaths`/`lostDowns` count after it. `lostBy` is the account whose death
started the loss.

## Consequences

- All variants are rendered when a try is shown (fast enough: tens of ms
  each). Only the focused one gets a preview texture.
- Squad-DPS rank is counted among pure `DPS` roles if you're a DPS, else
  among everyone.
- Phases alternate grey/purple. The data doesn't mark burn phases.
