//! Thin shell around [`cdt_view::pick`]: choose a directory, print it on
//! stdout for the shell function to `cd` into.
//!
//! Shipped under two command names, `cdt` and `cdtui`, which are both one-line
//! binaries in `src/bin/` calling [`run`].
use std::path::PathBuf;

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

/// Pick a directory and print it. Prints nothing if the user quits.
pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    let root = plain(
        std::env::args()
            .nth(1)
            .map(PathBuf::from)
            .unwrap_or(std::env::current_dir()?)
            .canonicalize()?,
    );
    if let Some(dir) = cdt_view::pick(root)? {
        println!("{}", dir.display());
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
}
