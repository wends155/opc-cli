//! # app
//!
//! Main application state manager and event handler for the OPC DA client TUI.
//!
//! ## Overview
//!
//! This module defines the state structures ([`App`]) and screen state representations
//! ([`CurrentScreen`]) driving the TUI layout, handling user inputs, managing the list selection
//! states, and communicating asynchronously with the background OPC DA client provider.

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind};
use opc_da_client::{
    OpcError, OpcProvider, OpcValue, TagBatch, TagCollector, TagValues, WriteResult,
};
use ratatui::widgets::{ListState, TableState};
use std::collections::{HashSet, VecDeque};
use std::sync::Arc;
use tokio::sync::oneshot;

/// Action signal returned by [`App::handle_key`] to the event loop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppAction {
    /// Continue running the application event loop.
    Continue,
    /// Terminate the application event loop and exit cleanly.
    Quit,
}

/// Default timeout for OPC operations (server listing and tag browsing).
const OPC_TIMEOUT_SECS: u64 = 300;

/// Maximum tags to retrieve when browsing an OPC server namespace.
const MAX_BROWSE_TAGS: usize = 10000;

/// Screens navigable within the TUI.
#[derive(Debug, Default, PartialEq, Eq, Clone, Copy)]
pub enum CurrentScreen {
    #[default]
    Home,
    Loading,
    ServerList,
    TagList,
    TagValues,
    WriteInput,
    Exiting,
}

impl std::fmt::Display for CurrentScreen {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Home => write!(f, "Home"),
            Self::Loading => write!(f, "Loading"),
            Self::ServerList => write!(f, "ServerList"),
            Self::TagList => write!(f, "TagList"),
            Self::TagValues => write!(f, "TagValues"),
            Self::WriteInput => write!(f, "WriteInput"),
            Self::Exiting => write!(f, "Exiting"),
        }
    }
}

/// Manages screen hierarchy, host target, and browsed server context.
#[derive(Debug, Clone)]
pub struct NavigationState {
    pub current_screen: CurrentScreen,
    pub previous_screen: CurrentScreen,
    pub host_input: String,
    pub browsed_server: Option<String>,
}

impl Default for NavigationState {
    fn default() -> Self {
        Self {
            current_screen: CurrentScreen::Home,
            previous_screen: CurrentScreen::Home,
            host_input: "localhost".into(),
            browsed_server: None,
        }
    }
}

/// Manages modal dialog input state (e.g. write value modal).
#[derive(Debug, Clone, Default)]
pub struct DialogState {
    pub write_tag_id: Option<String>,
    pub write_value_input: String,
}

impl DialogState {
    /// Clears the active write tag and input buffer.
    pub fn clear(&mut self) {
        self.write_tag_id = None;
        self.write_value_input.clear();
    }
}

/// Manages automatic periodic tag reading state and interval tracking.
#[derive(Debug, Clone, Default)]
pub struct AutoRefresher {
    pub server: Option<String>,
    pub tag_ids: Vec<String>,
    pub last_read_time: Option<std::time::Instant>,
}

impl AutoRefresher {
    /// Clears the auto-refresh server target, tag list, and timestamp.
    pub fn clear(&mut self) {
        self.server = None;
        self.tag_ids.clear();
        self.last_read_time = None;
    }
}

/// Manages background asynchronous tasks, cooperative collectors, and abort handles.
#[derive(Default)]
pub struct TaskManager {
    pub browse_collector: TagCollector,
    pub browse_result_rx: Option<oneshot::Receiver<Result<Vec<String>, OpcError>>>,
    pub fetch_result_rx: Option<oneshot::Receiver<Result<Vec<String>, OpcError>>>,
    pub read_result_rx: Option<oneshot::Receiver<Result<TagValues, OpcError>>>,
    pub write_result_rx: Option<oneshot::Receiver<Result<WriteResult, OpcError>>>,
    pub active_abort: Option<tokio::task::AbortHandle>,
}

impl TaskManager {
    /// Cancels all ongoing background tasks, signals cooperative collector cancellation,
    /// aborts the active Tokio task handle, and clears all oneshot receivers.
    pub fn cancel_all(&mut self) {
        self.browse_collector.cancel();
        if let Some(handle) = self.active_abort.take() {
            handle.abort();
        }
        self.browse_result_rx = None;
        self.fetch_result_rx = None;
        self.read_result_rx = None;
        self.write_result_rx = None;
    }

    /// Sets the active task abort handle, aborting any prior active task.
    pub fn set_active_task(&mut self, handle: tokio::task::AbortHandle) {
        if let Some(prev) = self.active_abort.replace(handle) {
            prev.abort();
        }
    }

    /// Clears the active task handle.
    #[allow(dead_code)]
    pub fn clear_active_task(&mut self) {
        self.active_abort = None;
    }
}

/// Encapsulates tag search state, pre-computed lowercase cache, and $O(1)$ match lookup mask.
#[derive(Debug, Default)]
pub struct SearchEngine {
    pub search_mode: bool,
    pub search_query: String,
    pub search_matches: Vec<usize>,
    pub search_match_mask: Vec<bool>,
    pub search_match_index: usize,
    tags_lowercase: Vec<String>,
}

impl SearchEngine {
    /// Updates the cached lowercase tags and resizes/recomputes match buffers.
    pub fn update_tags(&mut self, tags: &[String]) {
        self.tags_lowercase = tags.iter().map(|t| t.to_lowercase()).collect();
        self.search_match_mask = vec![false; tags.len()];
        if self.search_mode {
            self.recompute_matches();
        } else {
            self.search_matches.clear();
            self.search_match_index = 0;
        }
    }

    /// Enter search mode, clearing query and previous match state.
    pub fn enter(&mut self) {
        self.search_mode = true;
        self.search_query.clear();
        self.search_matches.clear();
        self.search_match_mask.fill(false);
        self.search_match_index = 0;
    }

    /// Exit search mode, keeping cursor position.
    pub fn exit(&mut self) {
        self.search_mode = false;
    }

    /// Updates the search query with an appended character and recomputes matches.
    pub fn push_char(&mut self, c: char) {
        self.search_query.push(c);
        self.recompute_matches();
    }

    /// Deletes the last character from the search query and recomputes matches.
    pub fn backspace(&mut self) {
        self.search_query.pop();
        self.recompute_matches();
    }

    /// Returns whether the item at `idx` matches the current search query ($O(1)$).
    #[inline]
    pub fn is_match(&self, idx: usize) -> bool {
        self.search_match_mask.get(idx).copied().unwrap_or(false)
    }

    /// Recomputes search matches against cached lowercase tags without allocating strings.
    pub fn recompute_matches(&mut self) {
        self.search_matches.clear();
        self.search_match_mask.fill(false);

        if self.search_query.is_empty() {
            self.search_match_index = 0;
            return;
        }

        let query_lower = self.search_query.to_lowercase();
        for (idx, tag_lower) in self.tags_lowercase.iter().enumerate() {
            if tag_lower.contains(&query_lower) {
                self.search_matches.push(idx);
                if idx < self.search_match_mask.len() {
                    self.search_match_mask[idx] = true;
                }
            }
        }
        self.search_match_index = 0;
    }

    /// Advances to the next search match and returns its tag index if available.
    pub fn next_match(&mut self) -> Option<usize> {
        if self.search_matches.is_empty() {
            return None;
        }
        self.search_match_index = (self.search_match_index + 1) % self.search_matches.len();
        self.search_matches.get(self.search_match_index).copied()
    }

    /// Moves to the previous search match and returns its tag index if available.
    pub fn prev_match(&mut self) -> Option<usize> {
        if self.search_matches.is_empty() {
            return None;
        }
        if self.search_match_index == 0 {
            self.search_match_index = self.search_matches.len() - 1;
        } else {
            self.search_match_index -= 1;
        }
        self.search_matches.get(self.search_match_index).copied()
    }

    /// Returns the first match index if any.
    pub fn first_match(&self) -> Option<usize> {
        self.search_matches.first().copied()
    }
}

/// Manages displayed servers, tags, selection state, table state, and the status message ring buffer.
pub struct ViewState {
    pub servers: Vec<String>,
    pub tags: Vec<String>,
    pub selected_tags: HashSet<String>,
    pub tag_values: TagValues,
    pub selected_index: Option<usize>,
    pub list_state: ListState,
    pub table_state: TableState,
    pub messages: VecDeque<String>,
}

impl Default for ViewState {
    fn default() -> Self {
        Self {
            servers: Vec::new(),
            tags: Vec::new(),
            selected_tags: HashSet::new(),
            tag_values: TagValues::default(),
            selected_index: None,
            list_state: ListState::default(),
            table_state: TableState::default(),
            messages: VecDeque::with_capacity(10),
        }
    }
}

