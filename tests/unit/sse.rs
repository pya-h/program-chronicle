use crate::core::sse::SseEvent;

use super::support::pubkey;

#[test]
fn account_changed_constructor_sets_all_fields() {
    let address = pubkey(1);
    let event = SseEvent::account_changed(7, address, 500, 3, vec![1, 2, 3]);
    match event {
        SseEvent::AccountChanged { program_id, address: got, slot, write_version, data } => {
            assert_eq!(program_id, 7);
            assert_eq!(got, address);
            assert_eq!(slot, 500);
            assert_eq!(write_version, 3);
            assert_eq!(data, vec![1, 2, 3]);
        }
        _ => panic!("expected an AccountChanged event"),
    }
}

#[test]
fn program_event_constructor_sets_all_fields() {
    let event = SseEvent::program_event(9, vec![0xaa; 64], 4, 600, vec![5, 6]);
    match event {
        SseEvent::ProgramEvent { program_id, tx_signature, log_index, slot, data } => {
            assert_eq!(program_id, 9);
            assert_eq!(tx_signature, vec![0xaa; 64]);
            assert_eq!(log_index, 4);
            assert_eq!(slot, 600);
            assert_eq!(data, vec![5, 6]);
        }
        _ => panic!("expected a ProgramEvent event"),
    }
}
