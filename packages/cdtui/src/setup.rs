//! `cdt install` / `cdt uninstall`: write (or remove) the shell wrapper for
//! every shell this machine actually has, instead of leaving it to be pasted
//! into a profile by hand.
use std::error::Error;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Bracket our block inside an rc/profile file so a second `install` updates
/// in place instead of duplicating, and `uninstall` removes exactly what was
/// added — nothing else in the file is touched.
const BEGIN: &str = "# >>> cdt install >>>";
const END: &str = "# <<< cdt install <<<";

/// Which shells this machine has, by looking for their executable on `PATH`
/// — the only thing "installed" can mean for a program with no package
/// manager of its own to ask.
pub fn detected() -> Vec<&'static str> {
    let mut found: Vec<&'static str> = ["bash", "zsh", "pwsh"]
        .into_iter()
        .filter(|&bin| on_path(bin))
        .map(|bin| crate::shell::classify(bin).expect("every probed name is in KNOWN"))
        .collect();
    if cfg!(windows) {
        if on_path("powershell") {
            found.push("powershell");
        }
        found.push("cmd"); // always present on Windows
    }
    found.sort_unstable();
    found.dedup();
    found
}

/// Also used by `cdt health` to check for `rg`, which is not a shell.
pub(crate) fn on_path(bin: &str) -> bool {
    let Some(paths) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&paths)
        .any(|dir| dir.join(bin).is_file() || dir.join(format!("{bin}.exe")).is_file())
}

fn home() -> io::Result<PathBuf> {
    let var = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
    std::env::var_os(var)
        .map(PathBuf::from)
        .ok_or_else(|| io::Error::other(format!("no home directory (${var} is not set)")))
}

/// Install writes straight into these files (or, on Windows, re-detects them
/// fresh each call, since `cdt install` can run once for several shells).
/// Every bash/zsh shell has exactly one rc file; PowerShell can have two —
/// see [`powershell_profiles`].
fn rc_paths(shell: &str) -> io::Result<Vec<PathBuf>> {
    match shell {
        "bash" => Ok(vec![home()?.join(".bashrc")]),
        "zsh" => Ok(vec![home()?.join(".zshrc")]),
        "powershell" => powershell_profiles(),
        _ => unreachable!("only bash, zsh and powershell use an rc file"),
    }
}

/// PowerShell has no single `$PROFILE`: Windows PowerShell 5.1 and PowerShell
/// 7+ each read their own file. Which one actually runs for a given `cdt` is
/// decided by whatever launched that shell — a terminal emulator's own
/// config (WezTerm's `default_prog`, Windows Terminal's profile list, a
/// shortcut, a scheduled task — not by which one the user happens to type
/// into by habit. Guessing a single "right" file is how install can succeed
/// yet still leave `cdt` not cd-ing the moment a different launcher is used;
/// writing into every PowerShell profile this machine could have sidesteps
/// the guess entirely.
// ponytail: assumes Documents lives at %USERPROFILE%\Documents, which is
// wrong if it has been redirected — point `cdt install powershell` at the
// right file by hand if so.
fn powershell_profiles() -> io::Result<Vec<PathBuf>> {
    let home = home()?;
    if cfg!(windows) {
        Ok(vec![
            home.join("Documents/PowerShell/Microsoft.PowerShell_profile.ps1"),
            home.join("Documents/WindowsPowerShell/Microsoft.PowerShell_profile.ps1"),
        ])
    } else {
        Ok(vec![home.join(
            ".config/powershell/Microsoft.PowerShell_profile.ps1",
        )])
    }
}

/// Where the doskey macro definitions live. Any name/extension would do —
/// `doskey /macrofile` just reads lines, it is not executed as a script —
/// this one just reads clearly in a directory listing.
fn doskey_path() -> io::Result<PathBuf> {
    Ok(home()?.join(".cdt-doskey.cmd"))
}

/// The registry value that makes every new `cmd.exe` session load our
/// macro file automatically — the closest thing cmd has to "source an rc
/// file on start". It is a single shared string, so other tools' own
/// `AutoRun` commands (if any) are chained onto it with `&`, never replaced.
const AUTORUN_KEY: &str = r"HKCU\Software\Microsoft\Command Processor";