impl ViewState {
    /// Appends a message to the status message ring buffer (bounded to 10 entries).
    pub fn add_message(&mut self, message: String) {
        if self.messages.len() >= 10 {
            self.messages.pop_front();
        }
        self.messages.push_back(message);
    }

    /// Checks if a tag is currently selected.
    #[inline]
    pub fn is_selected(&self, tag_id: &str) -> bool {
        self.selected_tags.contains(tag_id)
    }

    /// Toggles selection of a tag.
    pub fn toggle_selection(&mut self, tag_id: &str) -> bool {
        if self.selected_tags.contains(tag_id) {
            self.selected_tags.remove(tag_id);
            false
        } else {
            self.selected_tags.insert(tag_id.to_string());
            true
        }
    }

    /// Selects all currently loaded tags.
    #[allow(dead_code)]
    pub fn select_all(&mut self) {
        for tag in &self.tags {
            self.selected_tags.insert(tag.clone());
        }
    }

    /// Clears all tag selections.
    pub fn clear_selection(&mut self) {
        self.selected_tags.clear();
    }
}

/// Helper representing the outcome of polling a oneshot receiver.
enum PollOutcome<T> {
    Ready(Result<T, OpcError>),
    Closed,
    Pending,
}

/// Helper function to poll a oneshot receiver and cleanly update its containing Option.
fn poll_channel<T>(rx_slot: &mut Option<oneshot::Receiver<Result<T, OpcError>>>) -> PollOutcome<T> {
    let Some(rx) = rx_slot.as_mut() else {
        return PollOutcome::Pending;
    };
    match rx.try_recv() {
        Ok(res) => {
            *rx_slot = None;
            PollOutcome::Ready(res)
        }
        Err(oneshot::error::TryRecvError::Closed) => {
            *rx_slot = None;
            PollOutcome::Closed
        }
        Err(oneshot::error::TryRecvError::Empty) => PollOutcome::Pending,
    }
}

/// Main application state for the OPC DA Client TUI.
///
/// Composes [`NavigationState`], [`DialogState`], [`AutoRefresher`], [`TaskManager`], [`SearchEngine`], and [`ViewState`].
pub struct App {
    pub nav: NavigationState,
    pub dialog: DialogState,
    pub refresher: AutoRefresher,
    pub tasks: TaskManager,
    pub search: SearchEngine,
    pub view: ViewState,
    pub opc_provider: Arc<dyn OpcProvider>,
}

impl App {
    /// Logs a screen transition event for diagnostics and field support.
    pub fn log_transition(&mut self, to: CurrentScreen, trigger: &str) {
        let from = self.nav.current_screen;
        tracing::info!(
            from = %from,
            to = %to,
            trigger = %trigger,
            "screen_transition"
        );
        self.nav.previous_screen = from;
        self.nav.current_screen = to;
    }

    /// Create a new `App` instance with the given OPC provider.
    pub fn new(opc_provider: Arc<dyn OpcProvider>) -> Self {
        Self {
            nav: NavigationState::default(),
            dialog: DialogState::default(),
            refresher: AutoRefresher::default(),
            tasks: TaskManager::default(),
            search: SearchEngine::default(),
            view: ViewState::default(),
            opc_provider,
        }
    }

    /// Appends a message to the status message ring buffer (bounded to 10 entries).
    pub fn add_message(&mut self, message: String) {
        self.view.add_message(message);
    }

    /// Handles a keyboard event, dispatching screen-specific actions and returning an [`AppAction`].
    pub fn handle_key(&mut self, key: KeyEvent) -> AppAction {
        if key.kind != KeyEventKind::Press {
            return AppAction::Continue;
        }

        let action = match self.nav.current_screen {
            CurrentScreen::Home => self.handle_key_home(key.code),
            CurrentScreen::ServerList => self.handle_key_server_list(key.code),
            CurrentScreen::TagList => self.handle_key_tag_list(key.code),
            CurrentScreen::TagValues => self.handle_key_tag_values(key.code),
            CurrentScreen::WriteInput => self.handle_key_write_input(key.code),
            CurrentScreen::Loading => {
                if key.code == KeyCode::Esc {
                    self.go_back();
                }
                AppAction::Continue
            }
            CurrentScreen::Exiting => AppAction::Quit,
        };

        if self.nav.current_screen == CurrentScreen::Exiting {
            AppAction::Quit
        } else {
            action
        }
    }

    fn handle_key_home(&mut self, code: KeyCode) -> AppAction {
        match code {
            KeyCode::Enter => self.start_fetch_servers(),
            KeyCode::Char(c) => self.nav.host_input.push(c),
            KeyCode::Backspace => {
                self.nav.host_input.pop();
            }
            KeyCode::Esc => {
                self.log_transition(CurrentScreen::Exiting, "user_quit");
                return AppAction::Quit;
            }
            _ => {}
        }
        AppAction::Continue
    }

    fn handle_key_server_list(&mut self, code: KeyCode) -> AppAction {
        match code {
            KeyCode::Esc => self.go_back(),
            KeyCode::PageDown => self.page_down(),
            KeyCode::PageUp => self.page_up(),
            KeyCode::Down => self.select_next(),
            KeyCode::Up => self.select_prev(),
            KeyCode::Enter => self.start_browse_tags(),
            KeyCode::Char('q' | 'Q') => {
                self.log_transition(CurrentScreen::Exiting, "user_quit");
                return AppAction::Quit;
            }
            _ => {}
        }
        AppAction::Continue
    }

    fn handle_key_tag_list(&mut self, code: KeyCode) -> AppAction {
        if self.search.search_mode {
            match code {
                KeyCode::Esc => self.exit_search_mode(),
                KeyCode::Backspace => self.search_backspace(),
                KeyCode::Tab => self.next_search_match(),
                KeyCode::BackTab => self.prev_search_match(),
                KeyCode::Char(' ') => self.toggle_tag_selection(),
                KeyCode::Enter => {
                    self.exit_search_mode();
                    self.start_read_values();
                }
                KeyCode::Char(c) => self.update_search_query(c),
                _ => {}
            }
        } else {
            match code {
                KeyCode::Esc => self.go_back(),
                KeyCode::PageDown => self.page_down(),
                KeyCode::PageUp => self.page_up(),
                KeyCode::Down => self.select_next(),
                KeyCode::Up => self.select_prev(),
                KeyCode::Char(' ') => self.toggle_tag_selection(),
                KeyCode::Char('s' | 'S') => self.enter_search_mode(),
                KeyCode::Enter => self.start_read_values(),
                KeyCode::Char('q' | 'Q') => {
                    self.log_transition(CurrentScreen::Exiting, "user_quit");
                    return AppAction::Quit;
                }
                _ => {}
            }
        }
        AppAction::Continue
    }

    fn handle_key_tag_values(&mut self, code: KeyCode) -> AppAction {
        match code {
            KeyCode::Esc => self.go_back(),
            KeyCode::PageDown => self.page_down(),
            KeyCode::PageUp => self.page_up(),
            KeyCode::Down => self.select_next(),
            KeyCode::Up => self.select_prev(),
            KeyCode::Char('w' | 'W') => self.enter_write_mode(),
            KeyCode::Char('q' | 'Q') => {
                self.log_transition(CurrentScreen::Exiting, "user_quit");
                return AppAction::Quit;
            }
            _ => {}
        }
        AppAction::Continue
    }

    fn handle_key_write_input(&mut self, code: KeyCode) -> AppAction {
        match code {
            KeyCode::Enter => self.start_write_value(),
            KeyCode::Esc => self.go_back(),
            KeyCode::Char(c) => self.dialog.write_value_input.push(c),
            KeyCode::Backspace => {
                self.dialog.write_value_input.pop();
            }
            _ => {}
        }
        AppAction::Continue
    }

    /// Returns the progress count of the active browse operation.
    pub fn loading_progress(&self) -> usize {
        self.tasks.browse_collector.len()
    }

    /// Initiates asynchronous server enumeration on the configured host.
    #[tracing::instrument(level = "info", skip(self))]
    pub fn start_fetch_servers(&mut self) {
        let host = self.nav.host_input.clone();
        self.log_transition(CurrentScreen::Loading, "start_fetch_servers");
        self.add_message(format!("Connecting to {host}..."));

        let provider = Arc::clone(&self.opc_provider);
        let (tx, rx) = oneshot::channel();

        let join_handle = tokio::spawn(async move {
            let result = tokio::time::timeout(
                std::time::Duration::from_secs(OPC_TIMEOUT_SECS),
                provider.list_servers(&host),
            )
            .await;

            let final_result = result.unwrap_or_else(|_| {
                tracing::error!("Server listing timed out ({OPC_TIMEOUT_SECS}s)");
                Err(OpcError::Internal(format!(
                    "Connection timed out ({OPC_TIMEOUT_SECS}s)"
                )))
            });

            let _ = tx.send(final_result);
        });

        self.tasks.set_active_task(join_handle.abort_handle());
        self.tasks.fetch_result_rx = Some(rx);
    }

