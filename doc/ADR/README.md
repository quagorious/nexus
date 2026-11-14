# Architecture & decisions

This folder explains how the addon works and why it's built the way it is.
Each numbered file is an Architecture Decision Record (ADR): the situation,
what we decided, and what follows from it. All of them were written on
2026-11-14, the day the addon was built.

## Records

| # | Decision | Status |
|---|---|---|
| [0001](0001-rust-nexus-addon-cross-compiled.md) | A Rust Nexus addon, cross-compiled from Linux | Accepted |
| [0002](0002-auth-store-only-a-subtoken.md) | Authenticate with an account-only subtoken; never store the API key | Accepted |
| [0003](0003-live-feed-over-sse.md) | Follow the live feed over SSE, react to `encounter` events | Accepted |
| [0004](0004-software-rendered-png-cards.md) | Render cards in software to PNG, in a separate crate | Accepted |
| [0005](0005-clipboard-png-and-dib-via-wine.md) | Put cards on the Windows clipboard as PNG + DIB | Accepted |
| [0006](0006-full-analysis-via-share-token.md) | Get the full analysis through the try's share token | Accepted, fragile |
| [0007](0007-card-variants-and-carousel.md) | Three card variants in a carousel; copy the focused one | Accepted |
| [0008](0008-boon-card-uses-subgroup.md) | The boon card lists your subgroup | Accepted (supersedes an earlier approach) |
| [0009](0009-notify-instead-of-opening-window.md) | Notify and auto-copy instead of opening the window | Accepted |
| [0010](0010-spec-icons-and-boss-images.md) | Spec icons from the GW2 API, boss artwork from the site's table | Accepted |
| [0011](0011-release-builds-on-github.md) | Build release DLLs on GitHub when a release is published | Accepted |

## How the code works

```
                  ┌──────────────── GW2 process (Wine/Proton) ────────────────────┐
                  │  Nexus ── loads ──▶ quagorious_share.dll                     │
                  │                                                               │
 quagorious.gg    │   live thread ──SSE──▶ events ──Cmd──▶ worker thread        │
 live.quagorious  │        ▲                                  │  HTTP (ureq)      │
 api.guildwars2   │        └──── session (shared) ◀───────────┤  render cards    │
                  │                                           ▼                   │
                  │   render thread (imgui) ◀── SHARED state ── CardView         │
                  │        │ copy                                                 │
                  │        ▼                                                      │
                  │   Win32 clipboard (PNG + CF_DIB) ── Wine ──▶ Linux clipboard │
                  └───────────────────────────────────────────────────────────────┘
```

Three threads, connected by one command channel and a few mutex-guarded
globals (`src/lib.rs`):

- **Render thread** (owned by Nexus): draws the window and the options page
  every frame, reads `SHARED`, and sends `Cmd`s (show a try, connect,
  change language). Copying to the clipboard also happens here, on click or
  keybind.
- **Worker thread**: owns all request/response HTTP and the card
  rendering. It handles `Cmd::Connect`, `Refresh`, `Show`, `LiveEncounter`,
  `Relocale` and `Disconnect`, and coalesces bursts of refreshes into one.
- **Live thread**: keeps the SSE connection open, turns `encounter` events
  into `Cmd::LiveEncounter`, and reconnects with backoff.

The session token is shared by the worker and the live thread and is
refreshed by whichever needs it first (`ensure_session`).

### Modules

| Path | Role |
|---|---|
| `src/lib.rs` | Nexus registration, settings, global state, worker and live threads, imgui window and options page, notifications |
| `src/net.rs` | All HTTP: GW2 subtoken, module session, live token, try list and details, SSE reader, full analysis, GW2 specs, raw downloads. Holds the SSE socket so `unload` can shut it down |
| `src/cards.rs` | Builds a `CardView` (all variants at 2× for the clipboard and 1× for the preview). Persists share tokens; caches spec icons and boss images on disk |
| `src/clipboard.rs` | Win32 clipboard: registered `PNG` + `CF_DIB` for images, `CF_UNICODETEXT` for text |
| `src/shell.rs` | Opens the web link (`ShellExecuteW` → `xdg-open` under Wine) |
| `crates/card/` | Platform-independent: API data model, German/English labels, the tiny-skia/ab_glyph renderer, `variants()`, `party()`, `today_chips()`. Testable on Linux with `cargo run -p quagorious_card --example render` |
| `assets/boss_images.json` | Boss id → image URL (the same table quagorious.gg uses) |
| `tools/probe.sh`, `tools/capture.sh` | Dump API samples / record the live feed (both read `./.gw2key`) |

### What happens when you finish a boss

1. Your log gets uploaded and analysed. Quagorious pushes a
   `{"type":"encounter","id":…,"token":…,"bossId":…}` event.
2. The live thread sends `Cmd::LiveEncounter`. The worker saves the share
   token to `tokens.json` and reloads the try list (for the "today" chips).
3. The worker fetches the try's details from the module API (in the card
   language). It fetches the full analysis with the share token (retrying
   once), makes sure the spec icons and the boss image are available, and
   renders every variant.
4. Depending on the options, it copies the focused variant to the
   clipboard, shows a Nexus notification, and/or opens the window.

### Files the addon writes (`addons/Quagorious/`)

| File | Content |
|---|---|
| `settings.json` | Subtoken + expiry, account, language, auto-open / auto-copy / notify |
| `tokens.json` | Try id → share token, newest 2000 |
| `gw2_specs.json` | Raw GW2 specializations + professions, refreshed after 14 days |
| `icons/<Spec>.png` | Spec icons |
| `bosses/<bossId>.png` | Boss artwork |

## Where we are now (2026-11-15)

**Working and verified in game:** connecting with an API key; the live
feed; new tries arriving about a second after the upload; the window with
try picker and card carousel; copying the focused card; the alacrity card
on a support wipe; the browser link; the fixed status line; pasting the
copied card into Discord on Linux (Wine's clipboard bridge); the Nexus
notification on a new try.

**Verified only on rendered samples (host preview):** all three variants on
six real tries (kills, wipes, CM, alac-heal, alac-DPS), English labels,
the subgroup-based boon card.

**Not verified yet:**
- A quickness card (no quick-support sample yet).
- The release workflow on GitHub (see [0011](0011-release-builds-on-github.md)).

**Known limitations:**
- The extra cards and the boss image only exist for tries whose share token
  the addon knows: tries it saw live, from the first install on. See
  [0006](0006-full-analysis-via-share-token.md).
- The full analysis comes from the website's own API, which isn't part of
  the module contract and may change.
- `GET /api/blish/encounters` returns at most 20 tries, so the "today" chips
  only cover the last 20 tries.
- Phases alternate grey/purple; the data doesn't say which ones are burn
  phases.
- Two gap-cause labels are guesses: `sourceIdle` ("Quelle zu selten") and
  `pulseOff` ("Puls aus").
- Nexus can't free textures. Each previewed card variant keeps about 1.6 MB
  of graphics memory until the game restarts.

**Ideas for the module API** (would remove the share-token dependency):
include `bossId` and the share `token` in `/api/blish/encounters[/<id>]`, or
serve the squad/phase/boon data there directly.
