# 0009. Notify and auto-copy instead of opening the window

- Status: Accepted
- Date: 2026-11-14

## Context

At first the window opened on every new try. After every boss that gets in
the way.

## Decision

Three independent options for a new try (Nexus → Options → Quagorious):

- **Copy new cards to the clipboard automatically**: copies the focused
  variant ([0007](0007-card-variants-and-carousel.md)).
- **Show a notification**: a Nexus alert (`send_alert`), e.g.
  `Quagorious: KILL Faultierion 3:12 · Karte "Übersicht" kopiert`, or
  "Kopieren fehlgeschlagen", or, without auto-copy, a hint to use the copy
  keybind.
- **Open the window**.

Defaults for new installs: open window on, auto-copy off, notify on. The
author's setup uses auto-copy + notify, without opening the window.

The texts don't name specific keys. Keybinds can be rebound in Nexus, so the
UI refers to `QUAGORIOUS_TOGGLE` / `QUAGORIOUS_COPY`. All in-game text stays
within Latin-1, because the game font has no other characters.

## Consequences

- After a boss, the card is ready to paste, with a short confirmation and
  nothing to close.
- The notification works in game (verified 2026-11-15).
