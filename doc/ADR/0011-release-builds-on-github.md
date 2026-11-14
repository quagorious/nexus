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
- Not done yet: Nexus could update the addon automatically from GitHub
  releases (`UpdateProvider::GitHub` with the repository URL in
  `nexus::export!`). The release assets this workflow produces are the
  prerequisite for that.
