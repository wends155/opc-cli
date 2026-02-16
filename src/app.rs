use crate::opc_impl;
use crate::traits::OpcProvider;
use anyhow::Result;
use ratatui::widgets::{ListState, TableState}; // Added TableState
use std::sync::atomic::AtomicUsize;
use std::sync::Arc;
use tokio::sync::oneshot;

/// Default timeout for OPC operations (server listing and tag browsing).
const OPC_TIMEOUT_SECS: u64 = 30;

/// A single tag's read result for display.
#[derive(Debug, Clone)]
pub struct TagValue {
    pub tag_id: String,
    pub value: String,
    pub quality: String,
    pub timestamp: String,
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum CurrentScreen {
    Home,
    Loading,
    ServerList,
    TagList,
    TagValues,
    Exiting,
}

pub struct App {
    pub host_input: String,
    pub servers: Vec<String>,
    pub tags: Vec<String>,
    pub selected_index: Option<usize>,
    pub current_screen: CurrentScreen,
    pub opc_provider: Arc<dyn OpcProvider>,
    pub messages: Vec<String>,
    pub list_state: ListState,
    pub table_state: TableState, // New field
    pub browse_progress: Arc<AtomicUsize>,
    pub browse_result_rx: Option<oneshot::Receiver<Result<Vec<String>>>>,
    pub fetch_result_rx: Option<oneshot::Receiver<Result<Vec<String>>>>,
    pub selected_tags: Vec<bool>,
    pub tag_values: Vec<TagValue>,
    pub read_result_rx: Option<oneshot::Receiver<Result<Vec<TagValue>>>>,
}

impl App {
    pub fn new(opc_provider: Arc<dyn OpcProvider>) -> Self {
        Self {
            host_input: "localhost".into(),
            servers: Vec::new(),
            tags: Vec::new(),
            selected_index: None,
            current_screen: CurrentScreen::Home,
            opc_provider,
            messages: Vec::new(),
            list_state: ListState::default(),
            table_state: TableState::default(), // Initialize
            browse_progress: Arc::new(AtomicUsize::new(0)),
            browse_result_rx: None,
            fetch_result_rx: None,
            selected_tags: Vec::new(),
            tag_values: Vec::new(),
            read_result_rx: None,
        }
    }

    pub fn add_message(&mut self, message: String) {
        self.messages.push(message);
        if self.messages.len() > 10 {
            self.messages.remove(0);
        }
    }

    // Actions
    pub fn start_fetch_servers(&mut self) {
        let host = self.host_input.clone();
        self.current_screen = CurrentScreen::Loading;
        self.add_message(format!("Connecting to {}...", host));

        let provider = Arc::clone(&self.opc_provider);
        let (tx, rx) = oneshot::channel();

        tokio::spawn(async move {
            let result = tokio::time::timeout(
                std::time::Duration::from_secs(OPC_TIMEOUT_SECS),
                provider.list_servers(&host),
            )
            .await;

            let final_result = match result {
                Ok(inner) => inner,
                Err(_) => {
                    tracing::error!("Server listing timed out ({}s)", OPC_TIMEOUT_SECS);
                    Err(anyhow::anyhow!(
                        "Connection timed out ({}s)",
                        OPC_TIMEOUT_SECS
                    ))
                }
            };

            let _ = tx.send(final_result);
        });

        self.fetch_result_rx = Some(rx);
    }

