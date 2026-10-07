//! Thin shell around [`cdt_view::pick`]: choose a directory, print it on
//! stdout for the shell wrapper to `cd` into.
//!
//! A process cannot change its parent's working directory, so the `cd` has to
//! happen in the shell. [`snippet`] carries the wrapper for each shell, and
//! [`setup`] writes it into the right rc file (or `.cmd` file) so nobody has
//! to paste it by hand. When nobody is capturing stdout the printed path
//! would go nowhere, so [`run`] says so rather than looking broken.
//!
//! Shipped under two command names, `cdt` and `cdtui`, which are both one-line
//! binaries in `src/bin/` calling [`run`].
use clap::{CommandFactory, Parser, Subcommand};
use std::io::IsTerminal;
use std::path::PathBuf;

mod setup;
mod shell;

/// Names clap's own usage/options text, generated from the doc comments
/// below, so it is not retyped by hand and cannot drift out of sync with the
/// actual flags. `-h`/`--help` is still handled by hand in [`run`] — clap's
/// built-in flag would print straight to stdout, which is the path channel —
/// so `render_help` is called explicitly and the result goes to stderr.
/// `--version` was never a flag here, and `disable_help_subcommand` turns off
/// clap's own `cdt help`, which would print to stdout for the same reason.
#[derive(Parser)]
#[command(
    name = "cdt",
    about = "pick a directory in a TUI and cd there.",
    after_help = KEYS,
    disable_help_flag = true,
    disable_version_flag = true,
    disable_help_subcommand = true,
    color = clap::ColorChoice::Never
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,

    /// Print this help and exit.
    #[arg(short = 'h', long = "help")]
    help: bool,

    /// Browse from DIR, or the current directory.
    dir: Option<PathBuf>,
}

/// A process cannot change its parent shell's directory, so one of these has
/// to run first.
#[derive(Subcommand)]
enum Command {
    /// Write the wrapper into every shell this machine has (or just the
    /// SHELLS named).
    Install { shells: Vec<String> },
    /// Remove the wrapper `install` added, from every shell this machine has
    /// (or just the SHELLS named).
    Uninstall { shells: Vec<String> },
    /// Report which shells have the wrapper installed, and whether `rg` is
    /// on PATH for full-text search.
    Health,
}

/// The part clap cannot generate: how the picker behaves once it opens.
/// Appended after the flag/argument listing via `after_help`.
const KEYS: &str = "\
The picker draws on stderr and prints only the chosen path on stdout, so the
wrapper can capture it. Without the wrapper the path is printed and nothing
moves. Keys: j/k move, l expand, h up, / find names, s grep contents,
. hidden, Enter cd, q quit.";

/// The shells `install`/`uninstall` know a wrapper for, as shown in error
/// messages.
const SHELLS: &str = "bash | zsh | powershell | cmd";

/// The `\\?\` extended-length prefix `canonicalize` adds on Windows.
const VERBATIM: &str = r#"\\?\"#;
/// Its UNC form, `\\?\UNC\server\share`.
const VERBATIM_UNC: &str = r#"\\?\UNC\"#;

