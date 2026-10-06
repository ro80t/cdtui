//! Which shell is asking for `--init`.
//!
//! Environment variables cannot answer this. Measured on Windows, a PowerShell
//! started from cmd inherits `PROMPT`, and one started from Git Bash inherits
//! `SHELL` and `MSYSTEM`, so every env-based guess reports the wrong shell in
//! ordinary nesting. The direct parent process does answer it: whichever shell
//! the user typed the command into is the process that spawned us.
//!
//! The walk goes upward until it recognises a shell, so `cargo run -- --init`
//! looks past `cargo.exe` to the shell behind it.

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

/// Map an executable's file name to a shell name, ignoring case, path and a
/// Windows `.exe` suffix.
fn classify(exe: &str) -> Option<&'static str> {
    let lower = exe.to_ascii_lowercase();
    let base = lower.rsplit(['\\', '/']).next().unwrap_or(&lower);
    let base = base.strip_suffix(".exe").unwrap_or(base);
    KNOWN.iter().find(|(f, _)| *f == base).map(|(_, s)| *s)
}

/// Stop climbing eventually: a corrupt snapshot could otherwise loop.
const MAX_DEPTH: usize = 12;

#[cfg(windows)]
pub fn detect() -> Option<&'static str> {
    let procs = snapshot()?;
    let parent_of = |pid: u32| procs.iter().find(|p| p.pid == pid).map(|p| p.ppid);
    let name_of = |pid: u32| procs.iter().find(|p| p.pid == pid).map(|p| p.name.as_str());

    let mut pid = parent_of(std::process::id())?;
    for _ in 0..MAX_DEPTH {
        if let Some(s) = classify(name_of(pid)?) {
            return Some(s);
        }
        pid = parent_of(pid)?;
    }
    None
}

#[cfg(windows)]
struct Proc {
    pid: u32,
    ppid: u32,
    name: String,
}

/// Every running process, from one ToolHelp snapshot.
#[cfg(windows)]
fn snapshot() -> Option<Vec<Proc>> {
    use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW,
        TH32CS_SNAPPROCESS,
    };

    // SAFETY: the handle is checked before use and closed on every exit path;
    // the entry is fully initialised with its required dwSize before each call,
    // and both enumeration calls only write into that owned struct.
    unsafe {
        let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snap == INVALID_HANDLE_VALUE || snap.is_null() {
            return None;
        }
        let mut e: PROCESSENTRY32W = std::mem::zeroed();
        e.dwSize = size_of::<PROCESSENTRY32W>() as u32;
        let mut out = Vec::new();
        if Process32FirstW(snap, &mut e) != 0 {
            loop {
                let len = e.szExeFile.iter().position(|&c| c == 0).unwrap_or(0);
                out.push(Proc {
                    pid: e.th32ProcessID,
                    ppid: e.th32ParentProcessID,
                    name: String::from_utf16_lossy(&e.szExeFile[..len]),
                });
                e.dwSize = size_of::<PROCESSENTRY32W>() as u32;
                if Process32NextW(snap, &mut e) == 0 {
                    break;
                }
            }
        }
        CloseHandle(snap);
        Some(out)
    }
}

/// The parent pid in a Linux `/proc/<pid>/stat` line.
///
/// Not behind a `cfg`, so it is compiled and tested on every platform and not
/// only where it runs. The second field is the executable name in parentheses
/// and it may itself contain spaces and parentheses, so the columns are only
/// unambiguous after the final `)`: `pid (comm) state ppid ...`.
// Only the tests reach it on Windows; narrowed to that platform so a genuine
// unused function on Unix is still reported.
#[cfg_attr(windows, allow(dead_code))]
fn ppid_from_stat(stat: &str) -> Option<u32> {
    let after = stat.get(stat.rfind(')')? + 1..)?;
    after.split_whitespace().nth(1)?.parse().ok()
}

/// Same walk as on Windows, over `/proc`. `$SHELL` is only the login shell —
/// it names the wrong one as soon as anything is nested — so it is the last
/// resort, for systems without `/proc` such as macOS.
#[cfg(not(windows))]
pub fn detect() -> Option<&'static str> {
    proc_walk().or_else(|| classify(&std::env::var("SHELL").ok()?))
}

#[cfg(not(windows))]
fn proc_walk() -> Option<&'static str> {
    let mut pid = std::process::id();
    for _ in 0..MAX_DEPTH {
        let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
        pid = ppid_from_stat(&stat)?;
        if pid == 0 {
            return None; // reached init's parent
        }
        let comm = std::fs::read_to_string(format!("/proc/{pid}/comm")).ok()?;
        if let Some(s) = classify(comm.trim()) {
            return Some(s);
        }
    }
    None
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

    /// `/bin/sh` is dash on Debian and ash on Alpine, so a Linux user running
    /// `cdt --init` under plain sh has to be recognised, not told to guess.
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

    /// Every name we can classify must have a wrapper to emit, or `--init`
    /// would detect a shell and then fail to serve it.
    #[test]
    fn every_classified_shell_has_a_snippet() {
        for (exe, shell) in KNOWN {
            // fish is recognised so the error can name it, but is unsupported.
            if *shell == "fish" {
                assert!(crate::init_text(shell).is_none());
                continue;
            }
            assert!(
                crate::init_text(shell).is_some(),
                "{exe} -> {shell} has no snippet"
            );
        }
    }

    /// The /proc/stat trap: the comm field is untrusted text that can contain
    /// spaces and parentheses, so splitting on whitespace from the left reads
    /// the wrong column and would walk to a nonexistent process.
    #[test]
    fn ppid_is_read_from_after_the_last_paren() {
        assert_eq!(ppid_from_stat("1234 (bash) S 1000 1234 1000 0"), Some(1000));
        assert_eq!(ppid_from_stat("42 (my proc) S 7 42 7 0"), Some(7));
        assert_eq!(ppid_from_stat("42 (a) b) S 7 42 7 0"), Some(7));
        assert_eq!(ppid_from_stat("42 ((nested)) R 99 42"), Some(99));
        // A shell named so that naive parsing would pick a plausible number.
        assert_eq!(ppid_from_stat("5 (S 111 222) S 333 5"), Some(333));
    }

    #[test]
    fn a_malformed_stat_line_is_rejected_rather_than_guessed() {
        for s in [
            "",
            "no parens here",
            "1234 (bash)",
            "1234 (bash) S",
            "(x) S z",
        ] {
            assert_eq!(ppid_from_stat(s), None, "{s:?}");
        }
    }

    #[cfg(windows)]
    #[test]
    fn the_snapshot_sees_this_process_and_its_parent() {
        let procs = snapshot().expect("a process snapshot");
        let me = std::process::id();
        let mine = procs
            .iter()
            .find(|p| p.pid == me)
            .expect("this process is listed");
        assert!(procs.iter().any(|p| p.pid == mine.ppid), "parent listed");
        assert!(procs.len() > 10, "suspiciously short: {}", procs.len());
        assert!(!mine.name.is_empty());
    }

    /// Under `cargo test` the parent chain is cargo, then the shell that ran
    /// it, so detection has to climb rather than read the direct parent.
    #[cfg(windows)]
    #[test]
    fn detect_climbs_past_non_shell_parents() {
        // A detached CI runner may genuinely have no shell above it, so None
        // is allowed; naming a shell we cannot serve is not.
        if let Some(s) = detect() {
            assert!(
                crate::init_text(s).is_some() || s == "fish",
                "detected {s:?} with no snippet"
            );
        }
    }
}
