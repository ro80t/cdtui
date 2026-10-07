//! Naming a shell: both the `install`/`uninstall` CLI arguments and the
//! `PATH` scan in [`crate::setup`] need to turn a shell's executable name
//! (`bash.exe`, `/usr/bin/zsh`, ...) or a bare name typed by hand into one of
//! the keys [`crate::snippet`] knows a wrapper for.

/// Shells we can emit a wrapper for, as the name [`crate::snippet`] expects.
/// `.exe` is stripped before matching, so each shell needs one entry. `dash`
/// and `ash` are here because they are `/bin/sh` on Debian and Alpine, and the
/// bash wrapper is verified to work in them. ksh is deliberately absent: its
/// older releases lack `local`, and an unverified wrapper is worse than the
/// error telling you to name your shell.
const KNOWN: &[(&str, &str)] = &[
    ("cmd", "cmd"),
    ("powershell", "powershell"),
    ("pwsh", "powershell"),
    ("bash", "bash"),
    ("sh", "bash"),
    ("dash", "bash"),
    ("ash", "bash"),
    ("zsh", "zsh"),
    ("fish", "fish"),
];

/// Map an executable's file name (or a bare name typed on the command line)
/// to a shell name, ignoring case, path and a Windows `.exe` suffix.
pub(crate) fn classify(exe: &str) -> Option<&'static str> {
    let lower = exe.to_ascii_lowercase();
    let base = lower.rsplit(['\\', '/']).next().unwrap_or(&lower);
    let base = base.strip_suffix(".exe").unwrap_or(base);
    KNOWN.iter().find(|(f, _)| *f == base).map(|(_, s)| *s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_reads_a_bare_name_a_path_and_any_case() {
        assert_eq!(classify("cmd.exe"), Some("cmd"));
        assert_eq!(classify("CMD.EXE"), Some("cmd"));
        assert_eq!(classify(r"C:\WINDOWS\system32\cmd.exe"), Some("cmd"));
        assert_eq!(classify("/usr/bin/bash"), Some("bash"));
        assert_eq!(classify("pwsh.exe"), Some("powershell"));
        assert_eq!(classify("PowerShell.exe"), Some("powershell"));
    }

    /// `/bin/sh` is dash on Debian and ash on Alpine, so a Linux user naming
    /// plain sh has to be recognised, not told to guess.
    #[test]
    fn the_posix_sh_implementations_are_recognised() {
        for exe in ["dash", "dash.exe", "/bin/dash", "ash", "sh", "/bin/sh"] {
            assert_eq!(classify(exe), Some("bash"), "{exe}");
        }
    }

    #[test]
    fn classify_rejects_things_that_are_not_shells() {
        for s in ["cargo.exe", "explorer.exe", "", "cdt.exe", "bashful.exe"] {
            assert_eq!(classify(s), None, "{s}");
        }
    }

    /// Every name we can classify must have a wrapper to emit, or naming it
    /// to `install`/`uninstall` would detect a shell and then fail to serve
    /// it.
    #[test]
    fn every_classified_shell_has_a_snippet() {
        for (exe, shell) in KNOWN {
            // fish is recognised so the error can name it, but is unsupported.
            if *shell == "fish" {
                assert!(crate::wrapper_text(shell).is_none());
                continue;
            }
            assert!(
                crate::wrapper_text(shell).is_some(),
                "{exe} -> {shell} has no snippet"
            );
        }
    }
}
