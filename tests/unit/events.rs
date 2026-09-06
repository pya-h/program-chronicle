use base64::{Engine, engine::general_purpose::STANDARD};

use crate::indexer::events::{EventParser, RawLogParser};

use super::support::pubkey;

fn data_line(bytes: &[u8]) -> String {
    format!("Program data: {}", STANDARD.encode(bytes))
}

#[test]
fn attributes_top_level_event_to_the_target_program() {
    let program = pubkey(1);
    let logs = vec![
        format!("Program {} invoke [1]", program),
        "Program log: Instruction: DoThing".to_string(),
        data_line(&[1, 2, 3, 4]),
        format!("Program {} success", program),
    ];

    let events = RawLogParser.parse(&program, &logs);
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].data, vec![1, 2, 3, 4]);
    assert_eq!(
        events[0].log_index, 2,
        "log_index is the absolute log-line index"
    );
}

#[test]
fn ignores_events_emitted_by_other_programs() {
    let target = pubkey(1);
    let other = pubkey(2);
    let logs = vec![
        format!("Program {} invoke [1]", target),
        format!("Program {} invoke [2]", other),
        data_line(&[9, 9]), // emitted while `other` is on top of the stack
        format!("Program {} success", other),
        format!("Program {} success", target),
    ];

    let events = RawLogParser.parse(&target, &logs);
    assert!(
        events.is_empty(),
        "only the target program's own events are indexed"
    );
}

#[test]
fn attributes_event_after_inner_cpi_returns() {
    let target = pubkey(1);
    let other = pubkey(2);
    let logs = vec![
        format!("Program {} invoke [1]", target),
        format!("Program {} invoke [2]", other),
        data_line(&[7]), // other's event, skipped
        format!("Program {} success", other),
        data_line(&[8]), // control is back on the target's frame
        format!("Program {} success", target),
    ];

    let events = RawLogParser.parse(&target, &logs);
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].data, vec![8]);
    assert_eq!(events[0].log_index, 4);
}

#[test]
fn collects_multiple_events_with_absolute_indexes() {
    let program = pubkey(3);
    let logs = vec![
        format!("Program {} invoke [1]", program),
        data_line(&[1]),
        "Program log: noise".to_string(),
        data_line(&[2]),
        format!("Program {} success", program),
    ];

    let events = RawLogParser.parse(&program, &logs);
    assert_eq!(events.len(), 2);
    assert_eq!((events[0].log_index, &events[0].data), (1, &vec![1]));
    assert_eq!((events[1].log_index, &events[1].data), (3, &vec![2]));
}

#[test]
fn skips_undecodable_base64_payloads() {
    let program = pubkey(4);
    let logs = vec![
        format!("Program {} invoke [1]", program),
        "Program data: not*valid*base64".to_string(),
        format!("Program {} success", program),
    ];

    assert!(
        RawLogParser.parse(&program, &logs).is_empty(),
        "a payload that is not valid base64 is dropped"
    );
}

#[test]
fn failed_invocation_pops_the_stack() {
    let target = pubkey(5);
    let other = pubkey(6);
    let logs = vec![
        format!("Program {} invoke [1]", target),
        format!("Program {} invoke [2]", other),
        format!("Program {} failed: custom program error: 0x1", other),
        data_line(&[42]), // the target frame is active again after the inner failure
        format!("Program {} success", target),
    ];

    let events = RawLogParser.parse(&target, &logs);
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].data, vec![42]);
}

#[test]
fn no_data_lines_yields_no_events() {
    let program = pubkey(7);
    let logs = vec![
        format!("Program {} invoke [1]", program),
        "Program log: just a message".to_string(),
        format!("Program {} consumed 10 of 200000 compute units", program),
        format!("Program {} success", program),
    ];

    assert!(RawLogParser.parse(&program, &logs).is_empty());
}

#[test]
fn data_line_without_an_active_frame_is_ignored() {
    let program = pubkey(8);
    let logs = vec![data_line(&[1, 2, 3])]; // no invoke line came before it
    assert!(RawLogParser.parse(&program, &logs).is_empty());
}
