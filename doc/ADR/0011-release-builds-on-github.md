# 0011. Build release DLLs on GitHub when a release is published

- Status: Accepted (not yet run on GitHub)
- Date: 2026-11-15

## Context

Until now, DLLs were built locally with `cargo-xwin` and copied into the
game folder by hand. To share the addon, builds should come from the
repository, be reproducible, and be verifiable. They should only run for
actual releases, not on every push.

## Decision

`.github/workflows/release.yml`:

- **Trigger:** `release: published` only. Pushes, pull requests and plain
  tags don't build.
- **Runner:** `windows-latest` with the stable toolchain and the
  `x86_64-pc-windows-msvc` target, so it builds natively there (no
  `cargo-xwin`). `--locked` uses the committed `Cargo.lock`.
- **Version guard:** the release tag must be `v` + the `Cargo.toml`
  version. Nexus shows the version compiled into the DLL, so a mismatch
  fails before compiling.
- **Artifacts:** `quagorious_share.dll` and `checksums.txt` (SHA-256 in
  `sha256sum` format) are uploaded to the release with `gh release upload
  --clobber`, so re-running a job replaces them.
- **Permissions:** `contents: write` only, using the built-in `GITHUB_TOKEN`.

## Consequences

- A release is: bump the version, commit and push, then `gh release create
  v<version> --generate-notes`.
- Users can check a download with `sha256sum -c checksums.txt`.
- Nexus updates the addon from these releases automatically (see below).

## Auto-updates through Nexus

The addon declares `provider: UpdateProvider::GitHub` and
`update_link: "https://github.com/quagorious/nexus"` in `nexus::export!`.
How Nexus uses that (read from its source, `CAddon::CheckUpdateViaGitHub`):

- It calls `GET api.github.com/repos/quagorious/nexus/releases`
  unauthenticated, so the repository has to stay public.
- For each release it parses `tag_name` with `v?\d+\.\d+(\.\d+){0,2}`, so
  `v0.2.0` works but `v0.2.0-beta` is skipped. It skips pre-releases unless
  they're allowed for the addon. It takes the highest version that has an
  asset ending in `.dll` (`checksums.txt` is ignored).
- It updates when that version is newer than the one in the DLL. nexus-rs
  compiles the `Cargo.toml` version in, with revision `-1` for stable
  versions, which Nexus reads as 65535. So a release with the **same**
  version (revision 0) counts as older: no re-download loop. Only a higher
  version triggers an update.
- A release whose workflow is still running has no `.dll` yet and is simply
  skipped until the upload finishes.

Installations from before this change (no update link) need one manual
update to a build that has it.
