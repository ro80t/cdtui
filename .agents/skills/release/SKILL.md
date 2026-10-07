---
name: release
description: Release a new version of the cdtui workspace (cdtui, cdt-tree, cdt-search, cdt-view) to crates.io via the CI trusted-publishing flow. Use when the user asks to publish, release, cut a version, or bump the version.
---

# Release

Canonical checklist and rationale: `.github/CONTRIBUTING.md` → "Releasing to
crates.io". This skill is the short path through it; read that file for the
full naming/metadata background if something here is unclear.

1. Bump `version` in `[workspace.package]` (root `Cargo.toml`) **and** the
   matching `version` in each `[workspace.dependencies]` path entry
   (`cdt-tree`, `cdt-search`, `cdt-view`) — all four move together.
2. Confirm CI is green on `main` (fmt, clippy, tests, MSRV, publish dry run).
3. `cargo package --workspace --list` — no stray or missing files.
4. `cargo doc --no-deps --workspace` passes (docs.rs builds after publishing).
5. crates.io has a [Trusted Publishing](https://crates.io/docs/trusted-publishing)
   config for each of the four crates (repo + `ci.yml` + the `publish` job).
   One-time, done on crates.io itself — not something this repo's code can
   set up. If any crate is missing it, the real-publish step below fails at
   the auth action.
6. `git tag v<version> && git push --tags`. The push is what triggers the
   actual publish: `.github/workflows/ci.yml`'s `publish` job runs
   `cargo publish --workspace --locked` only when `github.ref` starts with
   `refs/tags/v`, authenticating over OIDC via `rust-lang/crates-io-auth-action`
   — no local token involved.
7. Watch the Actions run for that tag push to confirm the publish step ran
   (not just the dry run) and succeeded.

Fallback (Trusted Publishing is GitHub-Actions-only — use this from a
workstation instead): `cargo login` with a publish-scoped token, then
`cargo publish --workspace --dry-run` followed by `cargo publish --workspace`.
If publishing crate-by-crate instead of `--workspace`, go leaves-up:
`cdt-tree` → `cdt-search` → `cdt-view` → `cdtui`.

A published version can never be deleted or reused — a mistake is yanked
(`cargo yank --version X -p <crate>`), not removed, and the fix ships as a new
version.
