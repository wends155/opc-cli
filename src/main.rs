mod app;
mod opc_impl;
mod traits;
mod ui;

use crate::app::{App, CurrentScreen};
use crate::opc_impl::OpcDaWrapper;
use anyhow::{Context, Result};
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::{io, sync::Arc, time::Duration};
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    let file_appender = tracing_appender_localtime::rolling::daily("logs", "opc-cli.log");
    let (non_blocking, _guard) = tracing_appender_localtime::non_blocking(file_appender);

    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("debug"));

    tracing_subscriber::registry()
        .with(fmt::layer().with_writer(non_blocking).with_filter(filter))
        .init();

    tracing::info!("Starting OPC CLI");

    // Initialize COM (MTA) for the main thread
    unsafe {
        windows::Win32::System::Com::CoInitializeEx(
            None,
            windows::Win32::System::Com::COINIT_MULTITHREADED,
        )
        .ok()
        .context("Failed to initialize COM MTA")?;
    }

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create app and run it
    let opc_wrapper = Arc::new(OpcDaWrapper::new());
    let mut app = App::new(opc_wrapper);
    let res = run_app(&mut terminal, &mut app).await;

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    // Uninitialize COM
    unsafe {
        windows::Win32::System::Com::CoUninitialize();
    }

    if let Err(err) = res {
        tracing::error!(error = ?err, "Application error");
    }

    Ok(())
}

async fn run_app<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    app: &mut App,
) -> io::Result<()> {
    // Clear any leftover events (like the Enter key used to start the app)
    while event::poll(Duration::from_millis(0))? {
        let _ = event::read()?;
    }

    loop {
        // Poll background task progress
        app.poll_fetch_result();
        app.poll_browse_result();
        app.poll_read_result();
        app.maybe_auto_refresh();

        terminal.draw(|f| ui::render(f, app))?;

        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                handle_key_event(app, key);
            }
        }

        if let CurrentScreen::Exiting = app.current_screen {
            return Ok(());
        }
    }
}

fn handle_key_event(app: &mut App, key: event::KeyEvent) {
    if key.kind != event::KeyEventKind::Press {
        return;
    }

    match app.current_screen {
        CurrentScreen::Home => match key.code {
            KeyCode::Enter => {
                app.start_fetch_servers();
            }
            KeyCode::Char(c) => {
                app.host_input.push(c);
            }
            KeyCode::Backspace => {
                app.host_input.pop();
            }
            KeyCode::Esc => {
                app.current_screen = CurrentScreen::Exiting;
            }
            _ => {}
        },
        CurrentScreen::ServerList => match key.code {
            KeyCode::Esc => app.go_back(),
            KeyCode::PageDown => app.page_down(),
            KeyCode::PageUp => app.page_up(),
            KeyCode::Down => app.select_next(),
            KeyCode::Up => app.select_prev(),
            KeyCode::Enter => {
                app.start_browse_tags();
            }
            KeyCode::Char('q') | KeyCode::Char('Q') => {
                app.current_screen = CurrentScreen::Exiting;
            }
            _ => {}
        },
        CurrentScreen::TagList => {
            if app.search_mode {
                match key.code {
                    KeyCode::Esc => app.exit_search_mode(),
                    KeyCode::Backspace => app.search_backspace(),
                    KeyCode::Tab => app.next_search_match(),
                    KeyCode::BackTab => app.prev_search_match(),
                    KeyCode::Char(' ') => app.toggle_tag_selection(),
                    KeyCode::Enter => {
                        app.exit_search_mode();
                        app.start_read_values();
                    }
                    KeyCode::Char(c) => app.update_search_query(c),
                    _ => {}
                }
            } else {
                match key.code {
                    KeyCode::Esc => app.go_back(),
                    KeyCode::PageDown => app.page_down(),
                    KeyCode::PageUp => app.page_up(),
                    KeyCode::Down => app.select_next(),
                    KeyCode::Up => app.select_prev(),
                    KeyCode::Char(' ') => app.toggle_tag_selection(),
                    KeyCode::Char('s') | KeyCode::Char('S') => app.enter_search_mode(),
                    KeyCode::Enter => app.start_read_values(),
                    KeyCode::Char('q') | KeyCode::Char('Q') => {
                        app.current_screen = CurrentScreen::Exiting;
                    }
                    _ => {}
                }
            }
        }
        CurrentScreen::TagValues => match key.code {
            KeyCode::Esc => app.go_back(),
            KeyCode::PageDown => app.page_down(),
            KeyCode::PageUp => app.page_up(),
            KeyCode::Down => app.select_next(),
            KeyCode::Up => app.select_prev(),
            KeyCode::Char('q') | KeyCode::Char('Q') => {
                app.current_screen = CurrentScreen::Exiting;
            }
            _ => {}
        },
        CurrentScreen::Loading => {
            if key.code == KeyCode::Esc {
                app.go_back();
            }
        }
        CurrentScreen::Exiting => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::MockOpcProvider;
    use crossterm::event::{KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};

    #[test]
    fn test_handle_key_event_press_release() {
        let mock = MockOpcProvider::new();
        let mut app = App::new(Arc::new(mock));

        // 1. Simulate Press 'a'
        let press_a = KeyEvent {
            code: KeyCode::Char('a'),
            modifiers: KeyModifiers::empty(),
            kind: KeyEventKind::Press,
            state: KeyEventState::empty(),
        };
        handle_key_event(&mut app, press_a);
        assert_eq!(app.host_input, "localhosta");

        // 2. Simulate Release 'b' (should be ignored)
        let release_b = KeyEvent {
            code: KeyCode::Char('b'),
            modifiers: KeyModifiers::empty(),
            kind: KeyEventKind::Release,
            state: KeyEventState::empty(),
        };
        handle_key_event(&mut app, release_b);
        assert_eq!(app.host_input, "localhosta"); // Still 'a', 'b' ignored
    }

    #[test]
    fn test_quit_logic_on_all_screens() {
        let mock = MockOpcProvider::new();
        let mut app = App::new(Arc::new(mock));

        let quit_q = KeyEvent {
            code: KeyCode::Char('q'),
            modifiers: KeyModifiers::empty(),
            kind: KeyEventKind::Press,
            state: KeyEventState::empty(),
        };

        let esc = KeyEvent {
            code: KeyCode::Esc,
            modifiers: KeyModifiers::empty(),
            kind: KeyEventKind::Press,
            state: KeyEventState::empty(),
        };

        // 1. Home Screen: Esc quits, 'q' does NOT quit (it's input)
        app.current_screen = CurrentScreen::Home;
        handle_key_event(&mut app, quit_q);
        assert_eq!(app.current_screen, CurrentScreen::Home);
        assert!(app.host_input.ends_with('q'));

        handle_key_event(&mut app, esc);
        assert_eq!(app.current_screen, CurrentScreen::Exiting);

        // 2. Server List: 'q' quits
        app.current_screen = CurrentScreen::ServerList;
        handle_key_event(&mut app, quit_q);
        assert_eq!(app.current_screen, CurrentScreen::Exiting);

        // 3. Tag List: 'q' quits
        app.current_screen = CurrentScreen::TagList;
        handle_key_event(&mut app, quit_q);
        assert_eq!(app.current_screen, CurrentScreen::Exiting);
    }
}
