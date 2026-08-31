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

    pub fn is_settled(self) -> bool {
        matches!(self, SlotStatus::Canonical | SlotStatus::Reorged)
    }
}
