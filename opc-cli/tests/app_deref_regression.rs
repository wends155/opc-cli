use opc_cli::app::App;
use opc_da_client::MockOpcProvider;
use std::sync::Arc;

#[test]
fn test_app_does_not_deref_to_view_state() {
    let mock = MockOpcProvider::new();
    let app = App::new(Arc::new(mock));

    // View fields must be accessed via app.view explicitly
    assert!(app.view.servers.is_empty());
    assert!(app.view.tags.is_empty());
    assert!(app.view.selected_tags.is_empty());
    assert_eq!(app.view.selected_index, None);
    assert!(app.view.messages.is_empty());
}
