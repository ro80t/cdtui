//! Search backends. Names go through ripgrep's walker (`ignore` crate, in
//! process); content goes through the `rg` binary.
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Cap on returned paths — a cd picker never needs more.
pub const MAX_HITS: usize = 500;

/// Paths under `root` whose file name contains `pat` (case-insensitive),
/// skipping anything .gitignore excludes.
pub fn find_names(root: &Path, pat: &str, hidden: bool) -> Vec<PathBuf> {
    // An empty pattern matches every name, which would answer a cleared query
    // with an arbitrary 500 paths. Nothing typed, nothing found — same as
    // `grep`, so backspacing a query away empties the hit list either way.
    if pat.is_empty() {
        return Vec::new();
    }
    let pat = pat.to_lowercase();
    ignore::WalkBuilder::new(root)
        .hidden(!hidden)
        .build()
        .flatten()
        .filter(|e| e.depth() > 0)
        .filter(|e| {
            e.file_name()
                .to_string_lossy()
                .to_lowercase()
                .contains(&pat)
        })
        .take(MAX_HITS)
        .map(|e| e.path().to_path_buf())
        .collect()
}

/// Files under `root` containing `pat`, via `rg -l`. Smart-case, like ripgrep's
/// own default. Errors if the `rg` binary is missing.
pub fn grep(root: &Path, pat: &str, hidden: bool) -> io::Result<Vec<PathBuf>> {
    if pat.is_empty() {
        return Ok(Vec::new());
    }
    let mut cmd = Command::new("rg");
    cmd.args(["--files-with-matches", "--color=never", "--smart-case"]);
    if hidden {
        cmd.arg("--hidden");
    }
    let out = cmd.arg(pat).arg(root).output()?;
    Ok(String::from_utf8_lossy(&out.stdout)
        .lines()
        .take(MAX_HITS)
        .map(PathBuf::from)
        .collect())
}

/// `p` relative to `root`, with forward slashes, for display.
pub fn label(root: &Path, p: &Path) -> String {
    p.strip_prefix(root)
        .unwrap_or(p)
        .to_string_lossy()
        .replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
    }

    #[test]
    fn find_names_matches_case_insensitively() {
        let hits = find_names(&root(), "LIB.RS", false);
        assert!(
            hits.iter().any(|p| label(&root(), p) == "src/lib.rs"),
            "{hits:?}"
        );
    }

    #[test]
    fn find_names_matches_directories_too() {
        // `src` is a directory in this crate: folder names must be searchable,
        // not just file names.
        let hits = find_names(&root(), "src", false);
        let src = root().join("src");
        assert!(hits.contains(&src), "{hits:?}");
        assert!(src.is_dir());
    }

    #[test]
    fn an_empty_pattern_finds_nothing_in_either_backend() {
        // Clearing the query must empty the hit list, not flood it.
        assert!(find_names(&root(), "", false).is_empty());
        if let Ok(hits) = grep(&root(), "", false) {
            assert!(hits.is_empty());
        }
    }

    #[test]
    fn grep_finds_a_string_in_this_file() {
        // Skip where ripgrep isn't installed rather than failing the suite.
        let Ok(hits) = grep(&root(), "MAX_HITS", false) else {
            return;
        };
        assert!(
            hits.iter().any(|p| label(&root(), p) == "src/lib.rs"),
            "{hits:?}"
        );
    }
}
