//! # ui
//!
//! Terminal User Interface rendering logic for the OPC DA client.
//!
//! ## Overview
//!
//! This module contains all rendering functions to draw screens, dialogs, progress bars,
//! status logs, and input widgets onto the terminal frame. It maps the state in [`App`]
//! to visual elements using `ratatui`.

use crate::app::{App, CurrentScreen};
use opc_da_client::{OpcValueOptionExt, SystemTimeOptionExt};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap},
};

/// Renders the complete terminal user interface for the current application state.
///
/// Divides the available terminal frame into main display, message/status area,
/// and contextual keybinding help footer, routing screen-specific rendering based
/// on [`app.nav.current_screen`](CurrentScreen).
///
/// # Arguments
///
/// * `f` - Mutable terminal frame from Ratatui.
/// * `app` - Mutable reference to the application state.
pub fn render(f: &mut Frame, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(
            [
                Constraint::Min(0),
                Constraint::Length(3),
                Constraint::Length(1),
            ]
            .as_ref(),
        )
        .split(f.area());

    let main_area = chunks[0];
    let status_area = chunks[1];
    let help_area = chunks[2];

    match app.nav.current_screen {
        CurrentScreen::Home => render_home(f, app, main_area),
        CurrentScreen::ServerList => render_server_list(f, app, main_area),
        CurrentScreen::TagList => render_tag_list(f, app, main_area),
        CurrentScreen::TagValues => render_tag_values(f, app, main_area),
        CurrentScreen::WriteInput => {
            render_tag_values(f, app, main_area);
            render_write_input(f, app, main_area);
        }
        CurrentScreen::Loading => {
            render_loading_popup(f, app, main_area);
        }
        CurrentScreen::Exiting => {}
    }

    render_status_bar(f, app, status_area);
    render_help(f, app, help_area);
}

fn render_help(f: &mut Frame, app: &App, area: Rect) {
    let msg = match app.nav.current_screen {
        CurrentScreen::Home => "Enter: Connect | Esc: Quit | Type hostname",
        CurrentScreen::ServerList => {
            "↑/↓: Nav | PgDn/PgUp: Page | Enter: Tags | Esc: Back | q: Quit"
        }
        CurrentScreen::TagList => {
            if app.search.search_mode {
                "Type: Search | Tab: Next | Space: Select | Enter: Read | Esc: Cancel"
            } else {
                "↑/↓: Nav | PgDn/PgUp: Page | Space: Select | s: Search | Enter: Read | Esc: Back | q: Quit"
            }
        }
        CurrentScreen::TagValues => "↑/↓: Nav | PgDn/PgUp: Page | w: Write | Esc: Back | q: Quit",
        CurrentScreen::WriteInput => "Enter: Submit | Esc: Cancel | Type value",
        CurrentScreen::Loading => "Please wait... | Esc: Cancel",
        CurrentScreen::Exiting => "Exiting...",
    };

    let span = Span::styled(msg, Style::default().fg(Color::DarkGray));
    f.render_widget(Paragraph::new(span), area);
}

fn render_home(f: &mut Frame, app: &App, area: Rect) {
    let display_text = format!("> {input}_", input = app.nav.host_input);
    let input = Paragraph::new(display_text)
        .style(Style::default().fg(Color::Yellow))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Step 1: Connect to Host ")
                .border_style(Style::default().fg(Color::Cyan)),
        );

    // Create a centered layout
    let vertical_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(40),
            Constraint::Length(3),
            Constraint::Percentage(40),
        ])
        .split(area);

    let horizontal_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(20),
            Constraint::Percentage(60),
            Constraint::Percentage(20),
        ])
        .split(vertical_chunks[1]);

    f.render_widget(input, horizontal_chunks[1]);
}

fn render_server_list(f: &mut Frame, app: &mut App, area: Rect) {
    let items: Vec<ListItem> = app
        .view
        .servers
        .iter()
        .map(|s| ListItem::new(Line::from(Span::raw(s))))
        .collect();

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Step 2: Select OPC Server "),
        )
        .highlight_style(
            Style::default()
                .add_modifier(Modifier::BOLD)
                .bg(Color::Blue)
                .fg(Color::White),
        )
        .highlight_symbol(">> ");

    f.render_stateful_widget(list, area, &mut app.view.list_state);
}