    pub fn poll_fetch_result(&mut self) {
        if let Some(rx) = &mut self.fetch_result_rx {
            match rx.try_recv() {
                Ok(Ok(servers)) => {
                    self.servers = servers;
                    self.current_screen = CurrentScreen::ServerList;
                    if self.servers.is_empty() {
                        self.selected_index = None;
                        self.list_state.select(None);
                    } else {
                        self.selected_index = Some(0);
                        self.list_state.select(Some(0));
                    }
                    self.add_message(format!(
                        "Found {} servers on {}",
                        self.servers.len(),
                        self.host_input
                    ));
                    self.fetch_result_rx = None;
                }
                Ok(Err(e)) => {
                    self.current_screen = CurrentScreen::Home;
                    tracing::error!(error = %e, "Failed to fetch servers");
                    self.add_message(format!("Error fetching servers: {}", e));
                    self.fetch_result_rx = None;
                }
                Err(oneshot::error::TryRecvError::Empty) => {
                    // Still running
                }
                Err(oneshot::error::TryRecvError::Closed) => {
                    self.current_screen = CurrentScreen::Home;
                    tracing::error!(
                        "Server listing background task terminated unexpectedly (sender dropped)"
                    );
                    self.add_message("Server listing task terminated unexpectedly".into());
                    self.fetch_result_rx = None;
                }
            }
        }
    }

    pub fn select_next(&mut self) {
        let count = match self.current_screen {
            CurrentScreen::ServerList => self.servers.len(),
            CurrentScreen::TagList => self.tags.len(),
            CurrentScreen::TagValues => self.tag_values.len(),
            _ => 0,
        };

        if count == 0 {
            return;
        }

        if let Some(idx) = self.selected_index {
            if idx < count - 1 {
                let new_idx = idx + 1;
                self.selected_index = Some(new_idx);
                self.list_state.select(Some(new_idx));
            }
        } else {
            self.selected_index = Some(0);
            self.list_state.select(Some(0));
        }
    }

    pub fn select_prev(&mut self) {
        if let Some(idx) = self.selected_index {
            if idx > 0 {
                let new_idx = idx - 1;
                self.selected_index = Some(new_idx);
                self.list_state.select(Some(new_idx));
            }
        }
    }

    pub fn start_browse_tags(&mut self) {
        if self.current_screen != CurrentScreen::ServerList {
            return;
        }

        let idx = match self.selected_index {
            Some(i) => i,
            None => return,
        };

        let server = match self.servers.get(idx) {
            Some(s) => s.clone(),
            None => return,
        };

        self.current_screen = CurrentScreen::Loading;
        self.browse_progress = Arc::new(AtomicUsize::new(0));
        self.add_message(format!("Browsing tags for {}...", server));

        let provider = Arc::clone(&self.opc_provider);
        let progress = Arc::clone(&self.browse_progress);
        let (tx, rx) = oneshot::channel();

        tokio::spawn(async move {
            let result = tokio::time::timeout(
                std::time::Duration::from_secs(OPC_TIMEOUT_SECS),
                provider.browse_tags(&server, 500, progress),
            )
            .await;

            let final_result = match result {
                Ok(inner) => inner,
                Err(_) => {
                    tracing::error!(
                        server = %server,
                        timeout_secs = OPC_TIMEOUT_SECS,
                        "Browse tags timed out — server may be hung during DCOM activation or browse walk"
                    );
                    Err(anyhow::anyhow!(
                        "Browse timed out ({}s) for '{}' — check logs for phase details",
                        OPC_TIMEOUT_SECS,
                        server
                    ))
                }
            };

            let _ = tx.send(final_result);
        });

        self.browse_result_rx = Some(rx);
    }

    pub fn poll_browse_result(&mut self) {
        if let Some(rx) = &mut self.browse_result_rx {
            match rx.try_recv() {
                Ok(Ok(tags)) => {
                    self.tags = tags;
                    self.selected_tags = vec![false; self.tags.len()];
                    self.current_screen = CurrentScreen::TagList;
                    if self.tags.is_empty() {
                        self.selected_index = None;
                        self.list_state.select(None);
                    } else {
                        self.selected_index = Some(0);
                        self.list_state.select(Some(0));
                    }
                    self.add_message(format!("Found {} tags", self.tags.len()));
                    self.browse_result_rx = None;
                }
                Ok(Err(e)) => {
                    self.current_screen = CurrentScreen::ServerList;
                    tracing::error!(error = %e, error_chain = ?e, "Browse tags failed");
                    let hint = opc_impl::friendly_com_hint(&e);
                    let msg = match hint {
                        Some(h) => format!("Error: {} ({})", h, e),
                        None => format!("Error: {:#}", e),
                    };
                    self.add_message(msg);
                    self.browse_result_rx = None;
                }
                Err(oneshot::error::TryRecvError::Empty) => {
                    // Still running
                }
                Err(oneshot::error::TryRecvError::Closed) => {
                    self.current_screen = CurrentScreen::ServerList;
                    tracing::error!(
                        "Browse background task terminated unexpectedly (sender dropped)"
                    );
                    self.add_message("Browse task terminated unexpectedly".into());
                    self.browse_result_rx = None;
                }
            }
        }
    }

