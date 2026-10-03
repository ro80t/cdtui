//! Rendering. One list plus a one-line status bar; nothing here mutates state.
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph};

use crate::app::{App, Mode};

const HELP: &str = "j/k move  l expand  h up  / find  s grep  . hidden  Enter cd  q quit";

pub(crate) fn draw(f: &mut Frame, app: &App) {
    let [body, bar] = Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).areas(f.area());

    let items: Vec<ListItem> = match app.mode {
        Mode::Tree => app.tree.entries.iter().map(tree_row).collect(),
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

    f.render_widget(
        Paragraph::new(status(app)).style(Style::new().fg(Color::DarkGray)),
        bar,
    );
}

fn tree_row(e: &cdt_tree::Entry) -> ListItem<'static> {
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
}

fn status(app: &App) -> String {
    match &app.mode {
        _ if !app.msg.is_empty() => app.msg.clone(),
        Mode::Find(q) => format!("find: {q}_  ({} hits)", app.hits.len()),
        Mode::Grep(q) => format!("grep: {q}_  ({} files)", app.hits.len()),
        Mode::Tree => HELP.into(),
    }
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
}