    /// Polls the background server enumeration task and updates application state.
    pub fn poll_fetch_result(&mut self) {
        match poll_channel(&mut self.tasks.fetch_result_rx) {
            PollOutcome::Ready(Ok(servers)) => {
                self.view.servers = servers;
                self.log_transition(CurrentScreen::ServerList, "fetch_result_success");
                if self.view.servers.is_empty() {
                    self.view.selected_index = None;
                    self.view.list_state.select(None);
                } else {
                    self.view.selected_index = Some(0);
                    self.view.list_state.select(Some(0));
                }
                let count = self.view.servers.len();
                let host = self.nav.host_input.clone();
                self.add_message(format!("Found {count} servers on {host}"));
            }
            PollOutcome::Ready(Err(e)) => {
                self.log_transition(CurrentScreen::Home, "fetch_result_error");
                tracing::error!(error = %e, "Failed to fetch servers");
                self.add_message(format!("Error fetching servers: {e}"));
            }
            PollOutcome::Closed => {
                self.log_transition(CurrentScreen::Home, "fetch_result_closed");
                tracing::error!(
                    "Server listing background task terminated unexpectedly (sender dropped)"
                );
                self.add_message("Server listing task terminated unexpectedly".into());
            }
            PollOutcome::Pending => {}
        }
    }

    /// Moves the active cursor selection forward by one row on list/table screens.
    pub fn select_next(&mut self) {
        let count = match self.nav.current_screen {
            CurrentScreen::ServerList => self.view.servers.len(),
            CurrentScreen::TagList => self.view.tags.len(),
            CurrentScreen::TagValues => self.view.tag_values.len(),
            _ => 0,
        };

        if count == 0 {
            return;
        }

        if let Some(idx) = self.view.selected_index {
            if idx < count - 1 {
                let new_idx = idx + 1;
                self.view.selected_index = Some(new_idx);
                self.view.list_state.select(Some(new_idx));
                if self.nav.current_screen == CurrentScreen::TagValues {
                    self.view.table_state.select(Some(new_idx));
                }
            }
        } else {
            self.view.selected_index = Some(0);
            self.view.list_state.select(Some(0));
            if self.nav.current_screen == CurrentScreen::TagValues {
                self.view.table_state.select(Some(0));
            }
        }
    }

    /// Moves the active cursor selection backward by one row on list/table screens.
    pub fn select_prev(&mut self) {
        if let Some(idx) = self.view.selected_index
            && idx > 0
        {
            let new_idx = idx - 1;
            self.view.selected_index = Some(new_idx);
            self.view.list_state.select(Some(new_idx));
            if self.nav.current_screen == CurrentScreen::TagValues {
                self.view.table_state.select(Some(new_idx));
            }
        }
    }

    /// Jump forward by PAGE_SIZE items (clamped to end of list).
    pub fn page_down(&mut self) {
        let count = match self.nav.current_screen {
            CurrentScreen::ServerList => self.view.servers.len(),
            CurrentScreen::TagList => self.view.tags.len(),
            CurrentScreen::TagValues => self.view.tag_values.len(),
            _ => 0,
        };

        if count == 0 {
            return;
        }

        let page_size = 20;
        if let Some(idx) = self.view.selected_index {
            let new_idx = (idx + page_size).min(count - 1);
            self.view.selected_index = Some(new_idx);
            self.view.list_state.select(Some(new_idx));
            if self.nav.current_screen == CurrentScreen::TagValues {
                self.view.table_state.select(Some(new_idx));
            }
        } else {
            self.view.selected_index = Some(0);
            self.view.list_state.select(Some(0));
            if self.nav.current_screen == CurrentScreen::TagValues {
                self.view.table_state.select(Some(0));
            }
        }
    }

    /// Jump backward by PAGE_SIZE items (clamped to start of list).
    pub fn page_up(&mut self) {
        let page_size = 20;
        if let Some(idx) = self.view.selected_index {
            let new_idx = idx.saturating_sub(page_size);
            self.view.selected_index = Some(new_idx);
            self.view.list_state.select(Some(new_idx));
            if self.nav.current_screen == CurrentScreen::TagValues {
                self.view.table_state.select(Some(new_idx));
            }
        } else {
            self.view.selected_index = Some(0);
            self.view.list_state.select(Some(0));
            if self.nav.current_screen == CurrentScreen::TagValues {
                self.view.table_state.select(Some(0));
            }
        }
    }

    /// Initiates asynchronous address space tag browsing on the currently selected server.
    #[tracing::instrument(level = "info", skip(self))]
    pub fn start_browse_tags(&mut self) {
        if self.nav.current_screen != CurrentScreen::ServerList {
            return;
        }

        let Some(idx) = self.view.selected_index else {
            return;
        };

        let server = match self.view.servers.get(idx) {
            Some(s) => s.clone(),
            None => return,
        };

        self.nav.browsed_server = Some(server.clone());

        self.log_transition(CurrentScreen::Loading, "start_browse_tags");
        let collector = TagCollector::new(MAX_BROWSE_TAGS);
        self.tasks.browse_collector = collector.clone();
        self.add_message(format!("Browsing tags on {server}..."));

        let provider = Arc::clone(&self.opc_provider);
        let collector_for_task = collector.clone();

        let (tx, rx) = oneshot::channel();

        let join_handle = tokio::spawn(async move {
            let timeout_duration = std::time::Duration::from_secs(OPC_TIMEOUT_SECS);
            let result = tokio::time::timeout(
                timeout_duration,
                provider.browse_tags(&server, collector_for_task),
            )
            .await;

            let final_result = match result {
                Ok(inner) => inner,
                Err(_) => {
                    // Timeout occurred. Signal cooperative cancellation to stop background
                    // worker threads and harvest whatever tags have been accumulated so far.
                    collector.cancel();
                    let partial_tags = collector.harvest();

                    if !partial_tags.is_empty() {
                        tracing::warn!(
                            server = %server,
                            count = partial_tags.len(),
                            timeout_secs = OPC_TIMEOUT_SECS,
                            "Browse tags timed out; returning partial results"
                        );
                        Ok(partial_tags)
                    } else {
                        tracing::error!(
                            server = %server,
                            timeout_secs = OPC_TIMEOUT_SECS,
                            "Browse tags timed out with zero tags found"
                        );
                        Err(OpcError::Internal(format!(
                            "Browse timed out ({OPC_TIMEOUT_SECS}s) for '{server}' with no tags found"
                        )))
                    }
                }
            };

            let _ = tx.send(final_result);
        });

        self.tasks.set_active_task(join_handle.abort_handle());
        self.tasks.browse_result_rx = Some(rx);
    }

    /// Polls the background address space browsing task and populates available tags.
    pub fn poll_browse_result(&mut self) {
        match poll_channel(&mut self.tasks.browse_result_rx) {
            PollOutcome::Ready(Ok(tags)) => {
                self.search.update_tags(&tags);
                self.view.tags = tags;
                self.view.clear_selection();
                self.log_transition(CurrentScreen::TagList, "browse_result_success");
                if self.view.tags.is_empty() {
                    self.view.selected_index = None;
                    self.view.list_state.select(None);
                } else {
                    self.view.selected_index = Some(0);
                    self.view.list_state.select(Some(0));
                }
                let count = self.view.tags.len();
                self.add_message(format!("Found {count} tags"));
            }
            PollOutcome::Ready(Err(e)) => {
                self.log_transition(CurrentScreen::ServerList, "browse_result_error");
                tracing::error!(error = %e, error_chain = ?e, "Browse tags failed");
                let msg = match e.friendly_hint() {
                    Some(h) => format!("Error: {h}"),
                    None => format!("Error: {e:#}"),
                };
                self.add_message(msg);
            }
            PollOutcome::Closed => {
                self.log_transition(CurrentScreen::ServerList, "browse_result_closed");
                tracing::error!(
                    "Browse tags background task terminated unexpectedly (sender dropped)"
                );
                self.add_message("Browse task terminated unexpectedly".into());
            }
            PollOutcome::Pending => {}
        }
    }

