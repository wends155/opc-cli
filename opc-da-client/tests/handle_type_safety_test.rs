#![allow(deprecated)]

use opc_da_client::{ClientItemHandle, GroupHandle, ItemHandle, ServerItemHandle};

#[test]
fn test_handle_type_safety_representations() {
    let client_h = ClientItemHandle::new(101);
    let server_h = ServerItemHandle::new(202);
    let group_h = GroupHandle::new(303);

    assert_eq!(client_h.as_raw(), 101);
    assert_eq!(server_h.as_raw(), 202);
    assert_eq!(group_h.as_raw(), 303);

    assert_eq!(u32::from(client_h), 101);
    assert_eq!(u32::from(server_h), 202);
    assert_eq!(u32::from(group_h), 303);

    assert_eq!(format!("{client_h}"), "101");
    assert_eq!(format!("{server_h}"), "202");
    assert_eq!(format!("{group_h}"), "303");
}

#[test]
#[allow(deprecated)]
fn test_legacy_item_handle_alias_compatibility() {
    let legacy_h: ItemHandle = ItemHandle::new(999);
    let server_h: ServerItemHandle = legacy_h;
    assert_eq!(server_h.as_raw(), 999);
}

fn requires_server_handle(handle: ServerItemHandle) -> u32 {
    handle.as_raw()
}

#[test]
fn test_type_enforcement_accepts_server_handle() {
    let server_h = ServerItemHandle::new(55);
    assert_eq!(requires_server_handle(server_h), 55);
}
