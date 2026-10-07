//! Directory tree model: a flat `Vec<Entry>` where nesting is held in `depth`.
//! Expanding splices children in, collapsing drains the nested run back out.
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub struct Entry {
    pub path: PathBuf,
    pub depth: usize,
    pub is_dir: bool,
    pub open: bool,
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

pub struct Tree {
    pub root: PathBuf,
    pub entries: Vec<Entry>,
    pub hidden: bool,
}

/// Whether an entry should behave as a directory. `DirEntry::file_type` does
/// not follow links, so a symlink or Windows junction pointing at a directory
/// reports `is_dir = false`; left alone it would render as a leaf and refuse
/// to expand. Only links pay the extra stat.
fn is_dir(e: &std::fs::DirEntry) -> bool {
    match e.file_type() {
        Ok(t) if t.is_dir() => true,
        Ok(t) if t.is_symlink() => e.path().is_dir(),
        _ => false,
    }
}

/// One directory's entries, directories first then files, case-insensitive.
pub fn children(dir: &Path, hidden: bool, depth: usize) -> Vec<Entry> {
    let mut v: Vec<Entry> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| hidden || !e.file_name().to_string_lossy().starts_with('.'))
        .map(|e| Entry {
            is_dir: is_dir(&e),
            path: e.path(),
            depth,
            open: false,
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
/// [`Tree::up`] uses on Windows as the synthetic level above any one drive.
fn load(root: &Path, hidden: bool, depth: usize) -> Vec<Entry> {
    #[cfg(windows)]
    if root.as_os_str().is_empty() {
        return drives()
            .into_iter()
            .map(|path| Entry {
                path,
                depth,
                is_dir: true,
                open: false,
            })
            .collect();
    }
    children(root, hidden, depth)
}

/// Number of entries right after `i` that are nested under it.
pub fn descendants(entries: &[Entry], i: usize) -> usize {
    let d = entries[i].depth;
    entries[i + 1..].iter().take_while(|e| e.depth > d).count()
}

impl Tree {
    pub fn new(root: PathBuf, hidden: bool) -> Self {
        Tree {
            entries: load(&root, hidden, 0),
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

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn get(&self, i: usize) -> Option<&Entry> {
        self.entries.get(i)
    }

    pub fn reload(&mut self) {
        self.entries = load(&self.root, self.hidden, 0);
    }

    /// Expand or collapse the directory at `i`. No-op on files.
    pub fn toggle(&mut self, i: usize) {
        let Some(e) = self.entries.get(i) else { return };
        if !e.is_dir {
            return;
        }
        if e.open {
            let n = descendants(&self.entries, i);
            self.entries.drain(i + 1..=i + n);
            self.entries[i].open = false;
        } else {
            let kids = children(&e.path, self.hidden, e.depth + 1);
            self.entries[i].open = true;
            self.entries.splice(i + 1..i + 1, kids);
        }
    }

    /// Index of the entry that `i` is nested under.
    pub fn parent_of(&self, i: usize) -> Option<usize> {
        let d = self.entries.get(i)?.depth;
        if d == 0 {
            return None;
        }
        self.entries[..i].iter().rposition(|e| e.depth < d)
    }

    /// Re-root one directory up. At a drive root on Windows, steps out to the
    /// synthetic drives level ([`load`]) instead of stopping, since `C:\` has
    /// no parent the way nested directories do. False once there is truly
    /// nothing above (the drives level itself, or `/` on other platforms).
    pub fn up(&mut self) -> bool {
        if let Some(parent) = self.root.parent() {
            self.root = parent.to_path_buf();
            self.reload();
            return true;
        }
        #[cfg(windows)]
        if !self.root.as_os_str().is_empty() {
            self.root = PathBuf::new();
            self.reload();
            return true;
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn e(depth: usize) -> Entry {
        Entry {
            path: PathBuf::from("x"),
            depth,
            is_dir: true,
            open: false,
        }
    }

    #[test]
    fn descendants_counts_only_the_nested_run() {
        let t = vec![e(0), e(1), e(2), e(1), e(0)];
        assert_eq!(descendants(&t, 0), 3);
        assert_eq!(descendants(&t, 1), 1);
        assert_eq!(descendants(&t, 2), 0);
        assert_eq!(descendants(&t, 4), 0);
    }

    #[test]
    fn expand_then_collapse_restores_the_entry_list() {
        // Own crate dir: contains src/ (a dir) and Cargo.toml.
        let mut t = Tree::new(PathBuf::from(env!("CARGO_MANIFEST_DIR")), false);
        let before = t.len();
        let i = t
            .entries
            .iter()
            .position(|e| e.is_dir)
            .expect("src/ exists");
        t.toggle(i);
        assert!(t.len() > before && t.entries[i].open);
        t.toggle(i);
        assert_eq!(t.len(), before);
        assert!(!t.entries[i].open);
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
            let kids = children(&base, false, 0);
            let l = kids
                .iter()
                .find(|e| e.name() == "link")
                .expect("the link is listed");
            assert!(l.is_dir, "a link to a directory must expand like one");
        }
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn parent_of_walks_out_one_level() {
        let t = Tree {
            root: PathBuf::new(),
            hidden: false,
            entries: vec![e(0), e(1), e(2)],
        };
        assert_eq!(t.parent_of(0), None);
        assert_eq!(t.parent_of(2), Some(1));
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
