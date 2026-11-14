# 0005. Put cards on the Windows clipboard as PNG + DIB

- Status: Accepted (verified 2026-11-15)
- Date: 2026-11-14

## Context

The game runs under Wine on KDE/Wayland. The card has to end up on the
Linux clipboard as an image that Discord accepts.

## Decision

- Write two formats in one clipboard transaction:
  - the registered format **`PNG`** with the PNG bytes (keeps the
    transparent rounded corners), and
  - **`CF_DIB`**: 32-bit bottom-up BGRA, with the corners blended onto the
    card background, since most DIB readers ignore alpha.
- Wine's clipboard bridge exports these as `image/png` and `image/bmp` to the
  Linux side.
- Text ("Copy text") goes as `CF_UNICODETEXT` with CRLF line endings.
- Retry `OpenClipboard` for up to ~200 ms, since another app may hold it.

## Consequences

- Native Windows apps and Wine-bridged Linux apps both get an image.
- Pasting into Discord on Linux works (verified in game on 2026-11-15).
  If it breaks after a Wine/Proton update, check
  `xclip -selection clipboard -t TARGETS -o` while a card is on the
  clipboard.
