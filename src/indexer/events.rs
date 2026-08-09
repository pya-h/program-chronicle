use base64::{Engine, engine::general_purpose::STANDARD};
use solana_sdk::pubkey::Pubkey;

pub struct ParsedEvent {
    pub log_index: usize, // prevents one tx touching multiple programs collision
    pub data: Vec<u8>,
}

pub trait EventParser: Send + Sync {
    fn parse(&self, program: &Pubkey, logs: &[String]) -> Vec<ParsedEvent>;
}

pub struct RawLogParser;

const DATA_PREFIX: &str = "Program data: "; // Events are logs formatted as: <log_index> | Program data: <event_raw_data>
// The table below shows the transaction.logs array, which simplifies what the parser is doing.
// index │ log line
// ──────┼──────────────────────────────────────────────────────────────
//   0   │ Program MyProg111...111 invoke [1]
//   1   │ Program log: Instruction: PlaceBet
//   2   │ Program data: SGVsbG8gd29ybGQ=            ← an event, base64
//   3   │ Program TokenkegQ...111 invoke [2]
//   4   │ Program log: Instruction: Transfer
//   5   │ Program TokenkegQ...111 consumed 3000 of 200000 compute units
//   6   │ Program TokenkegQ...111 success
//   7   │ Program data: QW5vdGhlciBldmVudA==        ← another event
//   8   │ Program MyProg111...111 consumed 15000 of 200000 compute units
//   9   │ Program MyProg111...111 success

impl EventParser for RawLogParser {
    fn parse(&self, program: &Pubkey, logs: &[String]) -> Vec<ParsedEvent> {
        let target = program.to_string();
        // The actual program's address that emits the event in previous line(s) so a stack is needed to link event's data & program
        let mut stack: Vec<&str> = Vec::new();
        let mut events = Vec::new();

        for (log_index, line) in logs.iter().enumerate() {
            if let Some(payload) = line.strip_prefix(DATA_PREFIX) {
                if stack.last().copied() == Some(target.as_str())
                    && let Ok(data) = STANDARD.decode(payload)
                {
                    events.push(ParsedEvent { log_index, data });
                }
            } else {
                let Some(rest) = line.strip_prefix("Program ") else {
                    continue;
                };
                let (first, remainder) = rest.split_once(' ').unwrap_or((rest, ""));
                if !first.ends_with(':') {
                    if remainder.starts_with("invoke [") {
                        stack.push(first);
                    } else if remainder == "success" || remainder.starts_with("failed") {
                        stack.pop();
                    }
                }
            }
        }
        events
    }
}