    /// Toggle tag selection at the current selected index.
    pub fn toggle_tag_selection(&mut self) {
        if self.current_screen != CurrentScreen::TagList {
            return;
        }
        if let Some(idx) = self.selected_index {
            if idx < self.selected_tags.len() {
                self.selected_tags[idx] = !self.selected_tags[idx];
            }
        }
    }

    /// Start reading values for selected tags.
    pub fn start_read_values(&mut self) {
        if self.current_screen != CurrentScreen::TagList {
            return;
        }

        // Gather selected tag IDs
        let selected_tag_ids: Vec<String> = self
            .tags
            .iter()
            .enumerate()
            .filter_map(|(idx, tag_id)| {
                if self.selected_tags.get(idx).copied().unwrap_or(false) {
                    Some(tag_id.clone())
                } else {
                    None
                }
            })
            .collect();

        if selected_tag_ids.is_empty() {
            self.add_message("No tags selected. Press Space to select tags.".into());
            return;
        }

        let server = match self.selected_index.and_then(|_| {
            self.servers
                .iter()
                .find(|s| self.tags.iter().any(|t| t.contains(*s)))
        }) {
            Some(s) => s.clone(),
            None => {
                // Fallback: get from the currently browsed server
                match self.servers.first() {
                    Some(s) => s.clone(),
                    None => {
                        self.add_message("No server available for reading".into());
                        return;
                    }
                }
            }
        };

        self.current_screen = CurrentScreen::Loading;
        self.add_message(format!("Reading {} tag values...", selected_tag_ids.len()));

        let provider = Arc::clone(&self.opc_provider);
        let (tx, rx) = oneshot::channel();

        tokio::spawn(async move {
            let result = tokio::time::timeout(
                std::time::Duration::from_secs(OPC_TIMEOUT_SECS),
                provider.read_tag_values(&server, selected_tag_ids),
            )
            .await;

            let final_result = match result {
                Ok(inner) => inner,
                Err(_) => {
                    tracing::error!("Read tag values timed out ({}s)", OPC_TIMEOUT_SECS);
                    Err(anyhow::anyhow!("Read timed out ({}s)", OPC_TIMEOUT_SECS))
                }
            };

            let _ = tx.send(final_result);
        });

        self.read_result_rx = Some(rx);
    }

    pub fn poll_read_result(&mut self) {
        if let Some(rx) = &mut self.read_result_rx {
            match rx.try_recv() {
                Ok(Ok(values)) => {
                    self.tag_values = values;
                    self.current_screen = CurrentScreen::TagValues;
                    if self.tag_values.is_empty() {
                        self.selected_index = None;
                        self.list_state.select(None);
                    } else {
                        self.selected_index = Some(0);
                        self.list_state.select(Some(0));
                    }
                    self.add_message(format!("Read {} tag values", self.tag_values.len()));
                    self.read_result_rx = None;
                }
                Ok(Err(e)) => {
                    self.current_screen = CurrentScreen::TagList;
                    tracing::error!(error = %e, error_chain = ?e, "Read tag values failed");
                    let hint = opc_impl::friendly_com_hint(&e);
                    let msg = match hint {
                        Some(h) => format!("Error reading values: {} ({})", h, e),
                        None => format!("Error reading values: {:#}", e),
                    };
                    self.add_message(msg);
                    self.read_result_rx = None;
                }
                Err(oneshot::error::TryRecvError::Empty) => {
                    // Still running
                }
                Err(oneshot::error::TryRecvError::Closed) => {
                    self.current_screen = CurrentScreen::TagList;
                    tracing::error!(
                        "Read values background task terminated unexpectedly (sender dropped)"
                    );
                    self.add_message("Read task terminated unexpectedly".into());
                    self.read_result_rx = None;
                }
            }
        }
    }

