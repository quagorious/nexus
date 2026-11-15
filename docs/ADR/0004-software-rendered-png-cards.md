# 0004. Render cards in software to PNG, in a separate crate

- Status: Accepted
- Date: 2026-11-14

## Context

The goal was a shareable image like the cards in the Blish HUD module, to
paste into Discord. imgui can draw it in game, but can't produce an image
to put on the clipboard. It also has to look identical in the preview and
in the copy.

## Decision

- Draw cards with **tiny-skia** (shapes, gradients) and **ab_glyph**
  (text), into a pixmap that is encoded as PNG. Fonts are Noto Sans
  Regular/Bold, embedded (SIL OFL, license in `crates/card/assets/`).
- Lay out in "units" (420 wide) and scale at draw time. Each card is
  rendered at **2×** (840 px, for the clipboard) and **1×** (for the in-game
  preview texture). Layout runs twice: once to measure the height, once to
  draw.
- Keep the renderer, data model and labels (German/English, locale number
  formats) in `crates/card`, with no Windows or Nexus dependency.
  `examples/render.rs` renders any sample JSON to PNGs on the host, which is
  how every card change was checked.
- The in-game preview uses a Nexus texture created from the 1× PNG, once
  per variant that is viewed.

## Consequences

- What you see in the window is exactly what gets copied.
- Nexus has no API to free textures. Every previewed variant stays in GPU
  memory (about 1.6 MB) until the game restarts. That's acceptable for a
  raid evening; only viewed variants get textures.
- The card font has more characters than the game's imgui font. In-game UI
  text has to stay within Latin-1 (a "●" showed as "?"), while the card
  can use "Ø", "–", "…".
