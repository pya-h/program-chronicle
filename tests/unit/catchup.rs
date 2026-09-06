use crate::db::entities::programs;
use crate::indexer::transactions::{catch_up_floor, resume_cursor};

use super::support::{pubkey, signature, ts};

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
        account_filters: None,
        scan_partition_offset: None,
        backfill_slot: None,
        backfill_cursor: None,
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

#[test]
fn no_frontier_means_the_walk_starts_at_the_chain_tip() {
    assert!(resume_cursor(&program(None, Some(100))).is_none());
}

#[test]
fn a_recorded_frontier_is_paged_on_from() {
    let mut model = program(None, Some(100));
    model.backfill_cursor = Some(signature(5).as_ref().to_vec());
    assert_eq!(resume_cursor(&model), Some(signature(5)));
}

#[test]
fn an_unreadable_frontier_falls_back_to_the_chain_tip() {
    let mut model = program(None, Some(100));
    model.backfill_cursor = Some(vec![1, 2, 3]);
    assert!(resume_cursor(&model).is_none());
}
