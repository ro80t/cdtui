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
    pub fn name(&self) -> std::borrow::Cow<'_, str> {
        self.path.file_name().unwrap_or_default().to_string_lossy()
    }
}

pub struct Tree {
    pub root: PathBuf,
    pub entries: Vec<Entry>,
    pub hidden: bool,
}

/// One directory's entries, directories first then files, case-insensitive.
pub fn children(dir: &Path, hidden: bool, depth: usize) -> Vec<Entry> {
    let mut v: Vec<Entry> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| hidden || !e.file_name().to_string_lossy().starts_with('.'))
        .map(|e| Entry {
            is_dir: e.file_type().map(|t| t.is_dir()).unwrap_or(false),
            path: e.path(),
            depth,
            open: false,
        })
        .collect();
    v.sort_by_key(|e| (!e.is_dir, e.name().to_lowercase()));
    v
}

/// Number of entries right after `i` that are nested under it.
pub fn descendants(entries: &[Entry], i: usize) -> usize {
    let d = entries[i].depth;
    entries[i + 1..].iter().take_while(|e| e.depth > d).count()
}

impl Tree {
    pub fn new(root: PathBuf, hidden: bool) -> Self {
        Tree {
            entries: children(&root, hidden, 0),
            root,
            hidden,
        }
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
        self.entries = children(&self.root, self.hidden, 0);
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

    /// Re-root one directory up. False at the filesystem root.
    pub fn up(&mut self) -> bool {
        let Some(parent) = self.root.parent().map(Path::to_path_buf) else {
            return false;
        };
        self.root = parent;
        self.reload();
        true
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
    fn parent_of_walks_out_one_level() {
        let t = Tree {
            root: PathBuf::new(),
            hidden: false,
            entries: vec![e(0), e(1), e(2)],
        };
        assert_eq!(t.parent_of(0), None);
        assert_eq!(t.parent_of(2), Some(1));
    }
}
