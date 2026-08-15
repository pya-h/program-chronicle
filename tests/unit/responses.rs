use base64::{Engine, engine::general_purpose::STANDARD};

use crate::db::entities::{program_accounts, program_events, programs, slots, transactions};
use crate::routes::responses::accounts::{AccountInfoResponse, ProgramAccountsResponse};
use crate::routes::responses::events::{ProgramEventResponse, ProgramEventsResponse};
use crate::routes::responses::stats::ProgramProgressResponse;
use crate::routes::responses::transactions::{
    TransactionResponse, TransactionSummaryResponse, TransactionsResponse,
};

use super::support::{pubkey, signature, ts};

fn account_model(address: Vec<u8>, owner: Vec<u8>) -> program_accounts::Model {
    program_accounts::Model {
        id: 1,
        program_id: 1,
        address,
        created_at: ts(),
        updated_at: ts(),
        lamports: 42,
        owner,
        state: vec![1, 2, 3],
        rent_epoch: Some(9),
        latest_slot: 500,
        latest_slot_id: 1,
        latest_tx_signature: None,
        latest_write_version: 2,
    }
}

fn event_model() -> program_events::Model {
    program_events::Model {
        id: 1,
        program_id: 1,
        program_account_id: None,
        tx_signature: signature(3).as_ref().to_vec(),
        slot: 500,
        slot_id: 1,
        log_index: 4,
        data: vec![0xde, 0xad],
        created_at: ts(),
    }
}

fn tx_model(fee_payer: Option<Vec<u8>>) -> transactions::Model {
    transactions::Model {
        id: 1,
        signature: signature(4).as_ref().to_vec(),
        program_id: Some(2),
        slot: 500,
        slot_id: 1,
        success: true,
        error: None,
        commitment: "confirmed".to_owned(),
        fee: Some(5000),
        fee_payer,
        raw: serde_json::json!({"k": "v"}),
        created_at: ts(),
    }
}

fn slot_model(block_time: Option<i64>) -> slots::Model {
    slots::Model {
        id: 1,
        rpc_id: 1,
        slot: 500,
        block_time,
        created_at: ts(),
        updated_at: ts(),
    }
}

#[test]
fn account_info_encodes_addresses_as_base58_and_data_as_base64() {
    let address = pubkey(1);
    let owner = pubkey(2);
    let model = account_model(address.to_bytes().to_vec(), owner.to_bytes().to_vec());

    let resp = AccountInfoResponse::from_model(model, Some(1234));
    assert_eq!(resp.address, address.to_string());
    assert_eq!(resp.owner, owner.to_string());
    assert_eq!(resp.data, STANDARD.encode([1, 2, 3]));
    assert_eq!(resp.lamports, 42);
    assert_eq!(resp.rent_epoch, Some(9));
    assert_eq!(resp.slot, 500);
    assert_eq!(resp.write_version, 2);
    assert_eq!(resp.block_time, Some(1234));
    assert!(
        !resp.executable,
        "executable is a constant false for indexed accounts"
    );
}

#[test]
fn account_info_falls_back_to_base64_for_non_pubkey_bytes() {
    // a blob that is not 32 bytes cannot be a pubkey, so it is base64-encoded instead of panicking
    let resp = AccountInfoResponse::from_model(account_model(vec![1, 2, 3], vec![4, 5, 6]), None);
    assert_eq!(resp.address, STANDARD.encode([1, 2, 3]));
    assert_eq!(resp.owner, STANDARD.encode([4, 5, 6]));
    assert_eq!(resp.block_time, None);
}

#[test]
fn program_event_encodes_signature_and_data() {
    let resp = ProgramEventResponse::from_model(event_model(), Some(1));
    assert_eq!(resp.tx_signature, signature(3).to_string());
    assert_eq!(resp.log_index, 4);
    assert_eq!(resp.data, STANDARD.encode([0xde, 0xad]));
    assert_eq!(resp.slot, 500);
    assert_eq!(resp.block_time, Some(1));
}

#[test]
fn transaction_response_encodes_signature_and_optional_fee_payer() {
    let payer = pubkey(6);
    let resp = TransactionResponse::from_model(
        tx_model(Some(payer.to_bytes().to_vec())),
        Some(1),
        vec![2, 7],
    );
    assert_eq!(resp.signature, signature(4).to_string());
    assert_eq!(resp.program_id, Some(2));
    assert_eq!(resp.programs, vec![2, 7]);
    assert!(resp.success);
    assert_eq!(resp.fee, Some(5000));
    assert_eq!(resp.fee_payer, Some(payer.to_string()));
    assert_eq!(resp.raw, serde_json::json!({"k": "v"}));
    assert_eq!(resp.block_time, Some(1));
}

#[test]
fn transaction_summary_handles_missing_fee_payer() {
    let resp = TransactionSummaryResponse::from_model(tx_model(None), None);
    assert_eq!(resp.fee_payer, None);
    assert_eq!(resp.block_time, None);
}

#[test]
fn program_progress_reads_straight_off_the_program_row() {
    let program = programs::Model {
        id: 3,
        rpc_id: 1,
        address: pubkey(7).to_bytes().to_vec(),
        name: Some("my-program".into()),
        last_processed_at: None,
        last_processed_slot: Some(900),
        start_slot: Some(100),
        active: true,
        account_filters: None,
        created_at: ts(),
        updated_at: ts(),
    };

    let resp = ProgramProgressResponse::from_model(program);
    assert_eq!(resp.id, 3);
    assert_eq!(resp.name.as_deref(), Some("my-program"));
    assert_eq!(resp.address, pubkey(7).to_string());
    assert_eq!(resp.start_slot, Some(100));
    assert_eq!(resp.last_processed_slot, Some(900));
    assert_eq!(resp.last_processed_at, None);
}

#[test]
fn program_accounts_response_threads_block_time_and_counts() {
    let rows = vec![
        (
            account_model(pubkey(1).to_bytes().to_vec(), pubkey(2).to_bytes().to_vec()),
            Some(slot_model(Some(111))),
        ),
        (
            account_model(pubkey(3).to_bytes().to_vec(), pubkey(4).to_bytes().to_vec()),
            None,
        ),
    ];

    let resp = ProgramAccountsResponse::new(rows, 2, 1);
    assert_eq!(resp.total, 2);
    assert_eq!(resp.pages, 1);
    assert_eq!(resp.data.len(), 2);
    assert_eq!(resp.data[0].block_time, Some(111));
    assert_eq!(
        resp.data[1].block_time, None,
        "a row with no joined slot has no block_time"
    );
}

#[test]
fn events_response_maps_rows_and_counts() {
    let rows = vec![(event_model(), Some(slot_model(Some(222))))];
    let resp = ProgramEventsResponse::new(rows, 1, 1);
    assert_eq!(resp.total, 1);
    assert_eq!(resp.data[0].block_time, Some(222));
}

#[test]
fn transactions_response_maps_rows_and_counts() {
    let rows = vec![(tx_model(None), Some(slot_model(Some(333))))];
    let resp = TransactionsResponse::new(rows, 1, 1);
    assert_eq!(resp.total, 1);
    assert_eq!(resp.data[0].block_time, Some(333));
}
