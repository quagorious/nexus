# 0001. A Rust Nexus addon, cross-compiled from Linux

- Status: Accepted
- Date: 2026-11-14

## Context

A friend runs quagorious.gg, a log analyser for Guild Wars 2, and is
building a Blish HUD module that shows share cards for each try. We wanted
the same in game, but for a setup that runs GW2 under Wine/Proton on Linux
with [Nexus](https://github.com/RaidcoreGG/Nexus) as the addon loader.
There was already a Rust Nexus addon here (betterTP), which set the
toolchain and conventions.

## Decision

- Write a Nexus addon in Rust with Zerthox's
  [nexus-rs](https://github.com/Zerthox/nexus-rs) bindings (imgui UI,
  keybinds, textures, alerts, paths).
- Build a Windows DLL from Linux with `cargo-xwin` for
  `x86_64-pc-windows-msvc`. GW2 and Nexus are Windows binaries even under
  Proton.
- Use a release profile with LTO, `panic = "abort"` (a panic must not unwind
  through the game), and `strip`.
- Keep everything that doesn't need Windows in a separate crate
  (`crates/card`, see [0004](0004-software-rendered-png-cards.md)), so it
  can be built and tested on the Linux host.

## Consequences

- One command builds the addon:
  `cargo xwin build --release --target x86_64-pc-windows-msvc`.
- Win32 calls (clipboard, ShellExecute) go through `windows-sys`, behind
  `cfg(windows)`, with stubs for other targets.
- The addon can't be run outside the game. UI changes are checked with
  screenshots taken in game (KDE `spectacle`); card changes are checked
  with the host preview.
- Nexus unloads the DLL while background threads may still be running, so
  `unload` stops them and waits for them (see
  [0003](0003-live-feed-over-sse.md)).
