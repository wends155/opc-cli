use crate::app::{App, CurrentScreen};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap},
};
use std::sync::atomic::Ordering;

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

    match app.current_screen {
        CurrentScreen::Home => render_home(f, app, main_area),
        CurrentScreen::ServerList => render_server_list(f, app, main_area),
        CurrentScreen::TagList => render_tag_list(f, app, main_area),
        CurrentScreen::TagValues => render_tag_values(f, app, main_area),
        CurrentScreen::Loading => {
            // Render the last screen in the background if it makes sense,
            // but for now let's just show the popup.
            render_loading_popup(f, app, main_area);
        }
        CurrentScreen::Exiting => {}
    }

    render_status_bar(f, app, status_area);
    render_help(f, app, help_area);
}

fn render_help(f: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let msg = match app.current_screen {
        CurrentScreen::Home => "Enter: Connect | Esc: Quit | Type hostname",
        CurrentScreen::ServerList => {
            "↑/↓: Nav | PgDn/PgUp: Page | Enter: Tags | Esc: Back | q: Quit"
        }
        CurrentScreen::TagList => {
            if app.search_mode {
                "Type: Search | Tab: Next | Space: Select | Enter: Read | Esc: Cancel"
            } else {
                "↑/↓: Nav | PgDn/PgUp: Page | Space: Select | s: Search | Enter: Read | Esc: Back | q: Quit"
            }
        }
        CurrentScreen::TagValues => "↑/↓: Nav | PgDn/PgUp: Page | Esc: Back | q: Quit",
        CurrentScreen::Loading => "Please wait...",
        CurrentScreen::Exiting => "Exiting...",
    };

    let span = Span::styled(msg, Style::default().fg(Color::DarkGray));
    f.render_widget(Paragraph::new(span), area);
}

fn render_home(f: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let display_text = format!("> {}_", app.host_input);
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

fn render_server_list(f: &mut Frame, app: &mut App, area: ratatui::layout::Rect) {
    let items: Vec<ListItem> = app
        .servers
        .iter()
        .map(|s| ListItem::new(Line::from(vec![Span::raw(s)])))
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

    f.render_stateful_widget(list, area, &mut app.list_state);
}

fn render_tag_list(f: &mut Frame, app: &mut App, area: ratatui::layout::Rect) {
    let list_chunks = if app.search_mode {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Min(0)])
            .split(area)
    } else {
        Layout::default()
            .constraints([Constraint::Min(0)])
            .split(area)
    };

    if app.search_mode {
        let search_text = format!("Search: {}_", app.search_query);
        let search_bar = Paragraph::new(search_text)
            .style(Style::default().fg(Color::Yellow))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Search Tags (Substring Match) ")
                    .border_style(Style::default().fg(Color::Yellow)),
            );
        f.render_widget(search_bar, list_chunks[0]);
    }

    let items: Vec<ListItem> = app
        .tags
        .iter()
        .enumerate()
        .map(|(idx, t)| {
            let checkbox = if app.selected_tags.get(idx).copied().unwrap_or(false) {
                "[✓] "
            } else {
                "[ ] "
            };

            let is_match = app.search_mode && app.search_matches.contains(&idx);
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

    let title = if app.search_mode {
        format!(
            " Step 3: Browse Tags ({}/{} matches) ",
            app.search_matches.len(),
            app.tags.len()
        )
    } else {
        " Step 3: Browse Tags ".to_string()
    };

    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(title))
        .highlight_style(Style::default().bg(Color::Green).fg(Color::Black))
        .highlight_symbol(" * ");

    let list_area = if app.search_mode {
        list_chunks[1]
    } else {
        list_chunks[0]
    };
    f.render_stateful_widget(list, list_area, &mut app.list_state);
}

fn render_tag_values(f: &mut Frame, app: &mut App, area: ratatui::layout::Rect) {
    use ratatui::widgets::{Row, Table};

    let header = Row::new(vec!["Tag ID", "Value", "Quality", "Timestamp"]).style(
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
    );

    let rows: Vec<Row> = app
        .tag_values
        .iter()
        .map(|tv| {
            Row::new(vec![
                tv.tag_id.clone(),
                tv.value.clone(),
                tv.quality.clone(),
                tv.timestamp.clone(),
            ])
        })
        .collect();

    let widths = [
        Constraint::Percentage(45),
        Constraint::Percentage(15),
        Constraint::Percentage(10),
        Constraint::Percentage(30),
    ];

    let table = Table::new(rows, widths)
        .header(header)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Step 4: Tag Values "),
        )
        //.highlight_style(Style::default().bg(Color::Blue).fg(Color::White)) // Deprecated
        .row_highlight_style(Style::default().bg(Color::Blue).fg(Color::White))
        .highlight_symbol(">> ");

    f.render_stateful_widget(table, area, &mut app.table_state);
}
fn render_status_bar(f: &mut Frame, app: &App, area: Rect) {
    let display_messages: Vec<Line> = app
        .messages
        .last()
        .map(|m| {
            vec![Line::from(vec![
                Span::styled("- ", Style::default().fg(Color::DarkGray)),
                Span::raw(m),
            ])]
        })
        .unwrap_or_default();

    let paragraph = Paragraph::new(display_messages)
        .block(Block::default().borders(Borders::ALL).title(" Status Log "))
        .wrap(Wrap { trim: true });
    f.render_widget(paragraph, area);
}

fn render_loading_popup(f: &mut Frame, app: &App, area: Rect) {
    let progress = app.browse_progress.load(Ordering::Relaxed);
    let msg = if progress > 0 {
        format!("Browsing OPC tags... ({} found so far)", progress)
    } else {
        "Communicating with OPC Server...".to_string()
    };

    let block = Block::default()
        .title(" Loading ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow));

    let area = centered_rect(60, 20, area);
    f.render_widget(Clear, area); // This clears the background
    f.render_widget(Paragraph::new(msg).block(block), area);
}

/// helper function to create a centered rect using up certain percentage of the available rect `r`
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
