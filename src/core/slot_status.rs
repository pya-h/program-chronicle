/// What the node last told us about a slot, per RPC
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SlotStatus {
    /// Never settled: what every `ensure_slot` row starts at, and what a not-yet-finalized answer
    /// leaves behind so the next tick retries it.
    Unknown,
    Canonical,
    Reorged,
    Pruned,
}

impl SlotStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            SlotStatus::Unknown => "unknown",
            SlotStatus::Canonical => "canonical",
            SlotStatus::Reorged => "reorged",
            SlotStatus::Pruned => "pruned",
        }
    }

    pub fn from_query(value: &str) -> Option<Self> {
        match value {
            "unknown" => Some(SlotStatus::Unknown),
            "canonical" => Some(SlotStatus::Canonical),
            "reorged" => Some(SlotStatus::Reorged),
            "pruned" => Some(SlotStatus::Pruned),
            _ => None,
        }
    }

    /// Whether the verdict is already known without downloading the block: `Reorged` drops the rows
    /// a status pass answered `None` for, `Pruned` finalizes them all. `Canonical` is absent on
    /// purpose — a row indexed at that slot later still needs the signature list read.
    pub fn answers_without_block(self) -> bool {
        matches!(self, SlotStatus::Reorged | SlotStatus::Pruned)
    }
}
