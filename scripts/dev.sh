#!/usr/bin/env sh
# Run the picker straight from this clone and cd where you picked, without
# installing anything.
#
#   . scripts/dev.sh              # start at the current directory
#   . scripts/dev.sh packages     # start somewhere else
#
# SOURCE it, do not execute it. An executed script gets its own process, and a
# process cannot change its parent's directory — the same reason `cdt` itself
# prints a path instead of cd-ing. Sourcing runs it in your shell, where `cd`
# means something.
#
# Keep using it as `cargo build` + `cdt` once the wrapper is installed; this is
# for the state before that, or for testing a build you have not installed.

if ! (return 0 2>/dev/null); then
    echo "dev.sh has to be sourced, or the cd is lost with the subshell:" >&2
    echo "    . scripts/dev.sh ${*:-}" >&2
    exit 1
fi

# $0 is the shell when sourced, so the path has to come from BASH_SOURCE
# (bash) or $0 (zsh, which does set it for sourced files).
_cdt_root=$(cd "$(dirname "${BASH_SOURCE:-$0}")/.." && pwd) || return 1

# cargo writes progress and the UI to stderr, so stdout is only the picked
# path. Nothing printed means the picker was quit: stay put. Requiring a real
# directory also keeps `. scripts/dev.sh --init` from trying to cd into the
# wrapper text that prints on stdout.
_cdt_dir=$(cargo run -q --manifest-path "$_cdt_root/Cargo.toml" -- "$@")
if [ -d "$_cdt_dir" ]; then
    cd "$_cdt_dir" || return 1
elif [ -n "$_cdt_dir" ]; then
    printf '%s\n' "$_cdt_dir"
fi

unset _cdt_root _cdt_dir
