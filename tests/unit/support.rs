use solana_sdk::{pubkey::Pubkey, signature::Signature};

/// A fixed timestamp for building entity models in isolation — value is irrelevant to the logic
/// under test, the response DTOs never read the `created_at`/`updated_at` columns.
pub fn ts() -> chrono::NaiveDateTime {
    chrono::DateTime::from_timestamp(0, 0).unwrap().naive_utc()
}

/// Deterministic 32-byte address from a seed, so tests can round-trip the base58 string.
pub fn pubkey(seed: u8) -> Pubkey {
    Pubkey::new_from_array([seed; 32])
}

/// Deterministic 64-byte signature from a seed.
pub fn signature(seed: u8) -> Signature {
    Signature::from([seed; 64])
}
