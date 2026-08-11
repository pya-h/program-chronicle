use solana_client::rpc_filter::RpcFilterType;
use solana_sdk::pubkey::Pubkey;

use crate::indexer::filter::parse_account_filters;
use crate::indexer::rpc::{AccountFilter, AccountSnapshot, MemcmpFilter};

use super::support::pubkey;

fn snapshot(address: Pubkey, data: Vec<u8>) -> AccountSnapshot {
    AccountSnapshot {
        address,
        owner: pubkey(0),
        lamports: 1,
        data,
        rent_epoch: 0,
        executable: false,
        slot: 10,
        write_version: None,
    }
}

#[test]
fn parse_collapses_absent_and_empty_to_track_all() {
    assert!(parse_account_filters(None).is_none());
    assert!(parse_account_filters(Some(&serde_json::json!({}))).is_none());
    assert!(
        parse_account_filters(Some(&serde_json::json!({"memcmp": [], "addresses": []}))).is_none()
    );
}

#[test]
fn parse_reads_a_populated_descriptor() {
    let filter =
        parse_account_filters(Some(&serde_json::json!({"data_size": 165}))).expect("populated");
    assert_eq!(filter.data_size, Some(165));
}

#[test]
fn data_size_predicate_matches_exact_length_only() {
    let filter = AccountFilter { data_size: Some(3), ..Default::default() };
    assert!(filter.matches(&snapshot(pubkey(1), vec![0, 1, 2])));
    assert!(!filter.matches(&snapshot(pubkey(1), vec![0, 1])));
    assert!(!filter.matches(&snapshot(pubkey(1), vec![0, 1, 2, 3])));
}

#[test]
fn memcmp_predicate_matches_bytes_at_offset() {
    // base58 of [9, 9] is used as the needle at offset 1
    let needle = bs58::encode([9u8, 9u8]).into_string();
    let filter = AccountFilter {
        memcmp: vec![MemcmpFilter { offset: 1, bytes: needle }],
        ..Default::default()
    };
    assert!(filter.matches(&snapshot(pubkey(1), vec![0, 9, 9, 0])));
    assert!(!filter.matches(&snapshot(pubkey(1), vec![0, 9, 8, 0])));
    assert!(!filter.matches(&snapshot(pubkey(1), vec![0, 9]))); // too short for the window
}

#[test]
fn addresses_predicate_is_an_allowlist() {
    let filter = AccountFilter {
        addresses: vec![pubkey(5).to_string()],
        ..Default::default()
    };
    assert!(filter.matches(&snapshot(pubkey(5), vec![])));
    assert!(!filter.matches(&snapshot(pubkey(6), vec![])));
}

#[test]
fn address_allowlist_only_when_addresses_are_the_sole_clause() {
    let only_addresses = AccountFilter {
        addresses: vec![pubkey(5).to_string(), pubkey(6).to_string()],
        ..Default::default()
    };
    let keys = only_addresses.address_allowlist().expect("allowlist");
    assert_eq!(keys, vec![pubkey(5), pubkey(6)]);

    // combining any server-side clause disqualifies the multi-account short-circuit
    let combined = AccountFilter {
        data_size: Some(1),
        addresses: vec![pubkey(5).to_string()],
        ..Default::default()
    };
    assert!(combined.address_allowlist().is_none());
}

#[test]
fn memcmp_rejects_an_offset_that_overflows() {
    let filter = AccountFilter {
        memcmp: vec![MemcmpFilter { offset: usize::MAX, bytes: bs58::encode([1u8]).into_string() }],
        ..Default::default()
    };
    assert!(!filter.matches(&snapshot(pubkey(1), vec![1, 2, 3])));
}

#[test]
fn memcmp_with_invalid_base58_never_matches() {
    let filter = AccountFilter {
        memcmp: vec![MemcmpFilter { offset: 0, bytes: "0OIl".into() }], // non-base58 chars
        ..Default::default()
    };
    assert!(!filter.matches(&snapshot(pubkey(1), vec![0, 1, 2])));
}

#[test]
fn matches_requires_every_clause() {
    let needle = bs58::encode([9u8, 9u8]).into_string();
    let filter = AccountFilter {
        data_size: Some(4),
        memcmp: vec![MemcmpFilter { offset: 1, bytes: needle }],
        addresses: vec![pubkey(5).to_string()],
        ..Default::default()
    };
    assert!(filter.matches(&snapshot(pubkey(5), vec![0, 9, 9, 0])));
    assert!(!filter.matches(&snapshot(pubkey(6), vec![0, 9, 9, 0]))); // wrong address
    assert!(!filter.matches(&snapshot(pubkey(5), vec![0, 9, 9]))); // wrong size
    assert!(!filter.matches(&snapshot(pubkey(5), vec![0, 9, 8, 0]))); // wrong bytes
}

#[test]
fn parse_ignores_a_malformed_descriptor() {
    // data_size expects an integer; a string fails deserialization and collapses to track-all
    assert!(parse_account_filters(Some(&serde_json::json!({"data_size": "big"}))).is_none());
}

#[test]
fn rpc_filters_translate_data_size_and_memcmp() {
    let filter = AccountFilter {
        data_size: Some(165),
        memcmp: vec![MemcmpFilter { offset: 0, bytes: pubkey(7).to_string() }],
        ..Default::default()
    };
    let filters = filter.rpc_filters();
    assert_eq!(filters.len(), 2);
    assert!(matches!(filters[0], RpcFilterType::DataSize(165)));
    assert!(matches!(filters[1], RpcFilterType::Memcmp(_)));
}

#[test]
fn validate_rejects_bad_base58_and_addresses() {
    let bad_memcmp = AccountFilter {
        memcmp: vec![MemcmpFilter { offset: 0, bytes: "0OIl".into() }], // non-base58 chars
        ..Default::default()
    };
    assert!(bad_memcmp.validate().is_err());

    let bad_address = AccountFilter {
        addresses: vec!["not-a-pubkey".into()],
        ..Default::default()
    };
    assert!(bad_address.validate().is_err());

    let good = AccountFilter { data_size: Some(165), ..Default::default() };
    assert!(good.validate().is_ok());
}