/// `canonicalize` returns a `\\?\` verbatim path on Windows, which both `cd`
/// and `Set-Location` reject ("path does not exist"), so the path we print has
/// to have the prefix taken back off. No-op on other platforms.
fn plain(p: PathBuf) -> PathBuf {
    let Some(s) = p.to_str() else { return p };
    // A share has to go back to \\server\share, not the literal "UNC\..."
    // that a blind prefix strip would leave behind.
    if let Some(share) = s.strip_prefix(VERBATIM_UNC) {
        return PathBuf::from([r#"\\"#, share].concat());
    }
    match s.strip_prefix(VERBATIM) {
        Some(rest) => PathBuf::from(rest),
        None => p,
    }
}

/// bash and zsh. `command cdt` skips the function so it does not recurse.
const BASH: &str = r#"cdt() { local d; d=$(command cdt "$@") && [ -n "$d" ] && cd "$d"; }
cdtui() { cdt "$@"; }
"#;

/// PowerShell. `-LiteralPath` is required: without it `Set-Location` reads
/// `[` and `]` in a directory name as wildcards and cannot find it.
const POWERSHELL: &str = r#"function cdt { $d = cdt.exe @args; if ($d) { Set-Location -LiteralPath $d } }
Set-Alias cdtui cdt
"#;

/// cmd.exe, which has no functions: this has to be saved as a file on PATH.
/// `%%d` is the in-a-file spelling; at the prompt the same loop takes `%d`.
const CMD: &str = r#"@echo off
rem Save as cdt.cmd in a PATH directory that comes BEFORE %USERPROFILE%\.cargo\bin.
rem Within one directory cmd prefers .EXE over .CMD, so next to cdt.exe this
rem file would never run. Check with: where cdt
for /f "delims=" %%d in ('cdt.exe %*') do cd /d "%%d"
"#;

/// Shown on stderr when a chosen path was printed with nothing to catch it.
const NO_WRAPPER: &str = "\
cdt: printed the path above but could not cd — a program cannot change its
     parent shell's directory, so a one-line shell wrapper has to do it.
     Run `cdt install` to set one up, or see `cdt --help`.";

/// The wrapper source for `shell`, or `None` if that shell is not supported.
fn snippet(shell: &str) -> Option<&'static str> {
    match shell {
        "bash" | "zsh" | "sh" => Some(BASH),
        "powershell" | "pwsh" => Some(POWERSHELL),
        "cmd" | "bat" => Some(CMD),
        _ => None,
    }
}

/// The wrapper for `shell` as it should be written out. The cmd one goes out
/// as CRLF: it is saved straight to a `.cmd` file, and cmd can mis-parse a
/// batch file with bare LF line endings.
pub fn wrapper_text(shell: &str) -> Option<String> {
    let s = snippet(shell)?;
    Some(match shell {
        "cmd" | "bat" => s.replace('\n', "\r\n"),
        _ => s.to_owned(),
    })
}

/// Resolve `names` to canonical shell keys — every shell this machine has,
/// if none were named — run `action` for each, and report every result
/// before failing, so one bad name does not hide what happened to the rest.
fn setup_many(
    names: Vec<String>,
    verb: &str,
    action: impl Fn(&str) -> Result<String, Box<dyn std::error::Error>>,
) -> Result<(), Box<dyn std::error::Error>> {
    let names: Vec<String> = if names.is_empty() {
        setup::detected().into_iter().map(str::to_owned).collect()
    } else {
        names
    };
    if names.is_empty() {
        eprintln!(
            "cdt: no shell found on this machine to {verb} — name one: cdt {verb} <shell> ({SHELLS})"
        );
        return Ok(());
    }
    let mut failed = false;
    for name in names {
        let outcome = match shell::classify(&name).filter(|s| snippet(s).is_some()) {
            Some(canon) => action(canon),
            None => Err(format!("no wrapper for {name:?}: supported shells are {SHELLS}").into()),
        };
        match outcome {
            Ok(msg) => eprintln!("{msg}"),
            Err(e) => {
                eprintln!("cdt: {e}");
                failed = true;
            }
        }
    }
    if failed {
        Err("some shells could not be set up; see above".into())
    } else {
        Ok(())
    }
}

/// `cdt health`: a report, not a gate — it always exits 0, same as `brew
/// doctor` or `flutter doctor`, since nothing here stops the picker working.
fn health() {
    let shells = setup::detected();
    if shells.is_empty() {
        eprintln!("cdt: no supported shell found on this machine ({SHELLS})");
    }
    for shell in shells {
        let state = if setup::installed(shell) {
            "wrapper installed"
        } else {
            "wrapper not installed — run `cdt install`"
        };
        eprintln!("{shell}: {state}");
    }
    let rg = if setup::on_path("rg") {
        "found — full-text search (s) available"
    } else {
        "not found — browsing and name search (/) still work, but s needs it"
    };
    eprintln!("rg: {rg}");
}

/// Pick a directory and print it. Prints nothing if the user quits.
pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    // A bad flag exits here with clap's own "error: unexpected argument ..."
    // on stderr, rather than falling through and being canonicalized as a
    // bogus directory.
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(e) => e.exit(),
    };

    // Help goes to stderr on purpose: stdout is the path channel, and a
    // wrapper would otherwise try to cd into this text.
    if cli.help {
        eprint!("{}", Cli::command().render_help());
        return Ok(());
    }

    match cli.command {
        Some(Command::Install { shells }) => return setup_many(shells, "install", setup::install),
        Some(Command::Uninstall { shells }) => {
            return setup_many(shells, "uninstall", setup::uninstall);
        }
        Some(Command::Health) => {
            health();
            return Ok(());
        }
        None => {}
    }

    let root = plain(cli.dir.unwrap_or(std::env::current_dir()?).canonicalize()?);
    if let Some(dir) = cdt_view::pick(root)? {
        println!("{}", dir.display());
        // stdout still being a console means no wrapper captured the path, so
        // the cd silently did not happen. Say why instead of looking broken.
        if std::io::stdout().is_terminal() {
            eprintln!("{NO_WRAPPER}");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Guards the literals themselves: a mangled escape here would make every
    /// other test in this module pass for the wrong reason.
    #[test]
    fn the_prefix_literals_are_the_bytes_windows_actually_emits() {
        assert_eq!(VERBATIM.as_bytes(), br#"\\?\"#);
        assert_eq!(VERBATIM.len(), 4);
        assert_eq!(VERBATIM_UNC.len(), 8);
        assert!(VERBATIM_UNC.starts_with(VERBATIM));
    }

    #[test]
    fn plain_strips_the_verbatim_disk_prefix() {
        let got = plain(PathBuf::from([VERBATIM, r"E:\GitHub\cdtui"].concat()));
        assert_eq!(got, PathBuf::from(r"E:\GitHub\cdtui"));
    }

    #[test]
    fn plain_restores_a_verbatim_unc_share() {
        let got = plain(PathBuf::from([VERBATIM_UNC, r"nas\pub\src"].concat()));
        assert_eq!(got, PathBuf::from(r#"\\nas\pub\src"#));
    }

    #[test]
    fn plain_leaves_an_ordinary_path_alone() {
        for s in [r"E:\GitHub\cdtui", "/home/me/src", r#"\\nas\pub"#] {
            assert_eq!(plain(PathBuf::from(s)), PathBuf::from(s));
        }
    }

    /// The end-to-end shape: whatever canonicalize hands back, what we print
    /// must not carry the prefix and must still name the same directory.
    #[test]
    fn canonicalize_output_is_usable_after_plain() {
        let raw = PathBuf::from(".").canonicalize().unwrap();
        let out = plain(raw.clone());
        assert!(!out.to_string_lossy().contains('?'), "{out:?}");
        assert!(out.is_dir(), "{out:?}");
        if cfg!(windows) {
            assert!(raw.to_string_lossy().starts_with(VERBATIM), "{raw:?}");
            assert!(out != raw, "prefix was not stripped: {out:?}");
        }
    }

    #[test]
    fn every_supported_shell_has_a_snippet_and_others_do_not() {
        for s in ["bash", "zsh", "sh", "powershell", "pwsh", "cmd", "bat"] {
            assert!(snippet(s).is_some_and(|t| !t.is_empty()), "{s}");
        }
        for s in ["fish", "nushell", "", "BASH"] {
            assert!(snippet(s).is_none(), "{s}");
        }
    }

    /// Each snippet must define the `cdt` name and actually run the binary;
    /// a wrapper that does neither would leave the user exactly where they
    /// started, which is the bug this whole mechanism exists to prevent.
    #[test]
    fn each_snippet_defines_cdt_and_invokes_the_binary() {
        for s in ["bash", "powershell", "cmd"] {
            let t = snippet(s).unwrap();
            assert!(t.contains("cdt"), "{s}: no cdt");
            assert!(
                t.contains("cd ") || t.contains("Set-Location"),
                "{s}: never cds"
            );
        }
    }

    #[test]
    fn the_powershell_snippet_uses_literalpath() {
        // Plain `Set-Location $d` fails on a directory whose name contains
        // brackets, because they are read as wildcards.
        let t = snippet("powershell").unwrap();
        assert!(t.contains("-LiteralPath"), "{t}");
    }

    #[test]
    fn the_cmd_snippet_uses_the_in_a_file_percent_spelling() {
        // `%%d` is correct inside a .cmd file; `%d` only works typed at the
        // prompt, and the snippet is meant to be saved to a file.
        let t = snippet("cmd").unwrap();
        assert!(t.contains("%%d"), "{t}");
    }

    /// The cmd wrapper is written straight to a .cmd file, which cmd can
    /// mis-parse with bare LF endings; the shell snippets must stay LF.
    #[test]
    fn only_the_cmd_wrapper_is_written_with_crlf() {
        let cmd = wrapper_text("cmd").unwrap();
        assert!(cmd.contains("\r\n"), "cmd wrapper must be CRLF");
        assert!(!cmd.contains("\n\n"), "no bare LF should survive: {cmd:?}");
        assert_eq!(cmd.matches('\n').count(), cmd.matches("\r\n").count());

        for s in ["bash", "powershell"] {
            assert!(!wrapper_text(s).unwrap().contains('\r'), "{s} must stay LF");
        }
    }
}