    /// Toggle tag selection at the current selected index.
    pub fn toggle_tag_selection(&mut self) {
        if self.nav.current_screen != CurrentScreen::TagList {
            return;
        }
        if let Some(idx) = self.view.selected_index
            && let Some(tag) = self.view.tags.get(idx).cloned()
        {
            let is_sel = self.view.toggle_selection(&tag);
            tracing::debug!(
                tag = %tag,
                selected = is_sel,
                "toggle_tag_selection"
            );
        }
    }

    /// Spawns background tag value reading for the specified server and tags.
    fn spawn_read_task(&mut self, server: String, tag_ids: Vec<String>) {
        let provider = Arc::clone(&self.opc_provider);
        let (tx, rx) = oneshot::channel();
        let batch = TagBatch::from(tag_ids);

        let join_handle = tokio::spawn(async move {
            let result = tokio::time::timeout(
                std::time::Duration::from_secs(OPC_TIMEOUT_SECS),
                provider.read_tag_values(&server, batch),
            )
            .await;

            let final_result = match result {
                Ok(inner) => inner,
                Err(_) => {
                    tracing::error!("Read tag values timed out ({OPC_TIMEOUT_SECS}s)");
                    Err(OpcError::Internal(format!(
                        "Read timed out ({OPC_TIMEOUT_SECS}s)"
                    )))
                }
            };

            let _ = tx.send(final_result);
        });

        self.tasks.set_active_task(join_handle.abort_handle());
        self.tasks.read_result_rx = Some(rx);
    }

    /// Start reading values for selected tags.
    #[tracing::instrument(level = "debug", skip(self))]
    pub fn start_read_values(&mut self) {
        if self.nav.current_screen != CurrentScreen::TagList {
            return;
        }

        // Gather selected tag IDs
        let selected_tag_ids: Vec<String> = self
            .view
            .tags
            .iter()
            .filter(|t| self.view.is_selected(t))
            .cloned()
            .collect();

        if selected_tag_ids.is_empty() {
            tracing::debug!("start_read_values: no tags selected");
            self.add_message("No tags selected. Press Space to select tags.".into());
            return;
        }

        let server = match &self.nav.browsed_server {
            Some(s) => s.clone(),
            None => {
                self.add_message("No server context — please browse tags first".into());
                return;
            }
        };

        // Store context for auto-refresh
        self.refresher.server = Some(server.clone());
        self.refresher.tag_ids.clone_from(&selected_tag_ids);

        tracing::info!(
            server = %server,
            count = selected_tag_ids.len(),
            tags = ?selected_tag_ids,
            "start_read_values: sending tags to backend"
        );
        self.log_transition(CurrentScreen::Loading, "start_read_values");
        self.add_message(format!("Reading {} tag values...", selected_tag_ids.len()));

        self.spawn_read_task(server, selected_tag_ids);
    }

    /// Polls the background tag reading task and updates the tag values display table.
    pub fn poll_read_result(&mut self) {
        match poll_channel(&mut self.tasks.read_result_rx) {
            PollOutcome::Ready(Ok(values)) => {
                self.view.tag_values = values;
                self.log_transition(CurrentScreen::TagValues, "read_result_success");
                if self.view.tag_values.is_empty() {
                    self.view.selected_index = None;
                    self.view.table_state.select(None);
                } else if let Some(idx) = self.view.selected_index {
                    let clamped = idx.min(self.view.tag_values.len() - 1);
                    self.view.selected_index = Some(clamped);
                    self.view.table_state.select(Some(clamped));
                } else {
                    self.view.selected_index = Some(0);
                    self.view.table_state.select(Some(0));
                }

                let total = self.view.tag_values.len();
                let error_count = self
                    .view
                    .tag_values
                    .iter()
                    .filter(|tv| tv.is_error())
                    .count();
                let bad_quality_count = self
                    .view
                    .tag_values
                    .iter()
                    .filter(|tv| tv.is_bad() && !tv.is_error())
                    .count();

                match (error_count > 0, bad_quality_count > 0) {
                    (true, true) => self.add_message(format!(
                        "Read {total} tag values (⚠ {error_count} errors, {bad_quality_count} bad quality)"
                    )),
                    (true, false) => self.add_message(format!(
                        "Read {total} tag values (⚠ {error_count} errors)"
                    )),
                    (false, true) => self.add_message(format!(
                        "Read {total} tag values (⚠ {bad_quality_count} bad quality)"
                    )),
                    (false, false) => self.add_message(format!("Read {total} tag values")),
                }

                self.refresher.last_read_time = Some(std::time::Instant::now());
            }
            PollOutcome::Ready(Err(e)) => {
                self.log_transition(CurrentScreen::TagList, "read_result_error");
                tracing::error!(error = %e, error_chain = ?e, "Read tag values failed");
                let msg = match e.friendly_hint() {
                    Some(h) => format!("Error reading values: {h}"),
                    None => format!("Error reading values: {e:#}"),
                };
                self.add_message(msg);
            }
            PollOutcome::Closed => {
                self.log_transition(CurrentScreen::TagList, "read_result_closed");
                tracing::error!(
                    "Read values background task terminated unexpectedly (sender dropped)"
                );
                self.add_message("Read task terminated unexpectedly".into());
            }
            PollOutcome::Pending => {}
        }
    }

    /// Enter write mode for a tag.
    ///
    /// Triggered from TagValues. If only one tag is displayed, it is auto-selected.
    /// If multiple are displayed, the currently highlighted row is used.
    pub fn enter_write_mode(&mut self) {
        if self.nav.current_screen != CurrentScreen::TagValues {
            return;
        }

        let tag_id = if self.view.tag_values.len() == 1 {
            Some(self.view.tag_values[0].tag_id.clone())
        } else if let Some(idx) = self.view.table_state.selected() {
            self.view
                .tag_values
                .get_index(idx)
                .map(|tv| tv.tag_id.clone())
        } else {
            None
        };

        if let Some(id) = tag_id {
            tracing::debug!(tag_id = %id, "enter_write_mode: entering write mode for tag");
            self.dialog.write_tag_id = Some(id);
            self.dialog.write_value_input.clear();
            self.log_transition(CurrentScreen::WriteInput, "enter_write_mode");
        } else {
            tracing::debug!("enter_write_mode: no tag selected");
            self.add_message("No tag selected to write.".into());
        }
    }

    /// Start writing a value to the selected tag.
    #[tracing::instrument(level = "info", skip(self))]
    pub fn start_write_value(&mut self) {
        let tag_id = match &self.dialog.write_tag_id {
            Some(t) => t.clone(),
            None => return,
        };
        let value_str = self.dialog.write_value_input.trim().to_string();
        if value_str.is_empty() {
            self.add_message("Value cannot be empty.".into());
            return;
        }

        let opc_value = self.resolve_write_value(&tag_id, &value_str);

        tracing::info!(
            tag = %tag_id,
            value = %value_str,
            parsed_type = ?opc_value,
            "start_write_value: initiating write"
        );

        let server = match &self.refresher.server {
            Some(s) => s.clone(),
            None => {
                self.add_message("No server context for write.".into());
                return;
            }
        };

        self.log_transition(CurrentScreen::Loading, "start_write_value");
        self.add_message(format!("Writing '{value_str}' to {tag_id}..."));

        let provider = Arc::clone(&self.opc_provider);
        let (tx, rx) = oneshot::channel();

        const OPC_TIMEOUT_SECS_WRITE: u64 = 10;

        let join_handle = tokio::spawn(async move {
            let result = tokio::time::timeout(
                std::time::Duration::from_secs(OPC_TIMEOUT_SECS_WRITE),
                provider.write_tag_value(&server, &tag_id, opc_value),
            )
            .await;

            let final_result = match result {
                Ok(inner) => inner,
                Err(_) => {
                    tracing::error!("Write tag value timed out ({OPC_TIMEOUT_SECS_WRITE}s)");
                    Err(OpcError::Internal(format!(
                        "Write timed out ({OPC_TIMEOUT_SECS_WRITE}s)"
                    )))
                }
            };
            let _ = tx.send(final_result);
        });

        self.tasks.set_active_task(join_handle.abort_handle());
        self.tasks.write_result_rx = Some(rx);
    }