fn render_tag_list(f: &mut Frame, app: &mut App, area: Rect) {
    let list_chunks = if app.search.search_mode {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Min(0)])
            .split(area)
    } else {
        Layout::default()
            .constraints([Constraint::Min(0)])
            .split(area)
    };

    if app.search.search_mode {
        let search_text = format!("Search: {query}_", query = app.search.search_query);
        let search_bar = Paragraph::new(search_text)
            .style(Style::default().fg(Color::Yellow))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Search Tags (Substring Match) "),
            );
        f.render_widget(search_bar, list_chunks[0]);
    }

    let items: Vec<ListItem> = app
        .view
        .tags
        .iter()
        .enumerate()
        .map(|(idx, t)| {
            let checkbox = if app.view.is_selected(t) {
                "[✓] "
            } else {
                "[ ] "
            };

            let is_match = app.search.search_mode && app.search.is_match(idx);
            let style = if is_match {
                Style::default().fg(Color::Yellow)
            } else {
                Style::default()
            };

            ListItem::new(Line::from(vec![
                Span::raw(checkbox),
                Span::styled(t, style),
            ]))
        })
        .collect();

    let title = if app.search.search_mode {
        format!(
            " Step 3: Browse Tags ({}/{} matches) ",
            app.search.search_matches.len(),
            app.view.tags.len()
        )
    } else {
        " Step 3: Browse Tags ".to_string()
    };

    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(title))
        .highlight_style(Style::default().bg(Color::Green).fg(Color::Black))
        .highlight_symbol(" * ");

    let list_area = if app.search.search_mode {
        list_chunks[1]
    } else {
        list_chunks[0]
    };
    f.render_stateful_widget(list, list_area, &mut app.view.list_state);
}

fn render_tag_values(f: &mut Frame, app: &mut App, area: Rect) {
    use ratatui::widgets::{Cell, Row, Table};

    let header = Row::new(["Tag ID", "Value", "Quality", "Timestamp"]).style(
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
    );

    let rows: Vec<Row> = app
        .view
        .tag_values
        .iter()
        .map(|tv| {
            Row::new([
                Cell::from(tv.tag_id.as_str()),
                Cell::from(tv.value().display().to_string()),
                Cell::from(tv.quality.to_string()),
                Cell::from(tv.timestamp.display().to_string()),
            ])
        })
        .collect();

    let widths = [
        Constraint::Percentage(45),
        Constraint::Percentage(15),
        Constraint::Percentage(10),
        Constraint::Percentage(30),
    ];

    let title = if app.view.tag_values.is_empty() {
        " Step 4: Tag Values ".to_string()
    } else {
        let total = app.view.tag_values.len();
        let error_count = app
            .view
            .tag_values
            .iter()
            .filter(|tv| tv.is_error())
            .count();
        let bad_quality_count = app
            .view
            .tag_values
            .iter()
            .filter(|tv| tv.is_bad() && !tv.is_error())
            .count();

        match (error_count > 0, bad_quality_count > 0) {
            (true, true) => format!(
                " Step 4: Tag Values ({total} items, ⚠ {error_count} errors, {bad_quality_count} bad quality) "
            ),
            (true, false) => {
                format!(" Step 4: Tag Values ({total} items, ⚠ {error_count} errors) ")
            }
            (false, true) => {
                format!(" Step 4: Tag Values ({total} items, ⚠ {bad_quality_count} bad quality) ")
            }
            (false, false) => format!(" Step 4: Tag Values ({total} items) "),
        }
    };

    let table = Table::new(rows, widths)
        .header(header)
        .block(Block::default().borders(Borders::ALL).title(title))
        .row_highlight_style(Style::default().bg(Color::Blue).fg(Color::White))
        .highlight_symbol(">> ");

    f.render_stateful_widget(table, area, &mut app.view.table_state);
}

fn render_status_bar(f: &mut Frame, app: &App, area: Rect) {
    let display_messages: Vec<Line> = app
        .view
        .messages
        .iter()
        .rev()
        .take(2)
        .rev()
        .map(|m| {
            Line::from(vec![
                Span::styled("- ", Style::default().fg(Color::DarkGray)),
                Span::raw(m),
            ])
        })
        .collect();

    let paragraph = Paragraph::new(display_messages)
        .block(Block::default().borders(Borders::ALL).title(" Status Log "))
        .wrap(Wrap { trim: true });
    f.render_widget(paragraph, area);
}

