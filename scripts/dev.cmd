@echo off
rem Run the picker straight from this clone and cd where you picked, without
rem installing anything.
rem
rem   scripts\dev.cmd              start at the current directory
rem   scripts\dev.cmd packages     start somewhere else
rem
rem Typed directly at the cmd prompt, a .cmd file runs in the SAME cmd.exe
rem process, not a child one, so "cd" inside it already lands on the caller
rem -- the same reason the installed cdt.cmd wrapper (see src/lib.rs) can
rem "cd /d" without any sourcing trick. Calling this with `call` from another
rem script loses that, same as `call`-ing any other batch file.
rem
rem Once the wrapper from `cdt --init cmd` is installed, `cargo build` + `cdt`
rem is the faster loop; this is for before that.

set "_cdt_root=%~dp0.."
if not exist "%_cdt_root%\Cargo.toml" (
    echo dev.cmd: cannot find the cdtui workspace next to this script. 1>&2
    exit /b 1
)

rem cargo writes progress and the UI to stderr, so stdout is only the picked
rem path. Nothing printed means the picker was quit: stay put.
set "_cdt_dir="
for /f "delims=" %%d in ('cargo run -q --manifest-path "%_cdt_root%\Cargo.toml" -- %*') do set "_cdt_dir=%%d"

if defined _cdt_dir (
    if exist "%_cdt_dir%\" (
        cd /d "%_cdt_dir%"
    ) else (
        echo %_cdt_dir%
    )
)

set "_cdt_root="
set "_cdt_dir="
