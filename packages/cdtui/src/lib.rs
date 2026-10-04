//! Thin shell around [`cdt_view::pick`]: choose a directory, print it on
//! stdout for the shell wrapper to `cd` into.
//!
//! A process cannot change its parent's working directory, so the `cd` has to
//! happen in the shell. [`snippet`] carries the wrapper for each shell and
//! `--init` prints it, which keeps one copy of it instead of one in the code
//! and another drifting in the README. When nobody is capturing stdout the
//! printed path would go nowhere, so [`run`] says so rather than looking broken.
//!
//! Shipped under two command names, `cdt` and `cdtui`, which are both one-line
//! binaries in `src/bin/` calling [`run`].
use std::io::IsTerminal;
use std::path::PathBuf;

mod shell;

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
";

/// The setup line for one shell, or every shell when we cannot tell which.
fn setup_hint(shell: Option<&str>) -> String {
    let one = match shell {
        Some("bash" | "zsh" | "sh") => "  eval \"$(cdt --init)\"          # add to ~/.bashrc\n",
        Some("powershell" | "pwsh") => {
            "  cdt --init | Out-String | Invoke-Expression          # add to $PROFILE\n"
        }
        Some("cmd" | "bat") => {
            "  cdt --init > \"%USERPROFILE%\\bin\\cdt.cmd\"\n  \
             then put that directory on PATH before %USERPROFILE%\\.cargo\\bin\n"
        }
        // Detection failed, or a shell with no wrapper: offer the lot.
        _ => {
            "  bash/zsh    eval \"$(cdt --init bash)\"\n  \
             PowerShell  cdt --init powershell | Out-String | Invoke-Expression\n  \
             cmd.exe     cdt --init cmd > \"%USERPROFILE%\\bin\\cdt.cmd\"\n"
        }
    };
    format!("{NO_WRAPPER}\n{one}\nRun `cdt --help` for the rest.")
}

const HELP: &str = "\
cdt — pick a directory in a TUI and cd there.

  cdt [DIR]            browse from DIR, or the current directory
  cdt --init           print the shell wrapper that performs the cd,
                       detecting the shell from the parent process
  cdt --init SHELL     the same, for a named shell
                       (bash | zsh | powershell | cmd)
  cdt --help           this text

The picker draws on stderr and prints only the chosen path on stdout, so the
wrapper can capture it. Without the wrapper the path is printed and nothing
moves. Keys: j/k move, l expand, h up, / find names, s grep contents,
. hidden, Enter cd, q quit.";

/// The wrapper source for `shell`, or `None` if that shell is not supported.
fn snippet(shell: &str) -> Option<&'static str> {
    match shell {
        "bash" | "zsh" | "sh" => Some(BASH),
        "powershell" | "pwsh" => Some(POWERSHELL),
        "cmd" | "bat" => Some(CMD),
        _ => None,
    }
}

/// The wrapper for `shell` as it should be written out. The cmd one is
/// redirected straight into a `.cmd` file, and cmd can mis-parse a batch file
/// with bare LF line endings, so that one goes out as CRLF.
pub fn init_text(shell: &str) -> Option<String> {
    let s = snippet(shell)?;
    Some(match shell {
        "cmd" | "bat" => s.replace('\n', "\r\n"),
        _ => s.to_owned(),
    })
}

/// Pick a directory and print it. Prints nothing if the user quits.
pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        // Help goes to stderr on purpose: stdout is the path channel, and a
        // wrapper would otherwise try to cd into this text.
        Some("-h" | "--help") => {
            eprintln!("{HELP}");
            return Ok(());
        }
        Some("--init") => {
            // No shell named: read it off the parent process. An explicit name
            // still wins, for the cases detection cannot see (a profile being
            // generated for another machine, say).
            let named = args.get(1).cloned();
            let shell = match named {
                Some(s) => s,
                None => shell::detect()
                    .ok_or("could not tell which shell this is — name it: cdt --init bash | zsh | powershell | cmd")?
                    .to_owned(),
            };
            let Some(s) = init_text(&shell) else {
                return Err(format!(
                    "no wrapper for {shell:?}: supported shells are bash, zsh, powershell and cmd"
                )
                .into());
            };
            print!("{s}");
            return Ok(());
        }
        _ => {}
    }

    let root = plain(
        args.into_iter()
            .next()
            .map(PathBuf::from)
            .unwrap_or(std::env::current_dir()?)
            .canonicalize()?,
    );
    if let Some(dir) = cdt_view::pick(root)? {
        println!("{}", dir.display());
        // stdout still being a console means no wrapper captured the path, so
        // the cd silently did not happen. Say why instead of looking broken.
        if std::io::stdout().is_terminal() {
            eprintln!("{}", setup_hint(shell::detect()));
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

    /// The cmd wrapper is redirected into a .cmd file, which cmd can mis-parse
    /// with bare LF endings; the shell snippets must stay LF.
    #[test]
    fn only_the_cmd_wrapper_is_written_with_crlf() {
        let cmd = init_text("cmd").unwrap();
        assert!(cmd.contains("\r\n"), "cmd wrapper must be CRLF");
        assert!(!cmd.contains("\n\n"), "no bare LF should survive: {cmd:?}");
        assert_eq!(cmd.matches('\n').count(), cmd.matches("\r\n").count());

        for s in ["bash", "powershell"] {
            assert!(!init_text(s).unwrap().contains('\r'), "{s} must stay LF");
        }
    }

    /// The hint has to name a command that works, and the detecting form is
    /// the one we now tell people to use.
    #[test]
    fn the_hint_names_the_command_for_the_detected_shell() {
        // Each shell gets its own one-liner, and it must be a command that
        // exists: the whole point is that the user can paste it and be done.
        for (sh, must) in [
            ("bash", "eval"),
            ("zsh", "eval"),
            ("powershell", "Invoke-Expression"),
            ("cmd", "cdt.cmd"),
        ] {
            let h = setup_hint(Some(sh));
            assert!(h.contains("--init"), "{sh}: no --init");
            assert!(
                h.contains(must),
                "{sh}: missing {must}
{h}"
            );
        }

        // Unknown or undetectable falls back to listing them all.
        for u in [None, Some("fish")] {
            let h = setup_hint(u);
            for must in ["bash", "powershell", "cmd"] {
                assert!(h.contains(must), "{u:?}: missing {must}");
            }
        }
    }
}
