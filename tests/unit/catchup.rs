use crate::db::entities::programs;
use crate::indexer::transactions::catch_up_floor;

use super::support::{pubkey, ts};

fn program(last_processed_slot: Option<i64>, start_slot: Option<i64>) -> programs::Model {
    programs::Model {
        id: 1,
        rpc_id: 1,
        address: pubkey(3).to_bytes().to_vec(),
        name: None,
        last_processed_at: None,
        last_processed_slot,
        start_slot,
        active: true,
        track_account_states: true,
        account_filters: None,
        created_at: ts(),
        updated_at: ts(),
    }
}

#[test]
fn floor_defaults_to_the_program_cursor() {
    assert_eq!(catch_up_floor(&program(Some(900), Some(100)), None), 900);
}

#[test]
fn a_fresh_program_walks_from_its_start_slot() {
    assert_eq!(catch_up_floor(&program(None, Some(100)), None), 100);
    assert_eq!(catch_up_floor(&program(None, None), None), 0);
}

#[test]
fn since_raises_the_floor_above_the_cursor() {
    assert_eq!(
        catch_up_floor(&program(Some(900), Some(100)), Some(1500)),
        1500
    );
}

#[test]
fn since_never_reaches_below_the_start_slot() {
    assert_eq!(
        catch_up_floor(&program(Some(2400), Some(2000)), Some(1500)),
        2000
    );
}