/// The exact command appended to `AutoRun`, and the token `install`/
/// `uninstall` search for to add or remove just their own piece of it.
fn doskey_token(path: &Path) -> String {
    format!(r#"doskey /macrofile="{}""#, path.display()).to_string()
}

/// `reg.exe` is always on `PATH` on Windows, so shelling out to it reads and
/// writes the one registry value this needs without pulling in a registry
/// crate for it.
fn autorun_value() -> io::Result<String> {
    let out = std::process::Command::new("reg")
        .args(["query", AUTORUN_KEY, "/v", "AutoRun"])
        .output()?;
    if !out.status.success() {
        return Ok(String::new()); // value does not exist yet
    }
    let text = String::from_utf8_lossy(&out.stdout);
    Ok(text
        .lines()
        .find_map(|line| line.split_once("REG_SZ"))
        .map(|(_, v)| v.trim().to_string())
        .unwrap_or_default())
}

fn set_autorun(value: &str) -> io::Result<()> {
    let status = std::process::Command::new("reg")
        .args(["add", AUTORUN_KEY, "/v", "AutoRun", "/d", value, "/f"])
        .status()?;
    if status.success() {
        Ok(())
    } else {
        Err(io::Error::other("`reg add` failed to set AutoRun"))
    }
}

fn delete_autorun() -> io::Result<()> {
    let status = std::process::Command::new("reg")
        .args(["delete", AUTORUN_KEY, "/v", "AutoRun", "/f"])
        .status()?;
    if status.success() {
        Ok(())
    } else {
        Err(io::Error::other("`reg delete` failed to clear AutoRun"))
    }
}

/// Our block, ready to splice into an rc file: markers around the same
/// wrapper text `--init` used to print.
fn block(shell: &str) -> String {
    let body = crate::wrapper_text(shell).expect("only called for bash, zsh and powershell");
    format!("{BEGIN}\n{body}{END}\n").to_string()
}

/// Drop a previously-inserted block, identified by its markers alone so it
/// is found even if the wrapper text itself changes between versions.
fn without_block(text: &str) -> Option<String> {
    let start = text.find(BEGIN)?;
    let end = start + text[start..].find(END)? + END.len();
    let mut out = String::with_capacity(text.len());
    out.push_str(&text[..start]);
    let rest = &text[end..];
    out.push_str(rest.strip_prefix('\n').unwrap_or(rest));
    Some(out)
}

/// Whether `shell` currently has the wrapper `install` would write — used by
/// `cdt health` to report status without changing anything.
pub fn installed(shell: &str) -> bool {
    if shell == "cmd" {
        let Ok(path) = doskey_path() else {
            return false;
        };
        let file_matches =
            fs::read_to_string(&path).ok().as_deref() == crate::wrapper_text("cmd").as_deref();
        let registered = autorun_value()
            .unwrap_or_default()
            .contains(&doskey_token(&path));
        return file_matches && registered;
    }
    rc_paths(shell).ok().is_some_and(|paths| {
        !paths.is_empty()
            && paths
                .iter()
                .all(|p| fs::read_to_string(p).is_ok_and(|text| text.contains(BEGIN)))
    })
}

fn write_block(path: &Path, shell: &str) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let existing = fs::read_to_string(path).unwrap_or_default();
    let mut text = without_block(&existing).unwrap_or(existing);
    if !text.is_empty() && !text.ends_with('\n') {
        text.push('\n');
    }
    text.push_str(&block(shell));
    fs::write(path, text)
}

pub fn install(shell: &str) -> Result<String, Box<dyn Error>> {
    if shell == "cmd" {
        return install_cmd();
    }
    let paths = rc_paths(shell)?;
    for path in &paths {
        write_block(path, shell)?;
    }
    let written = paths
        .iter()
        .map(|p| p.display().to_string())
        .collect::<Vec<_>>()
        .join(" and ");
    let reload = paths
        .iter()
        .map(|p| {
            if shell == "powershell" {
                format!(". \"{}\"", p.display())
            } else {
                format!("source {}", p.display())
            }
        })
        .collect::<Vec<_>>()
        .join("; ");
    Ok(format!(
        "{shell}: wrote the wrapper into {written} — restart the shell (or run `{reload}`) for it to take effect in this session"
    ))
}

pub fn uninstall(shell: &str) -> Result<String, Box<dyn Error>> {
    if shell == "cmd" {
        return uninstall_cmd();
    }
    let mut removed = Vec::new();
    for path in rc_paths(shell)? {
        let Ok(existing) = fs::read_to_string(&path) else {
            continue;
        };
        let Some(text) = without_block(&existing) else {
            continue;
        };
        fs::write(&path, text)?;
        removed.push(path.display().to_string());
    }
    Ok(if removed.is_empty() {
        format!("{shell}: nothing to remove")
    } else {
        format!(
            "{shell}: removed the wrapper from {}",
            removed.join(" and ")
        )
    })
}

fn install_cmd() -> Result<String, Box<dyn Error>> {
    let path = doskey_path()?;
    fs::write(
        &path,
        crate::wrapper_text("cmd").expect("cmd has a wrapper"),
    )?;
    let token = doskey_token(&path);
    let current = autorun_value()?;
    if !current.contains(&token) {
        let combined = if current.trim().is_empty() {
            token
        } else {
            format!("{current} & {token}").to_string()
        };
        set_autorun(&combined)?;
    }
    Ok(format!(
        "cmd: wrote the doskey macro to {} and set it to load on every new Command Prompt — open a new one for it to take effect",
        path.display()
    ))
}

fn uninstall_cmd() -> Result<String, Box<dyn Error>> {
    let path = doskey_path()?;
    let token = doskey_token(&path);
    let current = autorun_value().unwrap_or_default();
    let was_registered = current.contains(&token);
    if was_registered {
        let stripped = current
            .replace(&format!(" & {token}"), "")
            .replace(&token, "");
        if stripped.trim().is_empty() {
            delete_autorun()?;
        } else {
            set_autorun(stripped.trim())?;
        }
    }
    let had_file = fs::remove_file(&path).is_ok();
    if was_registered || had_file {
        Ok(format!(
            "cmd: removed the doskey macro ({})",
            path.display()
        ))
    } else {
        Ok(format!(
            "cmd: nothing to remove ({} not found)",
            path.display()
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn without_block_removes_only_the_marked_lines() {
        let text = "before\n# >>> cdt install >>>\njunk\n# <<< cdt install <<<\nafter\n";
        assert_eq!(without_block(text).unwrap(), "before\nafter\n");
    }

    #[test]
    fn without_block_is_none_when_there_is_no_block() {
        assert_eq!(without_block("just some rc file\n"), None);
    }

    #[test]
    fn block_is_bracketed_by_the_markers() {
        let b = block("bash");
        assert!(b.starts_with(BEGIN));
        assert!(b.trim_end().ends_with(END));
    }

    /// Reinstalling must not pile up a second copy of the block.
    #[test]
    fn installing_over_an_existing_block_replaces_it_once() {
        let once = format!("x\n{}", block("bash"));
        let after = without_block(&once).unwrap();
        assert_eq!(after, "x\n");
    }
}
