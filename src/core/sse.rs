use solana_sdk::pubkey::Pubkey;

/// Fan-out buffer per subscriber; a lagging consumer past this triggers a resync
pub const SSE_CHANNEL_CAPACITY: usize = 1024;

#[derive(Clone, Debug)]
pub enum SseEvent {
    AccountChanged {
        program_id: i32,
        address: Pubkey,
        slot: i64,
        write_version: i64,
        data: Vec<u8>,
    },
    ProgramEvent {
        program_id: i32,
        tx_signature: Vec<u8>,
        log_index: i32,
        slot: i64,
        data: Vec<u8>,
    },
}

impl SseEvent {
    pub fn account_changed(
        program_id: i32,
        address: Pubkey,
        slot: i64,
        write_version: i64,
        data: Vec<u8>,
    ) -> Self {
        Self::AccountChanged {
            program_id,
            address,
            slot,
            write_version,
            data,
        }
    }

    pub fn program_event(
        program_id: i32,
        tx_signature: Vec<u8>,
        log_index: i32,
        slot: i64,
        data: Vec<u8>,
    ) -> Self {
        Self::ProgramEvent {
            program_id,
            tx_signature,
            log_index,
            slot,
            data,
        }
    }
}