fn render_write_input(f: &mut Frame, app: &App, area: Rect) {
    let tag_id = app.dialog.write_tag_id.as_deref().unwrap_or("Unknown");
    let display_text = format!(
        "Tag: {tag_id}\nValue: {input}_",
        input = app.dialog.write_value_input
    );

    let popup_block = Block::default()
        .title(" Write Tag Value ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow));

    let input = Paragraph::new(display_text)
        .block(popup_block)
        .wrap(Wrap { trim: true });

    let area = centered_rect(60, 30, area);
    f.render_widget(Clear, area);
    f.render_widget(input, area);
}

fn render_loading_popup(f: &mut Frame, app: &App, area: Rect) {
    let progress = app.loading_progress();
    let msg = if progress > 0 {
        format!("Browsing OPC tags... ({progress} found so far)\nPress Esc to cancel")
    } else {
        "Communicating with OPC Server...\nPress Esc to cancel".to_string()
    };

    let block = Block::default()
        .title(" Loading ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow));

    let area = centered_rect(60, 20, area);
    f.render_widget(Clear, area);
    f.render_widget(Paragraph::new(msg).block(block), area);
}

/// Helper function to create a centered rect using up certain percentage of the available rect `r`.
fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

#[cfg(test)]
mod tests {
    use super::*;
    use opc_da_client::{MockOpcProvider, OpcQuality, OpcValue, TagValue, TagValues};
    use ratatui::{Terminal, backend::TestBackend};
    use std::sync::Arc;

    fn create_test_app() -> App {
        let mock = MockOpcProvider::new();
        App::new(Arc::new(mock))
    }

    #[test]
    fn test_headless_render_home() {
        let mut app = create_test_app();
        app.nav.current_screen = CurrentScreen::Home;
        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal.draw(|f| render(f, &mut app)).unwrap();
        let buffer = terminal.backend().buffer();
        assert!(!buffer.content().is_empty());
    }

    #[test]
    fn test_headless_render_server_list() {
        let mut app = create_test_app();
        app.nav.current_screen = CurrentScreen::ServerList;
        app.view.servers = vec!["Server.A".into(), "Server.B".into()];
        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal.draw(|f| render(f, &mut app)).unwrap();
        let buffer = terminal.backend().buffer();
        assert!(!buffer.content().is_empty());
    }

    #[test]
    fn test_headless_render_tag_list() {
        let mut app = create_test_app();
        app.nav.current_screen = CurrentScreen::TagList;
        app.view.tags = vec!["Tag.1".into(), "Tag.2".into()];
        app.view.toggle_selection("Tag.1");
        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal.draw(|f| render(f, &mut app)).unwrap();
        let buffer = terminal.backend().buffer();
        assert!(!buffer.content().is_empty());
    }

    #[test]
    fn test_headless_render_tag_values() {
        let mut app = create_test_app();
        app.nav.current_screen = CurrentScreen::TagValues;
        app.view.tag_values = TagValues::from(vec![
            TagValue::new(
                "Sensor.Temp",
                Some(OpcValue::Float(98.6)),
                OpcQuality::GOOD,
                None,
            ),
            TagValue::with_error(
                "Sensor.Fail",
                OpcQuality::BAD_COMM_FAILURE,
                opc_da_client::OpcError::Connection("Lost".into()),
            ),
        ]);
        let backend = TestBackend::new(120, 30);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal.draw(|f| render(f, &mut app)).unwrap();
        let buffer = terminal.backend().buffer();
        assert!(!buffer.content().is_empty());
    }

    #[test]
    fn test_headless_render_write_input_and_loading() {
        let mut app = create_test_app();
        app.nav.current_screen = CurrentScreen::WriteInput;
        app.dialog.write_tag_id = Some("Sensor.Setpoint".into());
        app.dialog.write_value_input = "100.5".into();
        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal.draw(|f| render(f, &mut app)).unwrap();

        app.nav.current_screen = CurrentScreen::Loading;
        terminal.draw(|f| render(f, &mut app)).unwrap();

        app.nav.current_screen = CurrentScreen::Exiting;
        terminal.draw(|f| render(f, &mut app)).unwrap();
    }
}
