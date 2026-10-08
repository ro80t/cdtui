//! Terminal lifecycle and the key loop. `on_key` holds the whole key table and
//! touches no IO, so the bindings are testable without a terminal.
use std::io;
use std::path::PathBuf;
use std::time::Duration;

use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::crossterm::execute;
use ratatui::crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::prelude::*;

use crate::Res;
use crate::app::{App, Mode};
use crate::view;

/// What a keypress asks the loop to do. `None` from `on_key` means "redraw".
#[derive(Debug, PartialEq)]
pub(crate) enum Action {
    Quit,
    Pick(PathBuf),
}

/// Puts the terminal back on the way out, on every path: a clean pick, an
/// early `?`, or a panic unwinding through the loop. Without it a crash leaves
/// the caller's shell in raw mode on the alternate screen.
struct Restore;

impl Drop for Restore {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(io::stderr(), LeaveAlternateScreen);
    }
}

/// Run the picker. `Ok(None)` means the user quit without choosing.
pub fn pick(root: PathBuf) -> Res<Option<PathBuf>> {
    let mut app = App::new(root);
    enable_raw_mode()?;
    // Armed only once raw mode is actually on, so a failure above restores
    // nothing it did not change.
    let _restore = Restore;
    execute!(io::stderr(), EnterAlternateScreen)?;
    event_loop(
        &mut Terminal::new(CrosstermBackend::new(io::stderr()))?,
        &mut app,
    )
}

/// How long to wait for a key before looking for search results. Short enough
/// that hits appear promptly, long enough to idle at no measurable cost.
const TICK: Duration = Duration::from_millis(80);

/// How long Enter will wait for a search that has not answered yet. Long
/// enough for a big tree, short enough that a wedged `rg` still lets you out.
const PICK_WAIT: Duration = Duration::from_secs(5);

fn event_loop(
    term: &mut Terminal<CrosstermBackend<io::Stderr>>,
    app: &mut App,
) -> Res<Option<PathBuf>> {
    let mut dirty = true;
    loop {
        if dirty {
            term.draw(|f| view::draw(f, app))?;
            dirty = false;
        }
        // Poll rather than block: a search finishing is also a reason to redraw.
        if event::poll(TICK)? {
            if let Event::Key(k) = event::read()? {
                if k.kind == KeyEventKind::Press {
                    match on_key(app, k.code) {
                        Some(Action::Quit) => return Ok(None),
                        Some(Action::Pick(p)) => return Ok(Some(p)),
                        None => dirty = true,
                    }
                }
            }
        }
        if let Some(done) = app.search.take_fresh() {
            app.apply(done);
            dirty = true;
        }
    }
}

pub(crate) fn on_key(app: &mut App, code: KeyCode) -> Option<Action> {
    match app.mode {
        Mode::Tree => on_tree_key(app, code),
        _ => on_search_key(app, code),
    }
}

/// In search mode every plain character is query text, so `q`, `/` and `.`
/// type rather than act. Enter picks, Esc goes back to the tree.
fn on_search_key(app: &mut App, code: KeyCode) -> Option<Action> {
    let (Mode::Find(q) | Mode::Grep(q)) = &app.mode else {
        return None;
    };
    let grep = matches!(app.mode, Mode::Grep(_));
    let mut q = q.clone();
    match code {
        KeyCode::Char(c) => q.push(c),
        KeyCode::Backspace => {
            q.pop();
        }
        KeyCode::Esc => {
            app.back_to_tree();
            return None;
        }
        KeyCode::Enter => {
            // The result still in flight is the one being chosen. Waiting for
            // it beats picking from an empty list, which would cd to the root.
            if app.search.pending() {
                if let Some(d) = app.search.wait(PICK_WAIT) {
                    app.apply(d);
                }
            }
            return Some(Action::Pick(app.target()));
        }
        KeyCode::Down | KeyCode::Tab => {
            app.move_by(1);
            return None;
        }
        KeyCode::Up => {
            app.move_by(-1);
            return None;
        }
        _ => return None,
    }
    app.mode = if grep { Mode::Grep(q) } else { Mode::Find(q) };
    // Mode is set first so the worker is told which backend the new query wants.
    let (Mode::Find(q) | Mode::Grep(q)) = &app.mode else {
        return None;
    };
    let q = q.clone();
    app.request_search(&q);
    None
}

