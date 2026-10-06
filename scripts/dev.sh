#!/usr/bin/env sh
# Run the picker straight from this clone and cd where you picked, without
# installing anything. Works on Linux, macOS and Windows (Git Bash / MSYS2 /
# Cygwin / WSL), in bash, zsh, dash and other POSIX shells.
#
#   . scripts/dev.sh                        # start at the current directory
#   . scripts/dev.sh packages               # start somewhere else
#   CDT_DIR=packages . scripts/dev.sh       # same, for a plain POSIX sh
#
# bash, zsh and ksh let `.` forward arguments; a plain sh (dash, ash) does not
# and silently drops them, so CDT_DIR is the portable way to name a directory.
# A plain sh also cannot see this file's own path, so there the workspace is
# found via git from the cwd — source it from inside the repo, or set CDT_ROOT.
#
# SOURCE it, do not execute it. An executed script gets its own process, and a
# process cannot change its parent's directory — the same reason `cdt` itself
# prints a path instead of cd-ing. Sourcing runs it in your shell, where `cd`
# means something.
#
# Once the wrapper from `cdt --init` is installed, `cargo build` + `cdt` is the
# faster loop; this is for before that, or for a build you have not installed.

# Whether we were sourced. Each shell needs its own test, measured rather than
# assumed: the common `(return 0 2>/dev/null)` trick reports "sourced" even for
# an executed script under dash, which would lose the cd silently.
_cdt_sourced=0
if [ -n "${BASH_SOURCE-}" ]; then
    # bash: $0 stays the shell when sourced, so the two differ.
    [ "${BASH_SOURCE}" != "$0" ] && _cdt_sourced=1
elif [ -n "${ZSH_EVAL_CONTEXT-}" ]; then
    # zsh: the context ends in :file while sourcing a file.
    case $ZSH_EVAL_CONTEXT in
    *:file*) _cdt_sourced=1 ;;
    esac
else
    # dash, ash, ksh, other POSIX sh: $0 is the script only when executed, and
    # the shell's own name ("dash", "sh", "-sh") when sourced.
    case $0 in
    *.sh) [ -f "$0" ] || _cdt_sourced=1 ;;
    *) _cdt_sourced=1 ;;
    esac
fi

if [ "$_cdt_sourced" -eq 0 ]; then
    echo "dev.sh has to be sourced, or the cd is lost with the subshell:" >&2
    echo "    . scripts/dev.sh $*" >&2
    unset _cdt_sourced
    exit 1
fi

# The workspace root. bash has BASH_SOURCE and zsh puts the sourced file in $0,
# but a plain POSIX sh gives neither — there $0 is just "dash"/"sh" — so git
# answers from the cwd, and CDT_ROOT overrides everything.
_cdt_root=${CDT_ROOT-}
if [ -z "$_cdt_root" ]; then
    _cdt_self=${BASH_SOURCE-}
    [ -n "$_cdt_self" ] || _cdt_self=$0
    _cdt_root=$(CDPATH= cd -- "$(dirname -- "$_cdt_self")/.." 2>/dev/null && pwd)
    if [ ! -f "$_cdt_root/Cargo.toml" ]; then
        _cdt_root=$(git rev-parse --show-toplevel 2>/dev/null)
    fi
fi
if [ ! -f "$_cdt_root/Cargo.toml" ]; then
    echo "dev.sh: cannot find the cdtui workspace." >&2
    echo "  A plain POSIX sh cannot see the script's own path, so either source" >&2
    echo "  this from inside the repo, or set CDT_ROOT=/path/to/cdtui." >&2
    unset _cdt_sourced _cdt_self _cdt_root
    return 1
fi

# cargo writes progress and the UI to stderr, so stdout is only the picked
# path. Nothing printed means the picker was quit: stay put. Requiring a real
# directory also keeps `. scripts/dev.sh --init` from trying to cd into the
# wrapper text that prints on stdout.
#
# On Windows the path comes back as E:\like\this; Git Bash and friends accept
# that in both `[ -d ]` and `cd`, so no conversion is needed.
_cdt_m="$_cdt_root/Cargo.toml"
if [ "$#" -gt 0 ]; then
    _cdt_dir=$(cargo run -q --manifest-path "$_cdt_m" -- "$@")
elif [ -n "${CDT_DIR-}" ]; then
    # The sh that could not forward the argument: take it from the environment.
    _cdt_dir=$(cargo run -q --manifest-path "$_cdt_m" -- "$CDT_DIR")
else
    _cdt_dir=$(cargo run -q --manifest-path "$_cdt_m")
fi
if [ -d "$_cdt_dir" ]; then
    CDPATH= cd -- "$_cdt_dir" || :
elif [ -n "$_cdt_dir" ]; then
    printf '%s\n' "$_cdt_dir"
fi

unset _cdt_sourced _cdt_self _cdt_root _cdt_m _cdt_dir
