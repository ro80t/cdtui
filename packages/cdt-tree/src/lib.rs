//! Directory listing + up/down navigation for a miller-column browser: each
//! pane is just one directory's children, and `Tree` tracks the one currently
//! browsed.
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub struct Entry {
    pub path: PathBuf,
    pub is_dir: bool,
}

impl Entry {
    /// A drive root (`C:\`) has no file name, so it would otherwise render as
    /// a blank row; show the path itself for anything like that.
    pub fn name(&self) -> std::borrow::Cow<'_, str> {
        match self.path.file_name() {
            Some(n) => n.to_string_lossy(),
            None => self.path.to_string_lossy(),
        }
    }
}

/// Whether an entry should behave as a directory. `DirEntry::file_type` does
/// not follow links, so a symlink or Windows junction pointing at a directory
/// reports `is_dir = false`; left alone it would render as a leaf and refuse
/// to be entered. Only links pay the extra stat.
fn is_dir(e: &std::fs::DirEntry) -> bool {
    match e.file_type() {
        Ok(t) if t.is_dir() => true,
        Ok(t) if t.is_symlink() => e.path().is_dir(),
        _ => false,
    }
}

/// One directory's entries, directories first then files, case-insensitive.
pub fn children(dir: &Path, hidden: bool) -> Vec<Entry> {
    let mut v: Vec<Entry> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| hidden || !e.file_name().to_string_lossy().starts_with('.'))
        .map(|e| Entry {
            is_dir: is_dir(&e),
            path: e.path(),
        })
        .collect();
    v.sort_by_key(|e| (!e.is_dir, e.name().to_lowercase()));
    v
}

/// The drive letters that exist, as `C:\`-style roots. std has no API for
/// this, so each letter is probed; cheap enough since it only runs when
/// stepping up from a drive root. Windows only — other platforms have one
/// root (`/`) already, nothing to list.
#[cfg(windows)]
fn drives() -> Vec<PathBuf> {
    (b'A'..=b'Z')
        .map(|b| PathBuf::from(format!("{}:\\", b as char)))
        .filter(|p| p.metadata().is_ok())
        .collect()
}

/// `root`'s entries — real directory listing, except the empty path, which
/// stands for the synthetic level above any one drive on Windows.
fn load(root: &Path, hidden: bool) -> Vec<Entry> {
    #[cfg(windows)]
    if root.as_os_str().is_empty() {
        return drives()
            .into_iter()
            .map(|path| Entry { path, is_dir: true })
            .collect();
    }
    children(root, hidden)
}

/// What `root`'s parent directory is, by the same rule [`Tree::up`] uses: a
/// drive root on Windows steps out to the synthetic drives level instead of
/// stopping, since `C:\` has no parent the way nested directories do.
fn parent_root(root: &Path) -> Option<PathBuf> {
    if let Some(p) = root.parent() {
        return Some(p.to_path_buf());
    }
    #[cfg(windows)]
    if !root.as_os_str().is_empty() {
        return Some(PathBuf::new());
    }
    None
}

/// What the parent pane shows for `root`, without moving into it. `None` once
/// there is truly nothing above (e.g. `/` on non-Windows, or the drives level
/// itself on Windows).
pub fn up_listing(root: &Path, hidden: bool) -> Option<Vec<Entry>> {
    parent_root(root).map(|p| load(&p, hidden))
}

pub struct Tree {
    pub root: PathBuf,
    pub entries: Vec<Entry>,
    pub hidden: bool,
}

impl Tree {
    pub fn new(root: PathBuf, hidden: bool) -> Self {
        Tree {
            entries: load(&root, hidden),
            root,
            hidden,
        }
    }

