use serde_json::json;
use solana_transaction_status::EncodedConfirmedTransactionWithStatusMeta;

use crate::indexer::rpc::json_rpc::{account_keys, fee_payer};

use super::support::pubkey;

/// A `json`-encoded (Raw message) transaction fixture with the given header counts and account keys,
/// no meta — the shape `getTransaction` returns and `account_keys`/`fee_payer` read.
fn raw_tx(
    required_signatures: u8,
    readonly_signed: u8,
    readonly_unsigned: u8,
    keys: &[String],
) -> EncodedConfirmedTransactionWithStatusMeta {
    serde_json::from_value(json!({
        "slot": 100,
        "blockTime": 1000,
        "transaction": {
            "signatures": [],
            "message": {
                "header": {
                    "numRequiredSignatures": required_signatures,
                    "numReadonlySignedAccounts": readonly_signed,
                    "numReadonlyUnsignedAccounts": readonly_unsigned,
                },
                "accountKeys": keys,
                "recentBlockhash": pubkey(255).to_string(),
                "instructions": [],
            },
        },
        "meta": null,
    }))
    .expect("fixture should deserialize")
}

#[test]
fn account_keys_classifies_writable_by_header() {
    let keys: Vec<String> = (0..4).map(|seed| pubkey(seed).to_string()).collect();
    // 2 signers (1 readonly), 2 unsigned (1 readonly) => W, R, W, R
    let tx = raw_tx(2, 1, 1, &keys);
    let accounts = account_keys(&tx);

    assert_eq!(accounts.len(), 4);
    assert_eq!(accounts[0].address, pubkey(0));
    assert!(accounts[0].writable, "writable signer");
    assert!(!accounts[1].writable, "readonly signer");
    assert!(accounts[2].writable, "writable unsigned");
    assert!(!accounts[3].writable, "readonly unsigned");
}

#[test]
fn account_keys_all_writable_when_no_readonly() {
    let keys: Vec<String> = (0..3).map(|seed| pubkey(seed).to_string()).collect();
    let tx = raw_tx(1, 0, 0, &keys);
    let accounts = account_keys(&tx);

    assert_eq!(accounts.len(), 3);
    assert!(accounts.iter().all(|account| account.writable));
}

#[test]
fn account_keys_handles_all_readonly_signers() {
    let keys: Vec<String> = (0..2).map(|seed| pubkey(seed).to_string()).collect();
    // every signer readonly: writable_signers saturates at 0
    let tx = raw_tx(2, 2, 0, &keys);
    let accounts = account_keys(&tx);

    assert!(!accounts[0].writable);
    assert!(!accounts[1].writable);
}

#[test]
fn account_keys_empty_for_non_json_encoding() {
    // a base58/binary-encoded transaction carries no readable account list
    let tx: EncodedConfirmedTransactionWithStatusMeta = serde_json::from_value(json!({
        "slot": 100,
        "blockTime": 1000,
        "transaction": "3Bxs4h24hBtQy9rw",
        "meta": null,
    }))
    .expect("binary fixture should deserialize");
    assert!(account_keys(&tx).is_empty());
    assert!(fee_payer(&tx).is_none());
}

#[test]
fn fee_payer_is_the_first_account_key() {
    let keys: Vec<String> = (10..13).map(|seed| pubkey(seed).to_string()).collect();
    let tx = raw_tx(1, 0, 0, &keys);
    assert_eq!(fee_payer(&tx), Some(pubkey(10)));
}