    /// Poll for the result of the background write operation.
    pub fn poll_write_result(&mut self) {
        match poll_channel(&mut self.tasks.write_result_rx) {
            PollOutcome::Ready(Ok(result)) => {
                match &result.status {
                    Ok(()) => {
                        tracing::info!(tag = %result.tag_id, "poll_write_result: write succeeded");
                        self.add_message(format!("✓ Write to '{}' succeeded", result.tag_id));
                    }
                    Err(e) => {
                        tracing::warn!(tag = %result.tag_id, error = %e, "poll_write_result: write failed");
                        self.add_message(format!("✗ Write to '{}' failed: {e}", result.tag_id));
                    }
                }
                self.log_transition(CurrentScreen::TagValues, "write_result_success");
                // Trigger a refresh to show the new value
                self.start_read_values();
            }
            PollOutcome::Ready(Err(e)) => {
                tracing::error!(error = %e, "Write tag values failed");
                self.add_message(format!("Write error: {e:#}"));
                self.log_transition(CurrentScreen::TagValues, "write_result_error");
            }
            PollOutcome::Closed => {
                self.log_transition(CurrentScreen::TagValues, "write_result_closed");
                tracing::error!("Write background task terminated unexpectedly");
                self.add_message("Write task terminated unexpectedly".into());
            }
            PollOutcome::Pending => {}
        }
    }

    /// Periodically triggers asynchronous tag reading if on the `TagValues` screen and elapsed time exceeds threshold.
    pub fn maybe_auto_refresh(&mut self) {
        if self.nav.current_screen != CurrentScreen::TagValues {
            return;
        }
        if self.tasks.read_result_rx.is_some() {
            return; // Read already in-flight
        }
        let elapsed = match self.refresher.last_read_time {
            Some(t) => t.elapsed(),
            None => return,
        };
        if elapsed < std::time::Duration::from_secs(1) {
            return;
        }

        let server_name = match &self.refresher.server {
            Some(s) => s.clone(),
            None => return,
        };
        let tag_ids = self.refresher.tag_ids.clone();
        if tag_ids.is_empty() {
            return;
        }

        tracing::debug!(tag_count = tag_ids.len(), "Auto-refreshing tag values");
        self.spawn_read_task(server_name, tag_ids);
    }

    /// Enter search mode, clearing any previous query.
    pub fn enter_search_mode(&mut self) {
        if self.nav.current_screen != CurrentScreen::TagList {
            return;
        }
        self.search.enter();
    }

    /// Exit search mode, keeping cursor position.
    pub fn exit_search_mode(&mut self) {
        self.search.exit();
    }

    /// Update the search query and recompute matches.
    pub fn update_search_query(&mut self, c: char) {
        self.search.push_char(c);
        if let Some(first) = self.search.first_match() {
            self.view.selected_index = Some(first);
            self.view.list_state.select(Some(first));
        }
    }

    /// Delete last character from search query and recompute.
    pub fn search_backspace(&mut self) {
        self.search.backspace();
        if let Some(first) = self.search.first_match() {
            self.view.selected_index = Some(first);
            self.view.list_state.select(Some(first));
        }
    }

    /// Jump to the next search match.
    pub fn next_search_match(&mut self) {
        if let Some(next_idx) = self.search.next_match() {
            self.view.selected_index = Some(next_idx);
            self.view.list_state.select(Some(next_idx));
        }
    }

    /// Jump to the previous search match.
    pub fn prev_search_match(&mut self) {
        if let Some(prev_idx) = self.search.prev_match() {
            self.view.selected_index = Some(prev_idx);
            self.view.list_state.select(Some(prev_idx));
        }
    }

    /// Navigates backward one screen in the TUI navigation hierarchy, resetting child state.
    ///
    /// On [`CurrentScreen::Loading`], cooperative background task cancellation is triggered,
    /// aborting active tasks and returning cleanly to [`previous_screen`](NavigationState::previous_screen).
    pub fn go_back(&mut self) {
        match self.nav.current_screen {
            CurrentScreen::Loading => {
                self.tasks.cancel_all();
                let target = self.nav.previous_screen;
                self.log_transition(target, "go_back_cancel_loading");
                self.add_message("Operation cancelled".into());
            }
            CurrentScreen::ServerList => {
                self.log_transition(CurrentScreen::Home, "go_back");
                self.view.servers.clear();
                self.view.selected_index = None;
                self.view.list_state.select(None);
            }
            CurrentScreen::TagList => {
                self.log_transition(CurrentScreen::ServerList, "go_back");
                self.view.tags.clear();
                self.view.clear_selection();
                self.search.update_tags(&[]);
                // Restore selection to the previous server if possible
                if !self.view.servers.is_empty() {
                    self.view.selected_index = Some(0);
                    self.view.list_state.select(Some(0));
                }
            }
            CurrentScreen::TagValues => {
                self.log_transition(CurrentScreen::TagList, "go_back");
                self.view.tag_values.clear();
                self.refresher.clear();
                // Restore selection to tags list
                if !self.view.tags.is_empty() {
                    self.view.selected_index = Some(0);
                    self.view.list_state.select(Some(0));
                } else {
                    self.view.selected_index = None;
                    self.view.list_state.select(None);
                }
            }
            CurrentScreen::WriteInput => {
                self.log_transition(CurrentScreen::TagValues, "go_back");
                self.dialog.clear();
            }
            _ => {}
        }
    }

    /// Resolves and parses user write input into an [`OpcValue`] with context-aware type coercion.
    ///
    /// If the existing tag is known to be a boolean, `"1"` and `"0"` are coerced to `OpcValue::Bool(true)`
    /// and `OpcValue::Bool(false)` respectively (REV-14). Otherwise, canonical [`OpcValue`]
    /// parsing via [`std::str::FromStr`] rules apply.
    ///
    /// # Arguments
    ///
    /// * `tag_id` - Identifier of the tag being written.
    /// * `value_str` - Raw string entered by the user.
    ///
    /// # Returns
    ///
    /// Returns the coerced [`OpcValue`].
    pub fn resolve_write_value(&self, tag_id: &str, value_str: &str) -> OpcValue {
        let mut opc_value = value_str
            .parse::<OpcValue>()
            .unwrap_or_else(|_| OpcValue::String(value_str.to_string()));

        let is_bool = self
            .view
            .tag_values
            .iter()
            .find(|tv| tv.tag_id == tag_id)
            .is_some_and(|tv| matches!(tv.value(), Some(OpcValue::Bool(_))));

        if is_bool {
            if value_str == "1" {
                opc_value = OpcValue::Bool(true);
            } else if value_str == "0" {
                opc_value = OpcValue::Bool(false);
            }
        }

        opc_value
    }
}

#[cfg(test)]
use opc_da_client::MockOpcProvider;

#[cfg(test)]
#[derive(Default)]
pub struct TestAppBuilder {
    mock_provider: Option<MockOpcProvider>,
    screen: CurrentScreen,
    servers: Vec<String>,
    tags: Vec<String>,
}

#[cfg(test)]
impl TestAppBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn with_provider(mut self, mock: MockOpcProvider) -> Self {
        self.mock_provider = Some(mock);
        self
    }

    #[must_use]
    pub fn with_screen(mut self, screen: CurrentScreen) -> Self {
        self.screen = screen;
        self
    }

    #[must_use]
    pub fn with_servers(mut self, servers: Vec<String>) -> Self {
        self.servers = servers;
        self
    }

    #[must_use]
    pub fn with_tags(mut self, tags: Vec<String>) -> Self {
        self.tags = tags;
        self
    }

    pub fn build(self) -> App {
        let provider = self.mock_provider.unwrap_or_default();
        let mut app = App::new(Arc::new(provider));
        app.nav.current_screen = self.screen;
        if !self.servers.is_empty() {
            app.view.servers = self.servers;
            app.view.selected_index = Some(0);
            app.view.list_state.select(Some(0));
        }
        if !self.tags.is_empty() {
            app.search.update_tags(&self.tags);
            app.view.tags = self.tags;
            app.view.selected_index = Some(0);
            app.view.list_state.select(Some(0));
        }
        app
    }
}

#[cfg(test)]
pub fn test_app() -> App {
    TestAppBuilder::new().build()
}

#[cfg(test)]
mod tests {
    use super::*;
    use mockall::predicate::*;
    use opc_da_client::{MockOpcProvider, OpcError, OpcQuality, OpcResult, TagValue, WriteResult};

    #[test]
    fn test_poll_fetch_result_success() {
        let (tx, rx) = oneshot::channel();
        let mut app = test_app();
        app.nav.current_screen = CurrentScreen::Loading;
        app.tasks.fetch_result_rx = Some(rx);

        tx.send(Ok(vec!["Server1".into(), "Server2".into()]))
            .unwrap();
        app.poll_fetch_result();

        assert_eq!(app.nav.current_screen, CurrentScreen::ServerList);
        assert_eq!(app.view.servers.len(), 2);
        assert_eq!(app.view.selected_index, Some(0));
        assert!(app.tasks.fetch_result_rx.is_none());
        assert!(
            app.view
                .messages
                .back()
                .unwrap()
                .contains("Found 2 servers")
        );
    }

