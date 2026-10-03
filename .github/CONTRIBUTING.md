# Contributing

Repository: <https://github.com/ro80t/cdtui> — user-facing docs live in
[README.md](../README.md); everything about working on the code is here.

## Getting started

```sh
git clone https://github.com/ro80t/cdtui
cd cdtui
cargo run -p cdtui          # the UI draws on stderr, so this is safe to watch
cargo install --path packages/cdtui
```

Requires Rust 1.88 or newer (`rust-version` in the root `Cargo.toml`) and, for
the full-text search, the `rg` binary on `PATH`.

## Workspace layout

| crate | directory | role | deps |
|---|---|---|---|
| `cdt-tree` | `packages/cdt-tree` | tree model: expand/collapse, ordering, re-rooting | none (std only) |
| `cdt-search` | `packages/cdt-search` | file-name search (`ignore`) and full-text search (`rg`) | `ignore` |
| `cdt-view` | `packages/cdt-view` | rendering, key handling, terminal setup | `ratatui`, `cdt-tree`, `cdt-search` |
| `cdtui` | `packages/cdtui` | the `cdt` binary (`cdt` = cdtui abbreviated): args in, chosen path to stdout | `cdt-view` |

Each directory under `packages/` is named exactly after the crate it holds.
Other rules that keep the split worth having:

- **Dependencies flow one way:** `cdtui` → `cdt-view` → {`cdt-tree`,
  `cdt-search`}. Never add an edge back up.
- `cdt-tree` stays std-only. Anything needing a crate belongs in `cdt-search`
  or `cdt-view`.
- `cdtui` stays a thin shell. Terminal raw-mode handling lives in `cdt-view`
  (`cdt_view::pick`) so the binary does not depend on ratatui.
- Shared metadata (version, license, repository, MSRV, keywords) lives in
  `[workspace.package]`; dependency versions live in
  `[workspace.dependencies]`. Do not pin a version inside a member crate.

## Day-to-day commands

```sh
cargo test --workspace
cargo test -p cdt-tree         # one crate
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all
```

Each crate keeps its own unit tests in the file they cover. New non-trivial
logic gets one test that fails if it breaks — not a suite per function.

## Faster builds with sccache

