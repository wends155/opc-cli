use crate::opc_impl;
use crate::traits::OpcProvider;
use anyhow::Result;
use ratatui::widgets::ListState;
use std::sync::atomic::AtomicUsize;
use std::sync::Arc;
use tokio::sync::oneshot;

/// Default timeout for OPC operations (server listing and tag browsing).
const OPC_TIMEOUT_SECS: u64 = 30;

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum CurrentScreen {
    Home,
    Loading,
    ServerList,
    TagList,
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
    pub browse_progress: Arc<AtomicUsize>,
    pub browse_result_rx: Option<oneshot::Receiver<Result<Vec<String>>>>,
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
            browse_progress: Arc::new(AtomicUsize::new(0)),
            browse_result_rx: None,
        }
    }

    pub fn add_message(&mut self, message: String) {
        self.messages.push(message);
        if self.messages.len() > 10 {
            self.messages.remove(0);
        }
    }

    // Actions
    pub async fn fetch_servers(&mut self) {
        let host = self.host_input.clone();
        self.current_screen = CurrentScreen::Loading;
        self.add_message(format!("Connecting to {}...", host));

        let result = tokio::time::timeout(
            std::time::Duration::from_secs(OPC_TIMEOUT_SECS),
            self.opc_provider.list_servers(&host),
        )
        .await;

        match result {
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
                self.add_message(format!("Found {} servers on {}", self.servers.len(), host));
            }
            Ok(Err(e)) => {
                self.current_screen = CurrentScreen::Home;
                tracing::error!(error = %e, "Failed to fetch servers");
                self.add_message(format!("Error fetching servers: {}", e));
            }
            Err(_) => {
                self.current_screen = CurrentScreen::Home;
                tracing::error!("Server listing timed out ({}s)", OPC_TIMEOUT_SECS);
                self.add_message(format!(
                    "Connection timed out ({}s) while listing servers",
                    OPC_TIMEOUT_SECS
                ));
            }
        }
    }

    pub fn select_next(&mut self) {
        let count = match self.current_screen {
            CurrentScreen::ServerList => self.servers.len(),
            CurrentScreen::TagList => self.tags.len(),
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
                    tracing::error!(server = %server, "Browse tags timed out after {}s", OPC_TIMEOUT_SECS);
                    Err(anyhow::anyhow!("Browse timed out ({}s)", OPC_TIMEOUT_SECS))
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
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::MockOpcProvider;
    use mockall::predicate::*;

    #[tokio::test]
    async fn test_fetch_servers_success() {
        let mut mock = MockOpcProvider::new();
        mock.expect_list_servers()
            .with(eq("localhost"))
            .times(1)
            .returning(|_| Ok(vec!["Server1".into(), "Server2".into()]));

        let mut app = App::new(Arc::new(mock));
        app.fetch_servers().await;

        assert_eq!(app.servers.len(), 2);
        assert_eq!(app.servers[0], "Server1");
        assert!(matches!(app.current_screen, CurrentScreen::ServerList));
    }

    #[tokio::test]
    async fn test_fetch_servers_failure() {
        let mut mock = MockOpcProvider::new();
        mock.expect_list_servers()
            .returning(|_| Err(anyhow::anyhow!("Connection failed")));

        let mut app = App::new(Arc::new(mock));
        app.fetch_servers().await;

        assert_eq!(app.servers.len(), 0);
        assert!(!app.messages.is_empty());
        assert!(app
            .messages
            .last()
            .unwrap()
            .contains("Error fetching servers"));
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
        let mut mock = MockOpcProvider::new();
        mock.expect_list_servers()
            .returning(|_| Ok(vec!["Server1".into()]));

        let mut app = App::new(Arc::new(mock));
        app.fetch_servers().await;
        assert_eq!(app.current_screen, CurrentScreen::ServerList);
        assert!(app.messages.iter().any(|m| m.contains("Connecting to")));
    }

    #[tokio::test]
    async fn test_tui_navigation_flow() {
        let mut mock = MockOpcProvider::new();
        mock.expect_list_servers()
            .returning(|_| Ok(vec!["Server1".into()]));

        let mut app = App::new(Arc::new(mock));

        // 1. Initial State: Home
        assert!(matches!(app.current_screen, CurrentScreen::Home));
        assert_eq!(app.host_input, "localhost");

        // 2. Simulate User hitting Enter to fetch servers
        app.fetch_servers().await;
        assert!(matches!(app.current_screen, CurrentScreen::ServerList));
        assert_eq!(app.servers.len(), 1);
        assert_eq!(app.selected_index, Some(0));
        assert_eq!(app.list_state.selected(), Some(0));

        // 3. User goes back to Home
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

    #[tokio::test]
    async fn test_fetch_servers_empty_list() {
        let mut mock = MockOpcProvider::new();
        mock.expect_list_servers().returning(|_| Ok(vec![]));

        let mut app = App::new(Arc::new(mock));
        app.fetch_servers().await;

        assert_eq!(app.current_screen, CurrentScreen::ServerList);
        assert!(app.servers.is_empty());
        assert_eq!(app.selected_index, None);
        assert!(app.messages.last().unwrap().contains("Found 0 servers"));
    }

    #[tokio::test]
    async fn test_fetch_servers_error_preserves_context() {
        let mut mock = MockOpcProvider::new();
        mock.expect_list_servers()
            .returning(|_| Err(anyhow::anyhow!("RPC server is unavailable")));

        let mut app = App::new(Arc::new(mock));
        app.fetch_servers().await;

        assert_eq!(app.current_screen, CurrentScreen::Home);
        let last_msg = app.messages.last().unwrap();
        assert!(last_msg.contains("Error fetching servers"));
        assert!(last_msg.contains("RPC server is unavailable")); // Context preserved
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
}