    #[test]
    fn test_poll_fetch_result_error() {
        let (tx, rx) = oneshot::channel();
        let mut app = test_app();
        app.nav.current_screen = CurrentScreen::Loading;
        app.tasks.fetch_result_rx = Some(rx);

        tx.send(Err(OpcError::Internal("Connection failed".to_string())))
            .unwrap();
        app.poll_fetch_result();

        assert_eq!(app.nav.current_screen, CurrentScreen::Home);
        assert!(app.tasks.fetch_result_rx.is_none());
        assert!(app.view.messages.back().unwrap().contains("Error"));
    }

    #[test]
    fn test_poll_fetch_result_empty_servers() {
        let (tx, rx) = oneshot::channel();
        let mut app = test_app();
        app.nav.current_screen = CurrentScreen::Loading;
        app.tasks.fetch_result_rx = Some(rx);

        tx.send(Ok(vec![])).unwrap();
        app.poll_fetch_result();

        assert_eq!(app.nav.current_screen, CurrentScreen::ServerList);
        assert!(app.view.servers.is_empty());
        assert_eq!(app.view.selected_index, None);
        assert!(
            app.view
                .messages
                .back()
                .unwrap()
                .contains("Found 0 servers")
        );
    }

    #[test]
    fn test_poll_fetch_result_closed() {
        let (tx, rx) = oneshot::channel::<OpcResult<Vec<String>>>();
        let mut app = test_app();
        app.nav.current_screen = CurrentScreen::Loading;
        app.tasks.fetch_result_rx = Some(rx);

        drop(tx);
        app.poll_fetch_result();

        assert_eq!(app.nav.current_screen, CurrentScreen::Home);
        assert!(
            app.view
                .messages
                .back()
                .unwrap()
                .contains("terminated unexpectedly")
        );
    }

    #[tokio::test]
    async fn test_start_fetch_servers_sets_loading() {
        let mut mock = MockOpcProvider::new();
        mock.expect_list_servers()
            .returning(|_| Ok(vec!["S1".into()]));

        let mut app = TestAppBuilder::new().with_provider(mock).build();
        app.start_fetch_servers();

        assert_eq!(app.nav.current_screen, CurrentScreen::Loading);
        assert!(app.tasks.fetch_result_rx.is_some());
    }

    #[test]
    fn test_server_navigation() {
        let mut app = TestAppBuilder::new()
            .with_screen(CurrentScreen::ServerList)
            .with_servers(vec!["S1".into(), "S2".into()])
            .build();

        app.select_next();
        assert_eq!(app.view.selected_index, Some(1));

        app.select_next(); // Should stay at 1
        assert_eq!(app.view.selected_index, Some(1));

        app.select_prev();
        assert_eq!(app.view.selected_index, Some(0));

        app.select_prev(); // Should stay at 0
        assert_eq!(app.view.selected_index, Some(0));
    }

    #[test]
    fn test_tag_navigation_logic() {
        let mut app = TestAppBuilder::new()
            .with_screen(CurrentScreen::TagList)
            .with_servers(vec!["S1".into()])
            .with_tags(vec!["T1".into(), "T2".into()])
            .build();

        app.select_next();
        assert_eq!(app.view.selected_index, Some(1));
        assert_eq!(app.view.list_state.selected(), Some(1));

        app.select_next(); // Should stay at 1
        assert_eq!(app.view.selected_index, Some(1));
    }

    #[tokio::test]
    async fn test_enter_selected_server_navigation() {
        let mut mock = MockOpcProvider::new();
        mock.expect_browse_tags()
            .with(eq("S1"), always())
            .returning(|_, _| Ok(vec!["T1".into()]));

        let mut app = TestAppBuilder::new()
            .with_provider(mock)
            .with_screen(CurrentScreen::ServerList)
            .with_servers(vec!["S1".into()])
            .build();

        app.start_browse_tags();
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        app.poll_browse_result();

        assert!(matches!(app.nav.current_screen, CurrentScreen::TagList));
        assert_eq!(app.view.tags.len(), 1);
        assert_eq!(app.view.selected_index, Some(0));
    }

    #[tokio::test]
    async fn test_browse_tags_collector_timeout_and_cancellation() {
        let collector = TagCollector::new(10);
        assert_eq!(collector.max_tags(), 10);
        assert_eq!(collector.len(), 0);
        assert!(!collector.is_cancelled());

        assert!(collector.push("Device.Tag1".into()));
        assert!(collector.push("Device.Tag2".into()));
        assert_eq!(collector.len(), 2);

        collector.cancel();
        assert!(collector.is_cancelled());

        let harvested = collector.harvest();
        assert_eq!(harvested, vec!["Device.Tag1", "Device.Tag2"]);
        assert!(collector.is_empty());
        assert_eq!(collector.len(), 0);
    }

    #[test]
    fn test_go_back_navigation() {
        let mut app = TestAppBuilder::new()
            .with_screen(CurrentScreen::TagList)
            .with_servers(vec!["S1".into()])
            .with_tags(vec!["T1".into()])
            .build();

        // TagList -> ServerList
        app.go_back();
        assert!(matches!(app.nav.current_screen, CurrentScreen::ServerList));
        assert!(app.view.tags.is_empty());
        assert_eq!(app.view.selected_index, Some(0));

        // ServerList -> Home
        app.go_back();
        assert!(matches!(app.nav.current_screen, CurrentScreen::Home));
        assert!(app.view.servers.is_empty());
        assert_eq!(app.view.selected_index, None);
    }

    #[test]
    fn test_loading_screen_esc_cancels_and_restores_previous_screen() {
        let mut app = test_app();
        app.nav.current_screen = CurrentScreen::ServerList;
        app.log_transition(CurrentScreen::Loading, "start_browse_tags");
        assert_eq!(app.nav.previous_screen, CurrentScreen::ServerList);
        assert_eq!(app.nav.current_screen, CurrentScreen::Loading);

        let (tx, rx) = oneshot::channel();
        app.tasks.browse_result_rx = Some(rx);

        // Esc pressed while loading
        app.go_back();

        assert_eq!(app.nav.current_screen, CurrentScreen::ServerList);
        assert!(app.tasks.browse_result_rx.is_none());
        assert!(
            app.view
                .messages
                .back()
                .unwrap()
                .contains("Operation cancelled")
        );
        drop(tx);
    }

    #[tokio::test]
    async fn test_loading_transition() {
        let mut app = test_app();
        app.start_fetch_servers();
        assert_eq!(app.nav.current_screen, CurrentScreen::Loading);
        assert!(
            app.view
                .messages
                .iter()
                .any(|m| m.contains("Connecting to"))
        );
    }

    #[tokio::test]
    async fn test_tui_navigation_flow() {
        let (tx, rx) = oneshot::channel();
        let mut app = test_app();

        // 1. Initial State: Home
        assert!(matches!(app.nav.current_screen, CurrentScreen::Home));
        assert_eq!(app.nav.host_input, "localhost");

        // 2. Start fetch
        app.start_fetch_servers();
        assert_eq!(app.nav.current_screen, CurrentScreen::Loading);
        app.tasks.fetch_result_rx = Some(rx);

        // 3. Complete fetch
        tx.send(Ok(vec!["Server1".into()])).unwrap();
        app.poll_fetch_result();

        assert!(matches!(app.nav.current_screen, CurrentScreen::ServerList));
        assert_eq!(app.view.servers.len(), 1);
        assert_eq!(app.view.selected_index, Some(0));
        assert_eq!(app.view.list_state.selected(), Some(0));

        // 4. User goes back to Home
        app.go_back();
        assert!(matches!(app.nav.current_screen, CurrentScreen::Home));
        assert!(app.view.servers.is_empty());
        assert_eq!(app.view.selected_index, None);
        assert_eq!(app.view.list_state.selected(), None);
    }

    #[tokio::test]
    async fn test_poll_browse_result_error_shows_message() {
        let (tx, rx) = oneshot::channel();
        let mut app = test_app();
        app.nav.current_screen = CurrentScreen::Loading;
        app.tasks.browse_result_rx = Some(rx);

        tx.send(Err(OpcError::Internal(
            "DCOM access denied on remote host".to_string(),
        )))
        .unwrap();

        app.poll_browse_result();

        assert_eq!(app.nav.current_screen, CurrentScreen::ServerList);
        assert!(app.tasks.browse_result_rx.is_none());
        let last_msg = app.view.messages.back().unwrap();
        assert!(last_msg.contains("Error: "));
        assert!(last_msg.contains("DCOM access denied"));
    }

