//! Picker state: what is selected, which mode we are in, and the directory
//! that Enter resolves to.
use std::path::{Path, PathBuf};

use cdt_tree::Tree;

pub(crate) enum Mode {
    Tree,
    Find(String),
    Grep(String),
}

pub(crate) struct App {
    pub(crate) tree: Tree,
    pub(crate) hits: Vec<PathBuf>,
    pub(crate) mode: Mode,
    pub(crate) sel: usize,
    pub(crate) msg: String,
}

impl App {
    pub(crate) fn new(root: PathBuf) -> Self {
        App {
            tree: Tree::new(root, false),
            hits: Vec::new(),
            mode: Mode::Tree,
            sel: 0,
            msg: String::new(),
        }
    }

    pub(crate) fn len(&self) -> usize {
        match self.mode {
            Mode::Tree => self.tree.len(),
            _ => self.hits.len(),
        }
    }

    pub(crate) fn selected(&self) -> Option<&Path> {
        match self.mode {
            Mode::Tree => self.tree.get(self.sel).map(|e| e.path.as_path()),
            _ => self.hits.get(self.sel).map(PathBuf::as_path),
        }
    }

    /// Directory to cd into: the entry itself if a dir, else its parent. Uses
    /// `Path::is_dir`, which follows symlinks, so a link to a directory is a
    /// valid cd target even though the tree renders it as a leaf.
    pub(crate) fn target(&self) -> PathBuf {
        match self.selected() {
            Some(p) if p.is_dir() => p.to_path_buf(),
            Some(p) => p.parent().unwrap_or(&self.tree.root).to_path_buf(),
            None => self.tree.root.clone(),
        }
    }

    pub(crate) fn move_by(&mut self, d: isize) {
        let n = self.len();
        if n > 0 {
            self.sel = (self.sel as isize + d).rem_euclid(n as isize) as usize;
        }
    }

    pub(crate) fn back_to_tree(&mut self) {
        self.mode = Mode::Tree;
        self.hits.clear();
        self.sel = 0;
        self.msg.clear();
    }

    /// `h`: close an open dir, else step out to the parent, else re-root upward.
    pub(crate) fn collapse_or_up(&mut self) {
        match self.tree.get(self.sel) {
            Some(e) if e.is_dir && e.open => self.tree.toggle(self.sel),
            Some(_) => match self.tree.parent_of(self.sel) {
                Some(p) => self.sel = p,
                None => {
                    if self.tree.up() {
                        self.sel = 0;
                    }
                }
            },
            None => {
                if self.tree.up() {
                    self.sel = 0;
                }
            }
        }
    }

    pub(crate) fn toggle_hidden(&mut self) {
        self.tree.hidden = !self.tree.hidden;
        self.tree.reload();
        self.sel = 0;
    }

    pub(crate) fn search(&mut self, q: &str) {
        self.sel = 0;
        match self.mode {
            Mode::Grep(_) => match cdt_search::grep(&self.tree.root, q, self.tree.hidden) {
                Ok(hits) => self.hits = hits,
                Err(e) => {
                    self.hits.clear();
                    self.msg = format!("rg unavailable: {e}");
                }
            },
            _ => self.hits = cdt_search::find_names(&self.tree.root, q, self.tree.hidden),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app() -> App {
        App::new(PathBuf::from(env!("CARGO_MANIFEST_DIR")))
    }

    #[test]
    fn move_by_wraps_and_tolerates_an_empty_list() {
        let mut app = app();
        let n = app.len();
        assert!(n > 0);
        app.move_by(-1);
        assert_eq!(app.sel, n - 1);
        app.move_by(1);
        assert_eq!(app.sel, 0);

        app.mode = Mode::Find(String::new()); // hits is empty
        app.move_by(1);
        assert_eq!(app.sel, 0);
    }

    #[test]
    fn target_of_a_file_is_its_directory() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let mut app = app();
        app.mode = Mode::Find(String::new());
        app.hits = vec![root.join("src/lib.rs")];
        assert_eq!(app.target(), root.join("src"));
    }

    #[test]
    fn target_of_a_directory_is_that_directory() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let mut app = app();
        app.mode = Mode::Find(String::new());
        app.hits = vec![root.join("src")];
        assert_eq!(app.target(), root.join("src"));
    }

    #[test]
    fn target_falls_back_to_the_root_when_nothing_is_selected() {
        let mut app = app();
        app.mode = Mode::Find(String::new()); // hits is empty, so sel points at nothing
        assert_eq!(app.target(), app.tree.root);
    }
}
