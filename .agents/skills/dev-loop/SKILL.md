---
name: dev-loop
description: Try a cdtui code change end-to-end, including the actual cd, without installing the binary. Use when the user wants to test, try out, or verify a change to the picker, or asks how to run cdt during development.
---

# Dev loop

`cdt` only ever prints the chosen path on stdout
(`packages/cdtui/src/lib.rs`) — a process cannot change its parent shell's
working directory, so something in the shell has to perform the `cd`. Three
launchers in `scripts/` do that straight from this clone, no install needed:

| shell | command | notes |
|---|---|---|
| bash/zsh/dash — Git Bash, WSL, MSYS2, Cygwin included | `. scripts/dev.sh` | must be **sourced** (`. `), not executed, or the cd is lost in the subshell |
| PowerShell | `.\scripts\dev.ps1` | no sourcing needed — a `.ps1`'s `Set-Location` already affects the caller, measured in this repo, not assumed |
| cmd.exe | `scripts\dev.cmd` | no `call` needed — a `.cmd` run by name already shares the caller's process |

All three accept a starting directory as an argument, e.g.
`. scripts/dev.sh packages` / `.\scripts\dev.ps1 packages`.

For a longer session, skip the launcher and install the real wrapper instead:
`cargo build --release`, put `target/release` ahead of the installed copy on
`PATH`, then run `cdt --init | ...` once per shell (see CONTRIBUTING.md §
"Running the picker while working on it" for the exact one-liner per shell).
After that, `cargo build --release` + `cdt` is the whole loop.

**The picker is interactive** (ratatui + crossterm raw mode) — it reads real
key presses and cannot be driven from a non-interactive or piped shell (no
TTY). Verify a change by actually running one of the launchers above in a
real terminal and pressing keys; do not try to script or automate the TUI
itself. Non-interactive checks belong in `cargo test` (`cdt-tree`'s and
`cdt-view`'s own unit tests cover the tree/key logic without a terminal).
