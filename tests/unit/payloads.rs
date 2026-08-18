use crate::routes::payloads::admin::{AddProgramPayload, UpdateProgramPayload};

#[test]
fn update_program_payload_is_empty_when_no_field_set() {
    let payload = UpdateProgramPayload {
        rpc_id: None,
        rpc_url: None,
        address: None,
        name: None,
        account_filters: None,
        scan_partition_offset: None,
    };
    assert!(payload.is_empty());
}

#[test]
fn update_program_payload_not_empty_with_any_field() {
    let with_rpc = UpdateProgramPayload {
        rpc_id: Some(1),
        rpc_url: None,
        address: None,
        name: None,
        account_filters: None,
        scan_partition_offset: None,
    };
    assert!(!with_rpc.is_empty());

    let with_address = UpdateProgramPayload {
        rpc_id: None,
        rpc_url: None,
        address: Some("addr".into()),
        name: None,
        account_filters: None,
        scan_partition_offset: None,
    };
    assert!(!with_address.is_empty());
}

#[test]
fn add_program_payload_requires_only_address() {
    let payload: AddProgramPayload =
        serde_json::from_value(serde_json::json!({"address": "somebase58"})).unwrap();
    assert_eq!(payload.address, "somebase58");
    assert!(payload.rpc_id.is_none());
    assert!(payload.rpc_url.is_none());
    assert!(payload.name.is_none());
    assert!(payload.start_slot.is_none());
}

#[test]
fn add_program_payload_missing_address_fails() {
    let result = serde_json::from_value::<AddProgramPayload>(serde_json::json!({"rpc_id": 1}));
    assert!(result.is_err(), "address is required");
}
