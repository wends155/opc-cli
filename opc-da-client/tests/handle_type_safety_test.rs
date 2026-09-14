use std::collections::HashSet;

use opc_da_client::{ClientGroupHandle, ClientItemHandle, ServerGroupHandle, ServerItemHandle};

#[test]
fn test_handle_type_safety_representations() {
    let client_g = ClientGroupHandle::new(101);
    let server_g = ServerGroupHandle::new(202);
    let client_i = ClientItemHandle::new(303);
    let server_i = ServerItemHandle::new(404);

    assert_eq!(client_g.as_raw(), 101);
    assert_eq!(server_g.as_raw(), 202);
    assert_eq!(client_i.as_raw(), 303);
    assert_eq!(server_i.as_raw(), 404);

    assert_eq!(u32::from(client_g), 101);
    assert_eq!(u32::from(server_g), 202);
    assert_eq!(u32::from(client_i), 303);
    assert_eq!(u32::from(server_i), 404);

    assert_eq!(ClientGroupHandle::from(101), client_g);
    assert_eq!(ServerGroupHandle::from(202), server_g);
    assert_eq!(ClientItemHandle::from(303), client_i);
    assert_eq!(ServerItemHandle::from(404), server_i);

    assert_eq!(format!("{client_g}"), "101");
    assert_eq!(format!("{server_g}"), "202");
    assert_eq!(format!("{client_i}"), "303");
    assert_eq!(format!("{server_i}"), "404");

    assert_eq!(ClientGroupHandle::default().as_raw(), 0);
    assert_eq!(ServerGroupHandle::default().as_raw(), 0);
    assert_eq!(ClientItemHandle::default().as_raw(), 0);
    assert_eq!(ServerItemHandle::default().as_raw(), 0);
}

fn requires_server_group(handle: ServerGroupHandle) -> u32 {
    handle.as_raw()
}

fn requires_server_item(handle: ServerItemHandle) -> u32 {
    handle.as_raw()
}

fn requires_client_group(handle: ClientGroupHandle) -> u32 {
    handle.as_raw()
}

fn requires_client_item(handle: ClientItemHandle) -> u32 {
    handle.as_raw()
}

#[test]
fn test_type_enforcement_accepts_correct_handles() {
    let server_g = ServerGroupHandle::new(11);
    let client_g = ClientGroupHandle::new(22);
    let server_i = ServerItemHandle::new(33);
    let client_i = ClientItemHandle::new(44);

    assert_eq!(requires_server_group(server_g), 11);
    assert_eq!(requires_client_group(client_g), 22);
    assert_eq!(requires_server_item(server_i), 33);
    assert_eq!(requires_client_item(client_i), 44);
}

#[test]
fn test_handle_hash_and_equality() {
    let mut set = HashSet::new();
    set.insert(ServerItemHandle::new(100));
    set.insert(ServerItemHandle::new(100));
    set.insert(ServerItemHandle::new(200));
    assert_eq!(set.len(), 2);
    assert!(set.contains(&ServerItemHandle::new(100)));
    assert!(!set.contains(&ServerItemHandle::new(300)));
}