    /// `root`, or — on the synthetic drives level — a label for it, since an
    /// empty path would otherwise render as a blank title bar.
    pub fn display_root(&self) -> std::borrow::Cow<'_, str> {
        if self.root.as_os_str().is_empty() {
            return "This PC".into();
        }
        self.root.to_string_lossy()
    }

    // No `is_empty`: nothing in this codebase checks a Tree for emptiness,
    // only its length (for `sel` bounds), so the pair convention clippy wants
    // would just be dead code.
    #[allow(clippy::len_without_is_empty)]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn get(&self, i: usize) -> Option<&Entry> {
        self.entries.get(i)
    }

    pub fn reload(&mut self) {
        self.entries = load(&self.root, self.hidden);
    }

    /// Re-root one directory up. False once there is truly nothing above.
    pub fn up(&mut self) -> bool {
        match parent_root(&self.root) {
            Some(p) => {
                self.root = p;
                self.reload();
                true
            }
            None => false,
        }
    }

    /// Re-root into `path`. False (no-op) if it is not a directory.
    pub fn down(&mut self, path: PathBuf) -> bool {
        if !path.is_dir() {
            return false;
        }
        self.root = path;
        self.reload();
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn down_then_up_round_trips_the_root() {
        let start = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let mut t = Tree::new(start.clone(), false);
        let src = t
            .entries
            .iter()
            .find(|e| e.is_dir)
            .expect("src/ exists")
            .path
            .clone();

        assert!(t.down(src.clone()));
        assert_eq!(t.root, src);

        assert!(t.up());
        assert_eq!(t.root, start);
    }

    #[test]
    fn down_into_a_file_is_a_no_op() {
        let start = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let mut t = Tree::new(start.clone(), false);
        let file = t
            .entries
            .iter()
            .find(|e| !e.is_dir)
            .expect("Cargo.toml exists")
            .path
            .clone();

        assert!(!t.down(file));
        assert_eq!(t.root, start);
    }

    #[test]
    fn up_listing_shows_the_parent_without_moving() {
        let start = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let listing = up_listing(&start, false).expect("has a parent");
        assert!(listing.iter().any(|e| e.path == start));
    }

    #[test]
    fn a_link_to_a_directory_counts_as_a_directory() {
        let base = std::env::temp_dir().join(format!("cdt-tree-{}", std::process::id()));
        let real = base.join("real");
        std::fs::create_dir_all(&real).expect("temp dir");
        let link = base.join("link");

        #[cfg(windows)]
        let made = std::os::windows::fs::symlink_dir(&real, &link).is_ok();
        #[cfg(unix)]
        let made = std::os::unix::fs::symlink(&real, &link).is_ok();

        // Creating a symlink needs admin or Developer Mode on Windows; skip
        // there rather than failing the suite, as the rg tests do.
        if made {
            let kids = children(&base, false);
            let l = kids
                .iter()
                .find(|e| e.name() == "link")
                .expect("the link is listed");
            assert!(l.is_dir, "a link to a directory must expand like one");
        }
        let _ = std::fs::remove_dir_all(&base);
    }

    /// The core contract this module exists for: Windows has no single root,
    /// so stepping up from a drive has to surface the other drives instead of
    /// just stopping, and stop for real once that level is reached.
    #[cfg(windows)]
    #[test]
    fn up_crosses_from_a_drive_root_to_the_drives_level_and_then_stops() {
        let mut t = Tree::new(PathBuf::from(env!("CARGO_MANIFEST_DIR")), false);
        while t.up() {} // walk out past every ancestor directory
        assert!(t.root.as_os_str().is_empty(), "{:?}", t.root);
        assert!(!t.entries.is_empty(), "no drives listed");
        assert!(t.entries.iter().all(|e| e.is_dir));
        assert_eq!(t.display_root(), "This PC");

        // The current drive must be among them, named without a blank row.
        let cur = std::env::current_dir().unwrap();
        let letter = cur.to_string_lossy().chars().next().unwrap();
        let want = PathBuf::from(format!("{letter}:\\"));
        assert!(
            t.entries
                .iter()
                .any(|e| e.path == want && !e.name().is_empty()),
            "{want:?} not in {:?}",
            t.entries.iter().map(|e| &e.path).collect::<Vec<_>>()
        );

        assert!(!t.up(), "nothing above the drives level");
    }
}
