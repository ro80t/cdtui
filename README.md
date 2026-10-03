# cdt

[![CI](https://github.com/ro80t/cdt/actions/workflows/ci.yml/badge.svg)](https://github.com/ro80t/cdt/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/cdtui.svg)](https://crates.io/crates/cdtui)

A neo-tree style terminal UI for picking a directory to `cd` into — browse the
tree, search it with ripgrep, press Enter, and your shell is there.

## Install

```sh
cargo install cdtui   # the crate is cdtui; the command it installs is cdt, short for cdtui
```

Needs the `rg` binary on `PATH` for full-text search (`s`); browsing and
file-name search work without it.

## Shell setup

A process cannot change its parent's directory, so `cdt` draws its UI on stderr
and prints only the chosen path to stdout. One shell function does the `cd`:

bash / zsh (`~/.bashrc`, `~/.zshrc`):

```sh
cdt() { local d; d=$(command cdt "$@") && [ -n "$d" ] && cd "$d"; }
```

PowerShell (`$PROFILE`):

```powershell
function cdt { $d = cdt.exe @args; if ($d) { Set-Location $d } }
```

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
| `/` | search file names (honours `.gitignore`) |
| `s` | full-text search with ripgrep |
| `.` | toggle hidden files |
| `Enter` | `cd` there and exit (a file resolves to its directory) |
| `q` / `Esc` | exit without changing directory |

`/` does not need ripgrep installed: it uses ripgrep's own walker (the `ignore`
crate) as a library. `s` shells out to `rg`.

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