ratatui pulls in ~190 crates, so most of a cold build is dependencies. CI runs
every compiling job through [sccache](https://github.com/mozilla/sccache),
which caches compiler output per object and reuses it across jobs and runs.

Locally it is opt-in, because a wired-in `rustc-wrapper` would break anyone
without sccache installed:

```sh
cargo install sccache --locked   # or: winget install sccache / scoop install sccache / brew install sccache
```

```sh
export RUSTC_WRAPPER=sccache CARGO_INCREMENTAL=0   # bash / zsh
```

```powershell
$env:RUSTC_WRAPPER = "sccache"; $env:CARGO_INCREMENTAL = "0"   # PowerShell
```

```sh
sccache --show-stats   # hit rate; expect misses on the first build only
```

`CARGO_INCREMENTAL=0` is required: sccache refuses to cache incremental
compilation. That is the trade-off — for a tight edit-and-rebuild loop inside
one crate, plain incremental builds win; sccache pays off when switching
branches, bumping dependencies, or rebuilding from a clean `target/`.

To make it permanent for yourself without touching the repo, put the wrapper in
your own config (`~/.cargo/config.toml`), not in the project's.

## CI

`.github/workflows/ci.yml` runs on every push to `main` and every PR:

| job | what it guards |
|---|---|
| `test` | clippy + tests on ubuntu and windows, `--locked` |
| `fmt` | `cargo fmt --all --check` |
| `msrv` | builds on exactly 1.88.0, so `rust-version` cannot drift |
| `publish` | `cargo publish --workspace --dry-run`, so release-blocking metadata fails on the PR instead |

`rg` is installed on both runners: without it the `cdt-search` grep test skips
itself and a regression would go unnoticed.

Caching is sccache alone (via `mozilla-actions/sccache-action`, backed by the
GitHub Actions cache), set up through the workflow-level `RUSTC_WRAPPER` and
`SCCACHE_GHA_ENABLED`. Each job ends with `sccache --show-stats` so a dropping
hit rate is visible in the log. The `fmt` job clears `RUSTC_WRAPPER` because
rustfmt never invokes rustc. No `Swatinem/rust-cache` on top: caching the whole
`target/` directory as well only duplicates what sccache already holds.

Dependabot (`.github/dependabot.yml`) opens weekly grouped PRs for cargo
dependencies and for the actions used above.

## Releasing to crates.io

### Naming

The crates.io namespace is flat and first-come-first-served. **`cdt` is already
taken** (a Delaunay triangulation crate, published 2021), so the binary crate is
published as `cdtui`.

| | crates.io name | name in Rust code | notes |
|---|---|---|---|
| bin | `cdtui` | — | `cargo install cdtui` installs a binary called **`cdt`** |
| lib | `cdt-tree` | `cdt_tree` | |
| lib | `cdt-search` | `cdt_search` | |
| lib | `cdt-view` | `cdt_view` | |

- **A binary's name is independent of its package name** and is not subject to
  the uniqueness constraint. `[[bin]] name = "cdt"` keeps the command `cdt` —
  cdtui abbreviated — while the package publishes as `cdtui`.
- Allowed characters: alphanumerics, `-` and `_`; must start with a letter; max
  64 characters.
- **`-` and `_` are equivalent, and names are case-insensitive.** Taking
  `cdt-tree` also blocks `cdt_tree` and `CDT_Tree`.
- Keep the family prefix consistent (`cdt-*`). A prefix itself cannot be
  reserved, so publish related names together if you want to hold them.
- Names chosen to look confusingly like a well-known crate can be reported as
  squatting.
- A published name cannot be renamed — you republish under a new name and yank
  the old crate.

### Required and recommended metadata

`cargo publish` **requires** `name`, `version`, `description` and `license` (or
`license-file`). Shared fields live in `[workspace.package]`; each crate
inherits them with `license.workspace = true` and friends.

| field | status | notes |
|---|---|---|
| `name`, `version` | required | a version can never be reused or overwritten |
| `description` | required | one or two sentences; write a distinct one per crate |
| `license` | required | SPDX expression, e.g. `MIT` or `MIT OR Apache-2.0` |
| `repository` | effectively required | the link crates.io shows; without it nobody trusts the crate |
| `readme` | recommended | shown on the crate page. All four crates point at `../../README.md`; cargo copies it into the archive |
| `keywords` | recommended | **max 5**, each ≤20 chars, alphanumerics plus `-`/`_` |
| `categories` | recommended | **max 5**, only the [official slugs](https://crates.io/categories); an invalid slug fails the publish |
| `homepage`, `documentation` | optional | omitting `documentation` makes crates.io link docs.rs |
| `rust-version` | recommended | MSRV, guarded by the `msrv` job. 1.88 here: what ratatui 0.30 and ignore 0.4.33 require |
| `authors` | optional | cannot be changed after publishing |
| `include` / `exclude` | optional | to trim what ends up in the archive |

### Workspace-specific gotchas

- **Path dependencies need a `version` too.** Without it the publish fails with
  `all dependencies must have a version specified when publishing`. That is why
  `[workspace.dependencies]` says
  `cdt-tree = { path = "packages/cdt-tree", version = "0.1.0" }`.
- **Every internal crate has to be published as well.** A crate marked
  `publish = false` cannot be depended on by a published crate. If you do not
  want to publish one, merge it into the binary crate instead.
- All four crates share one version via `version.workspace = true`. To version
  them independently, give each crate its own `version` and keep
  `[workspace.dependencies]` in sync.

### Checklist

- [ ] the copyright holder in `LICENSE` (currently `ro80t`) is what you want
- [ ] bump `version` in `[workspace.package]` **and** in the three path deps
- [ ] CI green (that already covers fmt, clippy, tests, MSRV and the dry run)
- [ ] `cargo package --workspace --list` shows no stray or missing files
- [ ] `cargo doc --no-deps --workspace` passes (docs.rs builds after publishing)
- [ ] tag the release (`git tag v0.1.0 && git push --tags`)
- [ ] `cargo login` with a publish-scoped token from
      [account settings](https://crates.io/settings/tokens). From CI prefer
      Trusted Publishing (OIDC) so no long-lived token is stored.

### Publish

Cargo 1.90 and later works out the dependency order for the whole workspace:

```sh
cargo publish --workspace --dry-run
cargo publish --workspace
```

One at a time, go **from the leaves up**:

```sh
cargo publish -p cdt-tree
cargo publish -p cdt-search
cargo publish -p cdt-view    # once the two above are in the index
cargo publish -p cdtui
```

### After publishing

- **Nothing can be deleted.** A mistake is handled with
  `cargo yank --version 0.1.0 -p cdtui`, which only stops new dependents —
  existing `Cargo.lock` files can still fetch it. Ship the fix as a new version.
- While on `0.1.x`, breaking changes are allowed (for 0.x, minor acts as major).
- Raising the MSRV (`rust-version`) is conventionally treated as a minor bump.
- Add co-owners with `cargo owner --add <github-user> cdtui`.
- No per-crate `LICENSE` file is present, so the archives carry only the SPDX
  expression, not the license text. To ship the text as well, copy `LICENSE`
  into each crate directory (or symlink it).
