//! Rendering. A yazi-style 3-pane browser (parent / current / preview) plus a
//! one-line status bar in tree mode, one flat list in search mode. Touches no
//! business state — `app.list` is the one exception, a scroll offset that is
//! purely a view concern but has to survive between frames.
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph};

use crate::app::{App, Mode};

const HELP: &str = "j/k move  l in  h out  / find  s grep  . hidden  Enter cd  q quit";

/// Keep this many rows visible past the cursor (vim's `scrolloff`), so
/// scrolling reveals what's coming instead of landing the selection right on
/// the edge of the pane.
const SCROLL_PADDING: usize = 4;

pub(crate) fn draw(f: &mut Frame, app: &mut App) {
    let [body, bar] = Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).areas(f.area());

    match app.mode {
        Mode::Tree => draw_panes(f, app, body),
        _ => draw_hits(f, app, body),
    }

    f.render_widget(
        Paragraph::new(status(app)).style(Style::new().fg(Color::DarkGray)),
        bar,
    );
}

/// The hit list is the one the user scrolls, so it gets `app.list`, kept
/// across frames so its offset persists (see the field doc on `App::list`).
fn draw_hits(f: &mut Frame, app: &mut App, area: Rect) {
    let items: Vec<ListItem> = app
        .hits
        .iter()
        .map(|p| ListItem::new(cdt_search::label(&app.tree.root, p)))
        .collect();
    app.list.select(Some(app.sel));
    f.render_stateful_widget(
        List::new(items)
            .block(
                Block::new()
                    .borders(Borders::ALL)
                    .title(format!(" {} ", app.tree.display_root())),
            )
            .highlight_style(Style::new().reversed())
            .scroll_padding(SCROLL_PADDING),
        area,
        &mut app.list,
    );
}

/// Three columns: the parent directory (for context), the one being browsed
/// (interactive), and a preview of whatever is selected in it.
fn draw_panes(f: &mut Frame, app: &mut App, area: Rect) {
    let [left, mid, right] = Layout::horizontal([
        Constraint::Ratio(1, 4),
        Constraint::Ratio(1, 2),
        Constraint::Ratio(1, 4),
    ])
    .areas(area);

    match cdt_tree::up_listing(&app.tree.root, app.tree.hidden) {
        Some(parent) => {
            let sel = parent.iter().position(|e| e.path == app.tree.root);
            pane(f, left, &parent, &mut ListState::default().with_selected(sel), " .. ", false);
        }
        None => f.render_widget(Block::new().borders(Borders::ALL).title(" .. "), left),
    }

    // The middle pane is the one j/k scrolls, so it keeps its offset in
    // `app.list` instead of a fresh, always-zeroed `ListState`.
    app.list.select(Some(app.sel));
    pane(
        f,
        mid,
        &app.tree.entries,
        &mut app.list,
        &format!(" {} ", app.tree.display_root()),
        true,
    );

    match app.selected().filter(|p| p.is_dir()) {
        Some(p) => pane(
            f,
            right,
            &cdt_tree::children(p, app.tree.hidden),
            &mut ListState::default(),
            " preview ",
            false,
        ),
        None => f.render_widget(Block::new().borders(Borders::ALL).title(" preview "), right),
    }
}

fn pane(
    f: &mut Frame,
    area: Rect,
    entries: &[cdt_tree::Entry],
    state: &mut ListState,
    title: &str,
    active: bool,
) {
    let items: Vec<ListItem> = entries.iter().map(entry_row).collect();
    let highlight = if active {
        Style::new().reversed()
    } else {
        Style::new().fg(Color::DarkGray).reversed()
    };
    let padding = if active { SCROLL_PADDING } else { 0 };
    f.render_stateful_widget(
        List::new(items)
            .block(Block::new().borders(Borders::ALL).title(title.to_string()))
            .highlight_style(highlight)
            .scroll_padding(padding),
        area,
        state,
    );
}

fn entry_row(e: &cdt_tree::Entry) -> ListItem<'static> {
    let style = if e.is_dir {
        Style::new().fg(Color::Cyan).bold()
    } else {
        Style::new()
    };
    let suffix = if e.is_dir { "/" } else { "" };
    ListItem::new(format!("{}{suffix}", e.name())).style(style)
}

fn status(app: &App) -> String {
    match &app.mode {
        _ if !app.msg.is_empty() => app.msg.clone(),
        Mode::Find(q) => format!("find: {q}_  ({})", count(app, "hits")),
        Mode::Grep(q) => format!("grep: {q}_  ({})", count(app, "files")),
        Mode::Tree => HELP.into(),
    }
}

/// Hit count, flagged when the backend stopped at its cap so a truncated list
/// does not read as the whole answer, and while a search is still running.
fn count(app: &App, noun: &str) -> String {
    let n = app.hits.len();
    let more = if n >= cdt_search::MAX_HITS { "+" } else { "" };
    if app.search.pending() {
        return if n == 0 {
            "searching…".into()
        } else {
            format!("{n}{more} {noun}, searching…").to_string()
        };
    }
    format!("{n}{more} {noun}").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn status_shows_help_in_tree_mode_and_the_query_while_searching() {
        let mut app = App::new(PathBuf::from(env!("CARGO_MANIFEST_DIR")));
        assert_eq!(status(&app), HELP);

        app.mode = Mode::Find("lib".into());
        assert!(status(&app).starts_with("find: lib_"), "{}", status(&app));

        // A message outranks the query, so an rg failure stays visible.
        app.msg = "rg unavailable".into();
        assert_eq!(status(&app), "rg unavailable");
    }

    /// A list capped at MAX_HITS must not read as the complete answer.
    #[test]
    fn a_capped_hit_list_is_flagged() {
        let mut app = App::new(PathBuf::from(env!("CARGO_MANIFEST_DIR")));
        app.mode = Mode::Find("e".into());

        app.hits = vec![PathBuf::from("p"); cdt_search::MAX_HITS];
        let s = status(&app);
        assert!(
            s.contains(&format!("{}+ hits", cdt_search::MAX_HITS)),
            "{s}"
        );

        app.hits.pop();
        assert!(!status(&app).contains('+'), "{}", status(&app));
    }

    #[test]
    fn an_in_flight_search_says_it_is_still_running() {
        let mut app = App::new(PathBuf::from(env!("CARGO_MANIFEST_DIR")));
        app.mode = Mode::Find("lib".into());
        app.request_search("lib");
        assert!(status(&app).contains("searching"), "{}", status(&app));

        app.settle();
        assert!(!status(&app).contains("searching"), "{}", status(&app));
        assert!(!app.hits.is_empty());
    }
}