    #[tokio::test]
    async fn test_poll_browse_result_closed_shows_message() {
        let (tx, rx) = oneshot::channel();
        let mut app = test_app();
        app.nav.current_screen = CurrentScreen::Loading;
        app.tasks.browse_result_rx = Some(rx);

        drop(tx);
        app.poll_browse_result();

        assert_eq!(app.nav.current_screen, CurrentScreen::ServerList);
        assert!(app.tasks.browse_result_rx.is_none());
        let last_msg = app.view.messages.back().unwrap();
        assert!(last_msg.contains("terminated unexpectedly"));
    }

    #[tokio::test]
    async fn test_poll_browse_result_empty_tags() {
        let (tx, rx) = oneshot::channel();
        let mut app = test_app();
        app.nav.current_screen = CurrentScreen::Loading;
        app.tasks.browse_result_rx = Some(rx);

        tx.send(Ok(vec![])).unwrap();
        app.poll_browse_result();

        assert_eq!(app.nav.current_screen, CurrentScreen::TagList);
        assert!(app.view.tags.is_empty());
        assert_eq!(app.view.selected_index, None);
        assert_eq!(app.view.list_state.selected(), None);
        assert!(app.view.messages.back().unwrap().contains("Found 0 tags"));
    }

    #[test]
    fn test_start_browse_no_selection() {
        let mut app = TestAppBuilder::new()
            .with_screen(CurrentScreen::ServerList)
            .with_servers(vec!["S1".into()])
            .build();
        app.view.selected_index = None;

        app.start_browse_tags();

        assert_eq!(app.nav.current_screen, CurrentScreen::ServerList);
        assert!(app.tasks.browse_result_rx.is_none());
    }

    #[test]
    fn test_start_browse_wrong_screen() {
        let mut app = TestAppBuilder::new()
            .with_screen(CurrentScreen::Home)
            .with_servers(vec!["S1".into()])
            .build();

        app.start_browse_tags();

        assert_eq!(app.nav.current_screen, CurrentScreen::Home);
        assert!(app.tasks.browse_result_rx.is_none());
    }

    #[test]
    fn test_poll_fetch_result_timeout() {
        let (tx, rx) = oneshot::channel();
        let mut app = test_app();
        app.nav.current_screen = CurrentScreen::Loading;
        app.tasks.fetch_result_rx = Some(rx);

        tx.send(Err(OpcError::Internal(
            "Connection timed out (30s)".to_string(),
        )))
        .unwrap();
        app.poll_fetch_result();

        assert_eq!(app.nav.current_screen, CurrentScreen::Home);
        assert!(app.view.messages.back().unwrap().contains("timed out"));
    }

    #[test]
    fn test_add_message_ring_buffer() {
        let mut app = test_app();

        for i in 0..15 {
            app.add_message(format!("msg-{}", i));
        }

        assert_eq!(app.view.messages.len(), 10);
        assert_eq!(app.view.messages.front().unwrap(), "msg-5");
        assert_eq!(app.view.messages.back().unwrap(), "msg-14");
    }

    #[test]
    fn test_select_on_empty_list() {
        let mut app = TestAppBuilder::new()
            .with_screen(CurrentScreen::ServerList)
            .build();

        app.select_next();
        assert_eq!(app.view.selected_index, None);

        app.select_prev();
        assert_eq!(app.view.selected_index, None);
    }

    #[test]
    fn test_poll_browse_result_no_task() {
        let mut app = TestAppBuilder::new()
            .with_screen(CurrentScreen::ServerList)
            .build();

        app.poll_browse_result();
        assert_eq!(app.nav.current_screen, CurrentScreen::ServerList);
    }

    #[test]
    fn test_toggle_tag_selection_hashset() {
        let mut app = TestAppBuilder::new()
            .with_screen(CurrentScreen::TagList)
            .with_tags(vec!["Tag1".into(), "Tag2".into()])
            .build();
        app.view.selected_index = Some(1);

        assert!(!app.view.is_selected("Tag2"));
        app.toggle_tag_selection();
        assert!(app.view.is_selected("Tag2"));

        app.toggle_tag_selection();
        assert!(!app.view.is_selected("Tag2"));
    }

    #[test]
    fn test_start_read_values_no_selection() {
        let mut app = TestAppBuilder::new()
            .with_screen(CurrentScreen::TagList)
            .with_tags(vec!["Tag1".into()])
            .build();

        app.start_read_values();

        assert_eq!(app.nav.current_screen, CurrentScreen::TagList);
        assert!(
            app.view
                .messages
                .back()
                .unwrap()
                .contains("No tags selected")
        );
        assert!(app.tasks.read_result_rx.is_none());
    }

    #[test]
    fn test_start_read_values_wrong_screen() {
        let mut app = TestAppBuilder::new()
            .with_screen(CurrentScreen::ServerList)
            .build();

        app.start_read_values();

        assert_eq!(app.nav.current_screen, CurrentScreen::ServerList);
        assert!(app.tasks.read_result_rx.is_none());
    }

    #[tokio::test]
    async fn test_start_read_values_success() {
        let mut mock = MockOpcProvider::new();
        mock.expect_read_tag_values()
            .with(
                eq("TestServer"),
                eq(TagBatch::from(vec!["Tag1".to_string()])),
            )
            .returning(|_, _| Ok(TagValues::default()));

        let mut app = TestAppBuilder::new()
            .with_provider(mock)
            .with_screen(CurrentScreen::TagList)
            .with_tags(vec!["Tag1".into()])
            .build();

        app.view.toggle_selection("Tag1");
        app.nav.browsed_server = Some("TestServer".into());

        app.start_read_values();

        assert_eq!(app.nav.current_screen, CurrentScreen::Loading);
        assert!(app.tasks.read_result_rx.is_some());
        assert_eq!(app.refresher.server, Some("TestServer".into()));
    }

    #[test]
    fn test_start_read_values_no_browsed_server() {
        let mut app = TestAppBuilder::new()
            .with_screen(CurrentScreen::TagList)
            .with_tags(vec!["Tag1".into()])
            .build();

        app.view.toggle_selection("Tag1");
        app.nav.browsed_server = None;

        app.start_read_values();

        assert_eq!(app.nav.current_screen, CurrentScreen::TagList);
        assert!(app.tasks.read_result_rx.is_none());
        assert!(
            app.view
                .messages
                .back()
                .unwrap()
                .contains("No server context")
        );
    }

    #[test]
    fn test_poll_read_result_success() {
        let (tx, rx) = oneshot::channel();
        let mut app = test_app();
        app.nav.current_screen = CurrentScreen::Loading;
        app.tasks.read_result_rx = Some(rx);

        let values = TagValues::from(vec![TagValue::new(
            "Tag1",
            Some(OpcValue::Int(123)),
            OpcQuality::GOOD,
            Some(std::time::SystemTime::UNIX_EPOCH),
        )]);

        tx.send(Ok(values)).unwrap();
        app.poll_read_result();

        assert_eq!(app.nav.current_screen, CurrentScreen::TagValues);
        assert_eq!(app.view.tag_values.len(), 1);
        assert_eq!(app.view.tag_values[0].value(), Some(&OpcValue::Int(123)));
        assert_eq!(app.view.tag_values[0].display_value(), "123");
        assert!(app.tasks.read_result_rx.is_none());
    }

    #[test]
    fn test_poll_read_result_error() {
        let (tx, rx) = oneshot::channel();
        let mut app = test_app();
        app.nav.current_screen = CurrentScreen::Loading;
        app.tasks.read_result_rx = Some(rx);

        tx.send(Err(OpcError::Internal("Read failed".to_string())))
            .unwrap();
        app.poll_read_result();

        assert_eq!(app.nav.current_screen, CurrentScreen::TagList);
        assert!(app.tasks.read_result_rx.is_none());
        assert!(
            app.view
                .messages
                .back()
                .unwrap()
                .contains("Error reading values")
        );
    }

    #[test]
    fn test_go_back_from_tag_values() {
        let mut app = TestAppBuilder::new()
            .with_screen(CurrentScreen::TagValues)
            .with_tags(vec!["Tag1".into()])
            .build();
        app.view.tag_values = TagValues::from(vec![TagValue::new(
            "Tag1",
            Some(OpcValue::Int(100)),
            OpcQuality::GOOD,
            None,
        )]);

        app.go_back();

        assert_eq!(app.nav.current_screen, CurrentScreen::TagList);
        assert!(app.view.tag_values.is_empty());
        assert_eq!(app.view.tags.len(), 1);
    }

    #[test]
    fn test_select_next_on_tag_values() {
        let mut app = TestAppBuilder::new()
            .with_screen(CurrentScreen::TagValues)
            .build();
        app.view.tag_values = TagValues::from(vec![
            TagValue::new(
                "T1",
                Some(OpcValue::String("V1".into())),
                OpcQuality::GOOD,
                Some(std::time::SystemTime::UNIX_EPOCH),
            ),
            TagValue::new(
                "T2",
                Some(OpcValue::String("V2".into())),
                OpcQuality::GOOD,
                Some(std::time::SystemTime::UNIX_EPOCH),
            ),
        ]);
        app.view.selected_index = Some(0);

        app.select_next();
        assert_eq!(app.view.selected_index, Some(1));

        app.select_next();
        assert_eq!(app.view.selected_index, Some(1));
    }

