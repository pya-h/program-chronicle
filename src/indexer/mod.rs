//! The ingestion engine. `rpc` is the chain data-source seam; the per-layer flow modules
//! (persist/state/transactions/events) and the supervisor land in later phases.

pub mod rpc;
