//! The picker itself: state, view, key loop. Renders on stderr so stdout can
//! carry the chosen path back to the calling shell.
use std::io;
use std::path::{Path, PathBuf};

use cdt_tree::Tree;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::crossterm::execute;
use ratatui::crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph};

type Res<T> = Result<T, Box<dyn std::error::Error>>;

const HELP: &str = "j/k move  l expand  h up  / find  s grep  . hidden  Enter cd  q quit";

enum Mode {
    Tree,
    Find(String),
    Grep(String),
}

struct App {
    tree: Tree,
    hits: Vec<PathBuf>,
    mode: Mode,
    sel: usize,
    msg: String,
}

impl App {
    fn new(root: PathBuf) -> Self {
        App {
            tree: Tree::new(root, false),
            hits: Vec::new(),
            mode: Mode::Tree,
            sel: 0,
            msg: String::new(),
        }
    }

    fn len(&self) -> usize {
        match self.mode {
            Mode::Tree => self.tree.len(),
            _ => self.hits.len(),
        }
    }

    fn selected(&self) -> Option<&Path> {
        match self.mode {
            Mode::Tree => self.tree.get(self.sel).map(|e| e.path.as_path()),
            _ => self.hits.get(self.sel).map(PathBuf::as_path),
        }
    }

    /// Directory to cd into: the entry itself if a dir, else its parent.
    fn target(&self) -> PathBuf {
        match self.selected() {
            Some(p) if p.is_dir() => p.to_path_buf(),
            Some(p) => p.parent().unwrap_or(&self.tree.root).to_path_buf(),
            None => self.tree.root.clone(),
        }
    }

    fn move_by(&mut self, d: isize) {
        let n = self.len();
        if n > 0 {
            self.sel = (self.sel as isize + d).rem_euclid(n as isize) as usize;
        }
    }

    fn back_to_tree(&mut self) {
        self.mode = Mode::Tree;
        self.hits.clear();
        self.sel = 0;
        self.msg.clear();
    }

    /// `h`: close an open dir, else step out to the parent, else re-root upward.
    fn collapse_or_up(&mut self) {
        match self.tree.get(self.sel) {
            Some(e) if e.is_dir && e.open => self.tree.toggle(self.sel),
            Some(_) if self.tree.parent_of(self.sel).is_some() => {
                self.sel = self.tree.parent_of(self.sel).unwrap();
            }
            _ => {
                if self.tree.up() {
                    self.sel = 0;
                }
            }
        }
    }

    fn search(&mut self, q: &str) {
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

fn draw(f: &mut Frame, app: &App) {
    let [body, bar] = Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).areas(f.area());

    let items: Vec<ListItem> = match app.mode {
        Mode::Tree => app
            .tree
            .entries
            .iter()
            .map(|e| {
                let icon = if !e.is_dir {
                    "  "
                } else if e.open {
                    "▾ "
                } else {
                    "▸ "
                };
                let style = if e.is_dir {
                    Style::new().fg(Color::Cyan).bold()
                } else {
                    Style::new()
                };
                ListItem::new(format!("{}{icon}{}", "  ".repeat(e.depth), e.name())).style(style)
            })
            .collect(),
        _ => app
            .hits
            .iter()
            .map(|p| ListItem::new(cdt_search::label(&app.tree.root, p)))
            .collect(),
    };

    let mut state = ListState::default().with_selected(Some(app.sel));
    f.render_stateful_widget(
        List::new(items)
            .block(
                Block::new()
                    .borders(Borders::ALL)
                    .title(format!(" {} ", app.tree.root.display())),
            )
            .highlight_style(Style::new().reversed()),
        body,
        &mut state,
    );

    let status = match &app.mode {
        _ if !app.msg.is_empty() => app.msg.clone(),
        Mode::Find(q) => format!("find: {q}_  ({} hits)", app.hits.len()),
        Mode::Grep(q) => format!("grep: {q}_  ({} files)", app.hits.len()),
        Mode::Tree => HELP.into(),
    };
    f.render_widget(
        Paragraph::new(status).style(Style::new().fg(Color::DarkGray)),
        bar,
    );
}

/// Run the picker. `Ok(None)` means the user quit without choosing.
pub fn pick(root: PathBuf) -> Res<Option<PathBuf>> {
    let mut app = App::new(root);
    enable_raw_mode()?;
    execute!(io::stderr(), EnterAlternateScreen)?;
    let picked = event_loop(
        &mut Terminal::new(CrosstermBackend::new(io::stderr()))?,
        &mut app,
    );
    disable_raw_mode()?;
    execute!(io::stderr(), LeaveAlternateScreen)?;
    picked
}

fn event_loop(
    term: &mut Terminal<CrosstermBackend<io::Stderr>>,
    app: &mut App,
) -> Res<Option<PathBuf>> {
    loop {
        term.draw(|f| draw(f, app))?;
        let Event::Key(k) = event::read()? else {
            continue;
        };
        if k.kind != KeyEventKind::Press {
            continue;
        }
        if let Mode::Find(q) | Mode::Grep(q) = &app.mode {
            let mut q = q.clone();
            match k.code {
                KeyCode::Char(c) => q.push(c),
                KeyCode::Backspace => {
                    q.pop();
                }
                KeyCode::Esc => {
                    app.back_to_tree();
                    continue;
                }
                KeyCode::Enter => return Ok(Some(app.target())),
                KeyCode::Down | KeyCode::Tab => {
                    app.move_by(1);
                    continue;
                }
                KeyCode::Up => {
                    app.move_by(-1);
                    continue;
                }
                _ => continue,
            }
            app.msg.clear();
            app.search(&q);
            app.mode = match app.mode {
                Mode::Grep(_) => Mode::Grep(q),
                _ => Mode::Find(q),
            };
            continue;
        }
        match k.code {
            KeyCode::Char('q') | KeyCode::Esc => return Ok(None),
            KeyCode::Enter => return Ok(Some(app.target())),
            KeyCode::Char('j') | KeyCode::Down => app.move_by(1),
            KeyCode::Char('k') | KeyCode::Up => app.move_by(-1),
            KeyCode::Char('g') => app.sel = 0,
            KeyCode::Char('G') => app.sel = app.len().saturating_sub(1),
            KeyCode::Char('l') | KeyCode::Right | KeyCode::Tab | KeyCode::Char(' ') => {
                app.tree.toggle(app.sel)
            }
            KeyCode::Char('h') | KeyCode::Left => app.collapse_or_up(),
            KeyCode::Char('.') => {
                app.tree.hidden = !app.tree.hidden;
                app.tree.reload();
                app.sel = 0;
            }
            KeyCode::Char('/') => app.mode = Mode::Find(String::new()),
            KeyCode::Char('s') => app.mode = Mode::Grep(String::new()),
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn move_by_wraps_and_tolerates_an_empty_list() {
        let mut app = App::new(PathBuf::from(env!("CARGO_MANIFEST_DIR")));
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
        let mut app = App::new(root.clone());
        app.mode = Mode::Find(String::new());
        app.hits = vec![root.join("src/lib.rs")];
        assert_eq!(app.target(), root.join("src"));
    }
}