    #[test]
    fn test_page_down_basic() {
        let tags: Vec<String> = (0..50).map(|i| format!("T{i}")).collect();
        let mut app = TestAppBuilder::new()
            .with_screen(CurrentScreen::TagList)
            .with_tags(tags)
            .build();

        app.page_down();
        assert_eq!(app.view.selected_index, Some(20));

        app.page_down();
        assert_eq!(app.view.selected_index, Some(40));

        app.page_down();
        assert_eq!(app.view.selected_index, Some(49));
    }

    #[test]
    fn test_page_up_basic() {
        let tags: Vec<String> = (0..50).map(|i| format!("T{i}")).collect();
        let mut app = TestAppBuilder::new()
            .with_screen(CurrentScreen::TagList)
            .with_tags(tags)
            .build();
        app.view.selected_index = Some(49);

        app.page_up();
        assert_eq!(app.view.selected_index, Some(29));

        app.page_up();
        assert_eq!(app.view.selected_index, Some(9));

        app.page_up();
        assert_eq!(app.view.selected_index, Some(0));
    }

    #[test]
    fn test_search_engine_zero_alloc_matching() {
        let tags = vec![
            "System.Cpu".into(),
            "System.Mem".into(),
            "User.Data".into(),
            "User.Settings".into(),
        ];
        let mut app = TestAppBuilder::new()
            .with_screen(CurrentScreen::TagList)
            .with_tags(tags)
            .build();

        app.enter_search_mode();
        assert!(app.search.search_mode);

        app.update_search_query('s');
        app.update_search_query('y');
        app.update_search_query('s');

        assert_eq!(app.search.search_matches.len(), 2);
        assert_eq!(app.search.search_matches[0], 0);
        assert_eq!(app.search.search_matches[1], 1);
        assert!(app.search.is_match(0));
        assert!(app.search.is_match(1));
        assert!(!app.search.is_match(2));
        assert_eq!(app.view.selected_index, Some(0));

        app.next_search_match();
        assert_eq!(app.view.selected_index, Some(1));

        app.next_search_match();
        assert_eq!(app.view.selected_index, Some(0));

        app.search_backspace();
        assert_eq!(app.search.search_matches.len(), 2);

        app.exit_search_mode();
        assert!(!app.search.search_mode);
    }

    #[test]
    fn test_poll_write_result_failure() {
        let mut app = test_app();
        let (tx, rx) = oneshot::channel();
        app.tasks.write_result_rx = Some(rx);

        let failed_res = WriteResult::failure(
            "Channel1.Device1.Tag1",
            OpcError::Connection("Lost connection".into()),
        );
        let _ = tx.send(Ok(failed_res));

        app.poll_write_result();
        assert!(app.tasks.write_result_rx.is_none());
        assert!(
            app.view
                .messages
                .iter()
                .any(|m| m.contains("failed: Connection failed: Lost connection"))
        );
    }

    #[test]
    fn test_poll_write_result_success() {
        let mut app = test_app();
        let (tx, rx) = oneshot::channel();
        app.tasks.write_result_rx = Some(rx);

        let ok_res = WriteResult::success("Channel1.Device1.Tag1");
        let _ = tx.send(Ok(ok_res));

        app.poll_write_result();
        assert!(app.tasks.write_result_rx.is_none());
        assert!(app.view.messages.iter().any(|m| m.contains("succeeded")));
    }

    #[test]
    fn test_destructure_tag_value_ergonomics() {
        use opc_da_client::{OpcQuality, OpcValueOptionExt, SystemTimeOptionExt};

        let tv = TagValue::new(
            "Plant.Sensor1",
            Some(OpcValue::Int(100)),
            OpcQuality::GOOD,
            None,
        );

        let TagValue {
            tag_id,
            quality,
            timestamp,
            ..
        } = &tv;

        let log_line = format!(
            "Tag: {} | Value: {} | Quality: {} | Timestamp: {}",
            tag_id,
            tv.value().display(),
            quality,
            timestamp.display_or("N/A")
        );

        assert_eq!(
            log_line,
            "Tag: Plant.Sensor1 | Value: 100 | Quality: Good | Timestamp: N/A"
        );
    }

    #[test]
    fn test_write_value_parsing_and_boolean_coercion() {
        let mut app = test_app();

        app.view.tag_values.push(TagValue::new(
            "Device.PumpRunning",
            Some(OpcValue::Bool(false)),
            OpcQuality::GOOD,
            None,
        ));

        app.view.tag_values.push(TagValue::new(
            "Device.SpeedRpm",
            Some(OpcValue::Int(1000)),
            OpcQuality::GOOD,
            None,
        ));

        assert_eq!("true".parse::<OpcValue>().unwrap(), OpcValue::Bool(true));
        assert_eq!("TRUE".parse::<OpcValue>().unwrap(), OpcValue::Bool(true));
        assert_eq!("false".parse::<OpcValue>().unwrap(), OpcValue::Bool(false));
        assert_eq!("FALSE".parse::<OpcValue>().unwrap(), OpcValue::Bool(false));
        assert_eq!("42".parse::<OpcValue>().unwrap(), OpcValue::Int(42));
        assert_eq!("2.5".parse::<OpcValue>().unwrap(), OpcValue::Float(2.5));
        assert_eq!(
            "Running".parse::<OpcValue>().unwrap(),
            OpcValue::String("Running".to_string())
        );

        assert_eq!(
            app.resolve_write_value("Device.PumpRunning", "1"),
            OpcValue::Bool(true)
        );
        assert_eq!(
            app.resolve_write_value("Device.PumpRunning", "0"),
            OpcValue::Bool(false)
        );
        assert_eq!(
            app.resolve_write_value("Device.SpeedRpm", "1"),
            OpcValue::Int(1)
        );
        assert_eq!(
            app.resolve_write_value("Device.SpeedRpm", "0"),
            OpcValue::Int(0)
        );
        assert_eq!(app.resolve_write_value("UnknownTag", "1"), OpcValue::Int(1));
    }

    #[test]
    fn test_select_all_and_clear_selection() {
        let mut app = TestAppBuilder::new()
            .with_screen(CurrentScreen::TagList)
            .with_tags(vec!["Tag1".into(), "Tag2".into()])
            .build();

        app.view.select_all();
        assert!(app.view.is_selected("Tag1"));
        assert!(app.view.is_selected("Tag2"));

        app.view.clear_selection();
        assert!(!app.view.is_selected("Tag1"));
        assert!(!app.view.is_selected("Tag2"));
    }

    #[tokio::test]
    async fn test_task_manager_abort_and_clear() {
        let mut tasks = TaskManager::default();
        assert!(tasks.active_abort.is_none());
        let join = tokio::spawn(async {
            tokio::time::sleep(std::time::Duration::from_secs(10)).await;
        });
        tasks.set_active_task(join.abort_handle());
        assert!(tasks.active_abort.is_some());
        tasks.clear_active_task();
        assert!(tasks.active_abort.is_none());
    }

    #[test]
    fn test_app_handle_key_action() {
        use crossterm::event::{KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};

        let mut app = test_app();
        assert_eq!(app.nav.current_screen, CurrentScreen::Home);

        // Press 'x' on Home -> Continue
        let key_x = KeyEvent {
            code: KeyCode::Char('x'),
            modifiers: KeyModifiers::empty(),
            kind: KeyEventKind::Press,
            state: KeyEventState::empty(),
        };
        let action = app.handle_key(key_x);
        assert_eq!(action, AppAction::Continue);
        assert!(app.nav.host_input.ends_with('x'));

        // Press Esc on Home -> Quit
        let key_esc = KeyEvent {
            code: KeyCode::Esc,
            modifiers: KeyModifiers::empty(),
            kind: KeyEventKind::Press,
            state: KeyEventState::empty(),
        };
        let action = app.handle_key(key_esc);
        assert_eq!(action, AppAction::Quit);
        assert_eq!(app.nav.current_screen, CurrentScreen::Exiting);

        // Release event -> Continue without effect
        let key_release = KeyEvent {
            code: KeyCode::Char('y'),
            modifiers: KeyModifiers::empty(),
            kind: KeyEventKind::Release,
            state: KeyEventState::empty(),
        };
        assert_eq!(app.handle_key(key_release), AppAction::Continue);
    }
}
