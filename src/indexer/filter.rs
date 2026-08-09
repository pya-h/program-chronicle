//! Translates a program's stored `account_filters` descriptor into the two forms ingestion needs:
//! server-side `getProgramAccounts`/`programSubscribe` filters, and an in-code predicate used as a
//! fallback for sources that can't filter server-side. `None` everywhere means "track all".

use solana_client::rpc_filter::{Memcmp, RpcFilterType};
use solana_sdk::pubkey::Pubkey;

use crate::core::error::SvmIndexerError;
use crate::indexer::rpc::{AccountFilter, AccountSnapshot, MemcmpFilter};

/// Parses a program's stored `account_filters` JSON, collapsing an absent/empty descriptor to
/// `None` so the ingestion paths treat it as track-all.
pub fn parse_account_filters(raw: Option<&serde_json::Value>) -> Option<AccountFilter> {
    let filter: AccountFilter = serde_json::from_value(raw?.clone()).ok()?;
    (!filter.is_empty()).then_some(filter)
}

impl MemcmpFilter {
    /// The raw bytes to match, decoded from the base58 `bytes`.
    fn raw_bytes(&self) -> Option<Vec<u8>> {
        bs58::decode(&self.bytes).into_vec().ok()
    }
}

impl AccountFilter {
    /// No predicate narrows anything — equivalent to no filter at all.
    pub fn is_empty(&self) -> bool {
        self.data_size.is_none() && self.memcmp.is_empty() && self.addresses.is_empty()
    }

    /// Rejects a malformed descriptor before it is stored: memcmp bytes must be base58 and every
    /// listed address a valid pubkey.
    pub fn validate(&self) -> Result<(), SvmIndexerError> {
        for memcmp in &self.memcmp {
            if memcmp.raw_bytes().is_none() {
                return Err(SvmIndexerError::BadInput(
                    "account_filters memcmp bytes must be valid base58!".to_owned(),
                ));
            }
        }
        for address in &self.addresses {
            if address.parse::<Pubkey>().is_err() {
                return Err(SvmIndexerError::InvalidPublicKey(
                    "account_filters addresses must be valid solana public keys!".to_owned(),
                ));
            }
        }
        Ok(())
    }

    /// When the descriptor is purely an address allowlist, the parsed pubkeys — so ingestion fetches
    /// and subscribes to just those keys instead of scanning every account the program owns.
    pub fn address_allowlist(&self) -> Option<Vec<Pubkey>> {
        if self.data_size.is_some() || !self.memcmp.is_empty() || self.addresses.is_empty() {
            return None;
        }
        self.addresses.iter().map(|a| a.parse().ok()).collect()
    }

    /// The `data_size`/`memcmp` predicates as `getProgramAccounts`/`programSubscribe` filters so the
    /// RPC narrows server-side. Addresses are handled separately via [`Self::address_allowlist`].
    pub fn rpc_filters(&self) -> Vec<RpcFilterType> {
        let mut filters = Vec::new();
        if let Some(size) = self.data_size {
            filters.push(RpcFilterType::DataSize(size));
        }
        for memcmp in &self.memcmp {
            if let Some(bytes) = memcmp.raw_bytes() {
                filters.push(RpcFilterType::Memcmp(Memcmp::new_raw_bytes(memcmp.offset, bytes)));
            }
        }
        filters
    }

    /// In-code predicate: `true` when the snapshot satisfies every clause. Used to drop non-matches
    /// on the live path (the RPC filter is best-effort) and to apply the `addresses` clause.
    pub fn matches(&self, account: &AccountSnapshot) -> bool {
        if self.data_size.is_some_and(|size| account.data.len() as u64 != size) {
            return false;
        }
        for memcmp in &self.memcmp {
            let Some(bytes) = memcmp.raw_bytes() else {
                return false;
            };
            let end = memcmp.offset + bytes.len();
            if account.data.len() < end || account.data[memcmp.offset..end] != bytes[..] {
                return false;
            }
        }
        if !self.addresses.is_empty()
            && !self.addresses.iter().any(|a| *a == account.address.to_string())
        {
            return false;
        }
        true
    }
}