    pub fn go_back(&mut self) {
        match self.current_screen {
            CurrentScreen::ServerList => {
                self.current_screen = CurrentScreen::Home;
                self.servers.clear();
                self.selected_index = None;
                self.list_state.select(None);
            }
            CurrentScreen::TagList => {
                self.current_screen = CurrentScreen::ServerList;
                self.tags.clear();
                // Restore selection to the previous server if possible
                if !self.servers.is_empty() {
                    self.selected_index = Some(0); // Simple fallback for now
                    self.list_state.select(Some(0));
                }
            }
            CurrentScreen::TagValues => {
                self.current_screen = CurrentScreen::TagList;
                self.tag_values.clear();
                // Restore selection to tags list
                if !self.tags.is_empty() {
                    self.selected_index = Some(0);
                    self.list_state.select(Some(0));
                } else {
                    self.selected_index = None;
                    self.list_state.select(None);
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::MockOpcProvider;
    use mockall::predicate::*;

    #[test]
    fn test_poll_fetch_result_success() {
        let (tx, rx) = oneshot::channel();
        let mock = MockOpcProvider::new();
        let mut app = App::new(Arc::new(mock));
        app.current_screen = CurrentScreen::Loading;
        app.fetch_result_rx = Some(rx);

        tx.send(Ok(vec!["Server1".into(), "Server2".into()]))
            .unwrap();
        app.poll_fetch_result();

        assert_eq!(app.current_screen, CurrentScreen::ServerList);
        assert_eq!(app.servers.len(), 2);
        assert_eq!(app.selected_index, Some(0));
        assert!(app.fetch_result_rx.is_none());
        assert!(app.messages.last().unwrap().contains("Found 2 servers"));
    }

    #[test]
    fn test_poll_fetch_result_error() {
        let (tx, rx) = oneshot::channel();
        let mock = MockOpcProvider::new();
        let mut app = App::new(Arc::new(mock));
        app.current_screen = CurrentScreen::Loading;
        app.fetch_result_rx = Some(rx);

        tx.send(Err(anyhow::anyhow!("Connection failed"))).unwrap();
        app.poll_fetch_result();

        assert_eq!(app.current_screen, CurrentScreen::Home);
        assert!(app.fetch_result_rx.is_none());
        assert!(app.messages.last().unwrap().contains("Error"));
    }

    #[test]
    fn test_poll_fetch_result_empty_servers() {
        let (tx, rx) = oneshot::channel();
        let mock = MockOpcProvider::new();
        let mut app = App::new(Arc::new(mock));
        app.current_screen = CurrentScreen::Loading;
        app.fetch_result_rx = Some(rx);

        tx.send(Ok(vec![])).unwrap();
        app.poll_fetch_result();

        assert_eq!(app.current_screen, CurrentScreen::ServerList);
        assert!(app.servers.is_empty());
        assert_eq!(app.selected_index, None);
        assert!(app.messages.last().unwrap().contains("Found 0 servers"));
    }

    #[test]
    fn test_poll_fetch_result_closed() {
        let (tx, rx) = oneshot::channel::<Result<Vec<String>>>();
        let mock = MockOpcProvider::new();
        let mut app = App::new(Arc::new(mock));
        app.current_screen = CurrentScreen::Loading;
        app.fetch_result_rx = Some(rx);

        // Drop the sender
        drop(tx);
        app.poll_fetch_result();

        assert_eq!(app.current_screen, CurrentScreen::Home);
        assert!(app
            .messages
            .last()
            .unwrap()
            .contains("terminated unexpectedly"));
    }

    #[tokio::test]
    async fn test_start_fetch_servers_sets_loading() {
        let mut mock = MockOpcProvider::new();
        mock.expect_list_servers()
            .returning(|_| Ok(vec!["S1".into()]));

        let mut app = App::new(Arc::new(mock));
        app.start_fetch_servers();

        assert_eq!(app.current_screen, CurrentScreen::Loading);
        assert!(app.fetch_result_rx.is_some());
    }

    #[test]
    fn test_server_navigation() {
        let mock = MockOpcProvider::new();
        let mut app = App {
            host_input: "localhost".into(),
            servers: vec!["S1".into(), "S2".into()],
            selected_index: Some(0),
            tags: vec![],
            current_screen: CurrentScreen::ServerList,
            opc_provider: Arc::new(mock),
            messages: vec![],
            list_state: ListState::default(),
            browse_progress: Arc::new(AtomicUsize::new(0)),
            browse_result_rx: None,
            fetch_result_rx: None,
            selected_tags: vec![],
            tag_values: vec![],
            read_result_rx: None,
            table_state: TableState::default(),
        };
        app.list_state.select(Some(0));

        app.select_next();
        assert_eq!(app.selected_index, Some(1));

        app.select_next(); // Should stay at 1
        assert_eq!(app.selected_index, Some(1));

        app.select_prev();
        assert_eq!(app.selected_index, Some(0));

        app.select_prev(); // Should stay at 0
        assert_eq!(app.selected_index, Some(0));
    }

    #[test]
    fn test_tag_navigation_logic() {
        let mock = MockOpcProvider::new();
        let mut app = App {
            host_input: "localhost".into(),
            servers: vec!["S1".into()],
            selected_index: Some(0),
            tags: vec!["T1".into(), "T2".into()],
            current_screen: CurrentScreen::TagList,
            opc_provider: Arc::new(mock),
            messages: vec![],
            list_state: ListState::default(),
            browse_progress: Arc::new(AtomicUsize::new(0)),
            browse_result_rx: None,
            fetch_result_rx: None,
            selected_tags: vec![],
            tag_values: vec![],
            read_result_rx: None,
            table_state: TableState::default(),
        };
        app.list_state.select(Some(0));

        // Test boundary check against tags (2), not servers (1)
        app.select_next();
        assert_eq!(app.selected_index, Some(1));
        assert_eq!(app.list_state.selected(), Some(1));

        app.select_next(); // Should stay at 1
        assert_eq!(app.selected_index, Some(1));
    }

    #[tokio::test]
    async fn test_enter_selected_server_navigation() {
        let mut mock = MockOpcProvider::new();
        mock.expect_browse_tags()
            .with(eq("S1"), eq(500), always())
            .returning(|_, _, _| Ok(vec!["T1".into()]));

        let mut app = App {
            host_input: "localhost".into(),
            servers: vec!["S1".into()],
            selected_index: Some(0),
            tags: vec![],
            current_screen: CurrentScreen::ServerList,
            opc_provider: Arc::new(mock),
            messages: vec![],
            list_state: ListState::default(),
            browse_progress: Arc::new(AtomicUsize::new(0)),
            browse_result_rx: None,
            fetch_result_rx: None,
            selected_tags: vec![],
            tag_values: vec![],
            read_result_rx: None,
            table_state: TableState::default(),
        };
        app.list_state.select(Some(0));

        app.start_browse_tags();
        // Wait briefly for the spawned task
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        app.poll_browse_result();

        assert!(matches!(app.current_screen, CurrentScreen::TagList));
        assert_eq!(app.tags.len(), 1);
        assert_eq!(app.selected_index, Some(0));
    }

    #[test]
    fn test_go_back_navigation() {
        let mock = MockOpcProvider::new();
        let mut app = App {
            host_input: "localhost".into(),
            servers: vec!["S1".into()],
            selected_index: Some(0),
            tags: vec!["T1".into()],
            current_screen: CurrentScreen::TagList,
            opc_provider: Arc::new(mock),
            messages: vec![],
            list_state: ListState::default(),
            browse_progress: Arc::new(AtomicUsize::new(0)),
            browse_result_rx: None,
            fetch_result_rx: None,
            selected_tags: vec![],
            tag_values: vec![],
            read_result_rx: None,
            table_state: TableState::default(),
        };
        app.list_state.select(Some(0));

        // TagList -> ServerList
        app.go_back();
        assert!(matches!(app.current_screen, CurrentScreen::ServerList));
        assert!(app.tags.is_empty());
        assert_eq!(app.selected_index, Some(0));

        // ServerList -> Home
        app.go_back();
        assert!(matches!(app.current_screen, CurrentScreen::Home));
        assert!(app.servers.is_empty());
        assert_eq!(app.selected_index, None);
    }

    #[tokio::test]
    async fn test_loading_transition() {
        let mock = MockOpcProvider::new();
        let mut app = App::new(Arc::new(mock));
        app.start_fetch_servers();
        assert_eq!(app.current_screen, CurrentScreen::Loading);
        assert!(app.messages.iter().any(|m| m.contains("Connecting to")));
    }

    #[tokio::test]
    async fn test_tui_navigation_flow() {
        let (tx, rx) = oneshot::channel();
        let mock = MockOpcProvider::new();
        let mut app = App::new(Arc::new(mock));

        // 1. Initial State: Home
        assert!(matches!(app.current_screen, CurrentScreen::Home));
        assert_eq!(app.host_input, "localhost");

        // 2. Start fetch
        app.start_fetch_servers();
        assert_eq!(app.current_screen, CurrentScreen::Loading);
        app.fetch_result_rx = Some(rx);

        // 3. Complete fetch
        tx.send(Ok(vec!["Server1".into()])).unwrap();
        app.poll_fetch_result();

        assert!(matches!(app.current_screen, CurrentScreen::ServerList));
        assert_eq!(app.servers.len(), 1);
        assert_eq!(app.selected_index, Some(0));
        assert_eq!(app.list_state.selected(), Some(0));

        // 4. User goes back to Home
        app.go_back();
        assert!(matches!(app.current_screen, CurrentScreen::Home));
        assert!(app.servers.is_empty());
        assert_eq!(app.selected_index, None);
        assert_eq!(app.list_state.selected(), None);
    }

    #[tokio::test]
    async fn test_poll_browse_result_error_shows_message() {
        let (tx, rx) = oneshot::channel();
        let mock = MockOpcProvider::new();
        let mut app = App::new(Arc::new(mock));
        app.current_screen = CurrentScreen::Loading;
        app.browse_result_rx = Some(rx);

        // Simulate provider returning a descriptive error
        tx.send(Err(anyhow::anyhow!("DCOM access denied on remote host")))
            .unwrap();

        app.poll_browse_result();

        assert_eq!(app.current_screen, CurrentScreen::ServerList);
        assert!(app.browse_result_rx.is_none());
        let last_msg = app.messages.last().unwrap();
        assert!(last_msg.contains("Error: "));
        assert!(last_msg.contains("DCOM access denied")); // Error context preserved
    }

    #[tokio::test]
    async fn test_poll_browse_result_closed_shows_message() {
        let (tx, rx) = oneshot::channel();
        let mock = MockOpcProvider::new();
        let mut app = App::new(Arc::new(mock));
        app.current_screen = CurrentScreen::Loading;
        app.browse_result_rx = Some(rx);

        // Drop sender without sending — simulates task panic
        drop(tx);

        app.poll_browse_result();

        assert_eq!(app.current_screen, CurrentScreen::ServerList);
        assert!(app.browse_result_rx.is_none());
        let last_msg = app.messages.last().unwrap();
        assert!(last_msg.contains("terminated unexpectedly"));
    }

    #[tokio::test]
    async fn test_poll_browse_result_empty_tags() {
        let (tx, rx) = oneshot::channel();
        let mock = MockOpcProvider::new();
        let mut app = App::new(Arc::new(mock));
        app.current_screen = CurrentScreen::Loading;
        app.browse_result_rx = Some(rx);

        tx.send(Ok(vec![])).unwrap();

        app.poll_browse_result();

        assert_eq!(app.current_screen, CurrentScreen::TagList);
        assert!(app.tags.is_empty());
        assert_eq!(app.selected_index, None);
        assert_eq!(app.list_state.selected(), None);
        assert!(app.messages.last().unwrap().contains("Found 0 tags"));
    }

    #[test]
    fn test_start_browse_no_selection() {
        let mock = MockOpcProvider::new();
        let mut app = App::new(Arc::new(mock));
        app.current_screen = CurrentScreen::ServerList;
        app.servers = vec!["S1".into()];
        app.selected_index = None; // No selection

        app.start_browse_tags();

        // Should remain on ServerList — no crash, no Loading transition
        assert_eq!(app.current_screen, CurrentScreen::ServerList);
        assert!(app.browse_result_rx.is_none());
    }

    #[test]
    fn test_start_browse_wrong_screen() {
        let mock = MockOpcProvider::new();
        let mut app = App::new(Arc::new(mock));
        app.current_screen = CurrentScreen::Home; // Wrong screen
        app.servers = vec!["S1".into()];
        app.selected_index = Some(0);

        app.start_browse_tags();

        assert_eq!(app.current_screen, CurrentScreen::Home); // Unchanged
        assert!(app.browse_result_rx.is_none());
    }

    #[test]
    fn test_poll_fetch_result_timeout() {
        let (tx, rx) = oneshot::channel();
        let mock = MockOpcProvider::new();
        let mut app = App::new(Arc::new(mock));
        app.current_screen = CurrentScreen::Loading;
        app.fetch_result_rx = Some(rx);

        tx.send(Err(anyhow::anyhow!("Connection timed out (30s)")))
            .unwrap();
        app.poll_fetch_result();

        assert_eq!(app.current_screen, CurrentScreen::Home);
        assert!(app.messages.last().unwrap().contains("timed out"));
    }

    #[test]
    fn test_add_message_ring_buffer() {
        let mock = MockOpcProvider::new();
        let mut app = App::new(Arc::new(mock));

        for i in 0..15 {
            app.add_message(format!("msg-{}", i));
        }

        assert_eq!(app.messages.len(), 10); // Capped at 10
        assert_eq!(app.messages[0], "msg-5"); // Oldest surviving
        assert_eq!(app.messages[9], "msg-14"); // Latest
    }

    #[test]
    fn test_select_on_empty_list() {
        let mock = MockOpcProvider::new();
        let mut app = App::new(Arc::new(mock));
        app.current_screen = CurrentScreen::ServerList;
        app.servers = vec![]; // Empty

        app.select_next();
        assert_eq!(app.selected_index, None);

        app.select_prev();
        assert_eq!(app.selected_index, None);
    }

    #[test]
    fn test_poll_browse_result_no_task() {
        let mock = MockOpcProvider::new();
        let mut app = App::new(Arc::new(mock));
        app.current_screen = CurrentScreen::ServerList;

        // No browse_result_rx set — should not panic
        app.poll_browse_result();

        assert_eq!(app.current_screen, CurrentScreen::ServerList);
    }

    #[test]
    fn test_toggle_tag_selection() {
        let mock = MockOpcProvider::new();
        let mut app = App::new(Arc::new(mock));
        app.current_screen = CurrentScreen::TagList;
        app.tags = vec!["Tag1".into(), "Tag2".into()];
        app.selected_tags = vec![false, false];
        app.selected_index = Some(1);

        app.toggle_tag_selection();
        assert_eq!(app.selected_tags, vec![false, true]);

        app.toggle_tag_selection();
        assert_eq!(app.selected_tags, vec![false, false]);
    }

    #[test]
    fn test_start_read_values_no_selection() {
        let mock = MockOpcProvider::new();
        let mut app = App::new(Arc::new(mock));
        app.current_screen = CurrentScreen::TagList;
        app.tags = vec!["Tag1".into()];
        app.selected_tags = vec![false];

        app.start_read_values();

        assert_eq!(app.current_screen, CurrentScreen::TagList);
        assert!(app.messages.last().unwrap().contains("No tags selected"));
        assert!(app.read_result_rx.is_none());
    }

    #[test]
    fn test_start_read_values_wrong_screen() {
        let mock = MockOpcProvider::new();
        let mut app = App::new(Arc::new(mock));
        app.current_screen = CurrentScreen::ServerList;

        app.start_read_values();

        assert_eq!(app.current_screen, CurrentScreen::ServerList);
        assert!(app.read_result_rx.is_none());
    }

    #[test]
    fn test_poll_read_result_success() {
        let (tx, rx) = oneshot::channel();
        let mock = MockOpcProvider::new();
        let mut app = App::new(Arc::new(mock));
        app.current_screen = CurrentScreen::Loading;
        app.read_result_rx = Some(rx);

        let values = vec![TagValue {
            tag_id: "Tag1".into(),
            value: "123".into(),
            quality: "Good".into(),
            timestamp: "Today".into(),
        }];

        tx.send(Ok(values)).unwrap();
        app.poll_read_result();

        assert_eq!(app.current_screen, CurrentScreen::TagValues);
        assert_eq!(app.tag_values.len(), 1);
        assert_eq!(app.tag_values[0].value, "123");
        assert!(app.read_result_rx.is_none());
    }

    #[test]
    fn test_poll_read_result_error() {
        let (tx, rx) = oneshot::channel();
        let mock = MockOpcProvider::new();
        let mut app = App::new(Arc::new(mock));
        app.current_screen = CurrentScreen::Loading;
        app.read_result_rx = Some(rx);

        tx.send(Err(anyhow::anyhow!("Read failed"))).unwrap();
        app.poll_read_result();

        assert_eq!(app.current_screen, CurrentScreen::TagList);
        assert!(app.read_result_rx.is_none());
        assert!(app
            .messages
            .last()
            .unwrap()
            .contains("Error reading values"));
    }

    #[test]
    fn test_go_back_from_tag_values() {
        let mock = MockOpcProvider::new();
        let mut app = App::new(Arc::new(mock));
        app.current_screen = CurrentScreen::TagValues;
        app.tags = vec!["Tag1".into()];
        app.tag_values = vec![TagValue {
            tag_id: "Tag1".into(),
            value: "100".into(),
            quality: "Good".into(),
            timestamp: "".into(),
        }];

        app.go_back();

        assert_eq!(app.current_screen, CurrentScreen::TagList);
        assert!(app.tag_values.is_empty());
        assert_eq!(app.tags.len(), 1); // Tags preserved
    }

    #[test]
    fn test_select_next_on_tag_values() {
        let mock = MockOpcProvider::new();
        let mut app = App::new(Arc::new(mock));
        app.current_screen = CurrentScreen::TagValues;
        app.tag_values = vec![
            TagValue {
                tag_id: "T1".into(),
                value: "V1".into(),
                quality: "Q".into(),
                timestamp: "T".into(),
            },
            TagValue {
                tag_id: "T2".into(),
                value: "V2".into(),
                quality: "Q".into(),
                timestamp: "T".into(),
            },
        ];
        app.selected_index = Some(0);

        app.select_next();
        assert_eq!(app.selected_index, Some(1));

        app.select_next(); // Should stay at 1
        assert_eq!(app.selected_index, Some(1));
    }
}