fn on_tree_key(app: &mut App, code: KeyCode) -> Option<Action> {
    match code {
        KeyCode::Char('q') | KeyCode::Esc => return Some(Action::Quit),
        KeyCode::Enter => return Some(Action::Pick(app.target())),
        KeyCode::Char('j') | KeyCode::Down => app.move_by(1),
        KeyCode::Char('k') | KeyCode::Up => app.move_by(-1),
        KeyCode::Char('g') => app.sel = 0,
        KeyCode::Char('G') => app.sel = app.len().saturating_sub(1),
        KeyCode::Char('l') | KeyCode::Right | KeyCode::Tab | KeyCode::Char(' ') => {
            app.tree.toggle(app.sel)
        }
        KeyCode::Char('h') | KeyCode::Left => app.collapse_or_up(),
        KeyCode::Char('.') => app.toggle_hidden(),
        KeyCode::Char('/') => app.mode = Mode::Find(String::new()),
        KeyCode::Char('s') => app.mode = Mode::Grep(String::new()),
        _ => {}
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app() -> App {
        App::new(PathBuf::from(env!("CARGO_MANIFEST_DIR")))
    }

    #[test]
    fn q_and_esc_quit_without_choosing() {
        assert_eq!(on_key(&mut app(), KeyCode::Char('q')), Some(Action::Quit));
        assert_eq!(on_key(&mut app(), KeyCode::Esc), Some(Action::Quit));
    }

    /// The core contract: Enter hands back a directory and ends the loop.
    #[test]
    fn enter_picks_a_directory_and_ends_the_loop() {
        let mut app = app();
        if let Some(Action::Pick(p)) = on_key(&mut app, KeyCode::Enter) {
            assert!(p.is_dir(), "{p:?}");
        } else {
            panic!("Enter must pick");
        };
    }

    #[test]
    fn l_expands_a_directory_and_h_collapses_it() {
        let mut app = app();
        app.sel = app
            .tree
            .entries
            .iter()
            .position(|e| e.is_dir)
            .expect("src/ exists");
        let before = app.tree.len();

        assert_eq!(on_key(&mut app, KeyCode::Char('l')), None);
        assert!(app.tree.len() > before);

        assert_eq!(on_key(&mut app, KeyCode::Char('h')), None);
        assert_eq!(app.tree.len(), before);
    }

    #[test]
    fn slash_and_s_enter_the_two_search_modes() {
        let mut app = app();
        on_key(&mut app, KeyCode::Char('/'));
        assert!(matches!(app.mode, Mode::Find(ref q) if q.is_empty()));

        app.back_to_tree();
        on_key(&mut app, KeyCode::Char('s'));
        assert!(matches!(app.mode, Mode::Grep(ref q) if q.is_empty()));
    }

    /// `q` must type into the query instead of quitting once searching.
    #[test]
    fn letters_type_into_the_query_rather_than_acting() {
        let mut app = app();
        on_key(&mut app, KeyCode::Char('/'));
        for c in ['q', '.', 's'] {
            assert_eq!(on_key(&mut app, KeyCode::Char(c)), None);
        }
        assert!(matches!(app.mode, Mode::Find(ref q) if q == "q.s"));

        on_key(&mut app, KeyCode::Backspace);
        assert!(matches!(app.mode, Mode::Find(ref q) if q == "q."));
    }

    #[test]
    fn esc_leaves_search_and_returns_to_the_tree() {
        let mut app = app();
        on_key(&mut app, KeyCode::Char('/'));
        on_key(&mut app, KeyCode::Char('l'));
        assert_eq!(on_key(&mut app, KeyCode::Esc), None);
        assert!(matches!(app.mode, Mode::Tree));
        assert!(app.hits.is_empty());
    }

    /// A find hit for a file resolves to its directory, which is what makes
    /// "Enter on a file" cd to the folder holding it.
    #[test]
    fn enter_on_a_file_hit_picks_its_directory() {
        let mut app = app();
        on_key(&mut app, KeyCode::Char('/'));
        for c in "lib.rs".chars() {
            on_key(&mut app, KeyCode::Char(c));
        }
        app.settle(); // the worker answers off the key loop now
        assert!(!app.hits.is_empty(), "lib.rs must be found");
        assert!(app.hits.iter().any(|p| p.is_file()));

        if let Some(Action::Pick(p)) = on_key(&mut app, KeyCode::Enter) {
            assert!(p.is_dir(), "{p:?}");
        } else {
            panic!("Enter must pick");
        };
    }

    /// The whole point of the worker: a keystroke returns before the search
    /// does, so the UI never stalls on a large tree.
    #[test]
    fn typing_returns_immediately_and_results_land_later() {
        let mut app = app();
        on_key(&mut app, KeyCode::Char('/'));
        for c in "lib".chars() {
            assert_eq!(on_key(&mut app, KeyCode::Char(c)), None);
        }
        assert!(app.search.pending(), "the search should still be running");
        assert!(app.hits.is_empty(), "hits must not be filled inline");

        app.settle();
        assert!(!app.search.pending());
        assert!(!app.hits.is_empty());
    }

    /// Enter pressed before the hits land must still pick the hit, not fall
    /// back to the root because the list happened to be empty at that instant.
    #[test]
    fn enter_before_the_results_land_still_picks_a_hit() {
        let mut app = app();
        on_key(&mut app, KeyCode::Char('/'));
        for c in "lib.rs".chars() {
            on_key(&mut app, KeyCode::Char(c));
        }
        assert!(app.search.pending(), "precondition: still searching");
        assert!(app.hits.is_empty(), "precondition: nothing arrived yet");

        if let Some(Action::Pick(p)) = on_key(&mut app, KeyCode::Enter) {
            assert_eq!(p, app.tree.root.join("src"), "picked {p:?}");
            assert_ne!(p, app.tree.root, "fell back to the root");
        } else {
            panic!("Enter must pick");
        };
    }

    /// Backspacing the query away clears the list without a round trip.
    #[test]
    fn clearing_the_query_empties_the_hits_at_once() {
        let mut app = app();
        on_key(&mut app, KeyCode::Char('/'));
        on_key(&mut app, KeyCode::Char('l'));
        app.settle();
        assert!(!app.hits.is_empty());

        on_key(&mut app, KeyCode::Backspace);
        assert!(matches!(app.mode, Mode::Find(ref q) if q.is_empty()));
        assert!(app.hits.is_empty(), "an empty query must show nothing");
        assert!(!app.search.pending(), "nothing to wait for");
    }

    #[test]
    fn dot_toggles_hidden_and_reloads() {
        let mut app = app();
        assert!(!app.tree.hidden);
        on_key(&mut app, KeyCode::Char('.'));
        assert!(app.tree.hidden);
        on_key(&mut app, KeyCode::Char('.'));
        assert!(!app.tree.hidden);
    }

    #[test]
    fn g_and_shift_g_jump_to_the_ends() {
        let mut app = app();
        on_key(&mut app, KeyCode::Char('G'));
        assert_eq!(app.sel, app.len() - 1);
        on_key(&mut app, KeyCode::Char('g'));
        assert_eq!(app.sel, 0);
    }
}
