# 0012. The squad view: a card and an imgui window

- Status: Accepted (card, window and text size verified in game on 2026-11-15)
- Date: 2026-11-15

## Context

The cards are about you. A commander, or anyone reviewing a wipe, wants the
whole squad on one screen: result, phase timeline with downs and deaths,
phase times across today's tries, every player's DPS, downs, deaths, fails
and boon uptimes, boons per group, recurring mistakes, and the squad's
mechanics. The reference design was a panel with all of this.

Everything except one part is in the full analysis
(`/api/encounters/<id>?share=<token>`, see
[0006](0006-full-analysis-via-share-token.md)):

- `players[]`: `subgroup`, `role`, `targetDps`, `downs`, `deaths`, `fails`,
  `commander` (0/1), `uptimeActive`, `firstDownMs`, `firstDeathMs`.
- `mechanics[]`: one entry per player and mechanic, with `kind` `fail`,
  `task` or `ok`. The `ok` entries `Downed` and `Dead` carry the time of
  every down and death, not only the first.
- `encounter.phases`, `lostAtMs`, `squadDps`, `squadSize`.

The reference design's map of where people went down needs positions. The
analysis has none.

## Decision

- **Both a card and a window.** The first version was only an imgui window.
  To paste it into Discord, it's now also the **Squad** card variant (after
  Overview, Fight timeline and the boon card), rendered by tiny-skia like
  the others but 900 units wide in two columns (`squad_card.rs`). The
  window ("Squad window" in the main window) stays for hovering (phase and
  marker tooltips, account names) and always shows the try that's selected
  in the main window.
- **The squad view is built before the cards**, so the Squad card exists
  when a new try is announced and auto-copied.
- **Header**: boss artwork at the right (faded in), date and start time in
  the subtitle, purple for a kill and red for a wipe, as on the cards.
- **Text size**: "Window text size" (80–200 %) in the options scales the
  font of both windows (`set_window_font_scale`); the squad window scales
  its fixed sizes along with it. Cards are images and aren't affected.
- **Numbers in the card crate, drawing in the addon.**
  `quagorious_card::squad::build()` turns the analyses into a `SquadView`
  of ready-made strings, numbers and colours, with German and English
  labels. It's tested on the host (`cargo test`), and
  `cargo run -p quagorious_card --example squad` prints it for real
  analyses. `src/squad_ui.rs` only draws.
- **Today's tries at the same boss.** The worker loads the full analysis of
  each of today's tries (same boss, mode and local day, from the try list)
  whose share token it knows. They're cached for the session. Only the
  current try is retried on failure.
- **Instead of the map**: a list of who went down when, per player, with the
  times of every down and death. A down that turns into a death within 3 s
  is shown once, as the death.
- **Phases**: only top-level phases (a phase inside another one, like
  "Void Time Caster" in "Purification 2", is dropped). The "phases today"
  table takes its columns from the try that got furthest. The fastest
  completed time per column is green. The phase the try was lost in
  (`lostAtMs`, or the end of a wipe) is marked with `*` and never counts
  as fastest.
- **Boons** use `uptimeActive` (time alive and in the fight), as in the
  reference design. The boon card keeps `uptimeWeighted`.
- **Recurring mistakes** are fail mechanics that hit in every analysed try
  today up to the current one, with the hits per try. They only appear
  from the second try on.
- **Mechanic names**: the analysis only has English descriptions. The
  panel uses the localized text from the module API where it knows the
  mechanic (your own mechanics in tries you've looked at), and the
  English description otherwise.
- All texts stay within Latin-1 (the in-game font).

## Consequences

- Like the extra cards, the panel only exists for tries the addon saw live.
- The first time you open an evening's try, the worker may fetch up to ~20
  analyses (about 100–150 KB each) before the card appears. After that
  they're cached, so a new try only fetches its own analysis.
- In imgui, `same_line_with_pos` counts from the table column's start
  inside a table. The window positions right-aligned text in screen
  coordinates instead (`goto_x`).
- In German, most mechanic names in the squad lists are still English.
- Only the parsed fields are kept in memory, not the whole response.
