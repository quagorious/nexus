# 0011. Build release DLLs on GitHub when a version tag is pushed

- Status: Accepted (amended after v0.2.0, see "Why the tag triggers the build")
- Date: 2026-11-15

## Context

Until now, DLLs were built locally with `cargo-xwin` and copied into the
game folder by hand. To share the addon, builds should come from the
repository, be reproducible, and be verifiable. They should only run for
actual releases, not on every push.

## Decision

`.github/workflows/release.yml`:

- **Trigger:** a pushed `v*` tag only. Branch pushes and pull requests
  don't build.
- **Runner:** `windows-latest` with the stable toolchain and the
  `x86_64-pc-windows-msvc` target, so it builds natively there (no
  `cargo-xwin`). `--locked` uses the committed `Cargo.lock`.
- **Version guard:** the tag must be `v` + the `Cargo.toml`
  version. Nexus shows the version compiled into the DLL, so a mismatch
  fails before compiling.
- **Artifacts:** after the build, the job creates the release for the tag
  with `quagorious_share.dll` and `checksums.txt` (SHA-256 in `sha256sum`
  format) already attached. Re-running a job for an existing release
  replaces them (`gh release upload --clobber`).
- **Permissions:** `contents: write` only, using the built-in `GITHUB_TOKEN`.

## Consequences

- A release is: bump the version, commit and push, then tag the commit
  `v<version>` and push the tag. Release notes are generated and can be
  edited on GitHub afterwards.
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
- A release without a `.dll` asset is skipped. Nexus caches the release
  list on disk (`addons/common/api.github.com/…/releases.json`), so that
  answer sticks until the cache expires.

## Why the tag triggers the build

Originally the workflow ran on `release: published` and uploaded the DLL to
the existing release. For v0.2.0 that left the release without assets for
about three minutes; a Nexus client checked 18 seconds before the upload,
cached the asset-less release and kept reporting "up to date". Building
first and creating the release last closes that window.

Installations from before this change (no update link) need one manual
update to a build that has it.
