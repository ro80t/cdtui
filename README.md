# cdt

[![CI](https://github.com/ro80t/cdtui/actions/workflows/ci.yml/badge.svg)](https://github.com/ro80t/cdtui/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/cdtui.svg)](https://crates.io/crates/cdtui)

A neo-tree style terminal UI for picking a directory to `cd` into — browse the
tree, search it with ripgrep, press Enter, and your shell is there.

## Install

```sh
cargo install cdtui   # installs two names for the same program: cdt and cdtui
```

The crate is `cdtui` because `cdt` was taken on crates.io. Both commands are
installed, so `cdt` works whether or not you remember the abbreviation.

Needs the `rg` binary on `PATH` for full-text search (`s`); browsing and
file-name search work without it.

## Shell setup

A process cannot change its parent's directory, so `cdt` draws its UI on stderr
and prints only the chosen path to stdout. One shell function does the `cd`:

bash / zsh (`~/.bashrc`, `~/.zshrc`):

```sh
cdt() { local d; d=$(command cdt "$@") && [ -n "$d" ] && cd "$d"; }
cdtui() { cdt "$@"; }   # optional: the long name too
```

PowerShell (`$PROFILE`):

```powershell
function cdt { $d = cdt.exe @args; if ($d) { Set-Location $d } }
Set-Alias cdtui cdt   # optional: the long name too
```

The wrapper is what does the `cd`, so a name only works once it has one — the
bare `cdtui` command still just prints a path.

## Usage

```sh
cdt          # start at the current directory
cdt ~/src    # start somewhere else
```

| key | |
|---|---|
| `j` / `k` | move |
| `l` / `Space` / `Tab` | expand / collapse a directory |
| `h` | collapse, then step out to the parent, then re-root one level up |
| `g` / `G` | first / last |
| `/` | search names — files *and* folders (honours `.gitignore`) |
| `s` | full-text search of file contents with ripgrep |
| `.` | toggle hidden files |
| `Enter` | `cd` there and exit (a file resolves to its directory) |
| `q` / `Esc` | exit without changing directory |

While `/` or `s` is open every letter types into the query, so `q` and `.`
search rather than act:

| key | |
|---|---|
| `↑` / `↓` / `Tab` | move through the hits |
| `Backspace` | edit the query |
| `Enter` | `cd` to the selected hit and exit |
| `Esc` | back to the tree |

`/` does not need ripgrep installed: it uses ripgrep's own walker (the `ignore`
crate) as a library. `s` shells out to `rg`. Both run on a worker thread, so
typing stays responsive on a large tree and the status bar shows `searching…`
until the hits land. A count shown as `500+` means the list was capped.

## Libraries

The parts are published separately and usable on their own:
[`cdt-tree`](https://crates.io/crates/cdt-tree) (tree model),
[`cdt-search`](https://crates.io/crates/cdt-search) (name and content search),
[`cdt-view`](https://crates.io/crates/cdt-view) (the ratatui picker).

## Contributing

Workspace layout, dev commands, CI and the release procedure:
[.github/CONTRIBUTING.md](.github/CONTRIBUTING.md).

## License

MIT
