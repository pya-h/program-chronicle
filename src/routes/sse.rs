use std::convert::Infallible;

use axum::{
    Router,
    extract::{Path, State},
    response::sse::{Event, KeepAlive, Sse},
    routing::get,
};
use base64::{Engine, engine::general_purpose::STANDARD};
use futures::{Stream, stream::unfold};
use serde_json::{Value, json};
use solana_sdk::{pubkey::Pubkey, signature::Signature};
use tokio::sync::broadcast::{Receiver, error::RecvError};

use crate::core::{error::SvmIndexerError, sse::SseEvent, state::AppState};

fn build_event(name: &str, payload: Value) -> Event {
    Event::default()
        .event(name)
        .json_data(payload)
        .unwrap_or_else(|_| Event::default().event(name))
}

fn account_event(
    program_id: i32,
    address: &Pubkey,
    slot: i64,
    write_version: i64,
    data: &[u8],
) -> Event {
    build_event(
        "account",
        json!({
            "program_id": program_id,
            "address": address.to_string(),
            "slot": slot,
            "write_version": write_version,
            "data": STANDARD.encode(data),
        }),
    )
}

fn program_event(
    program_id: i32,
    tx_signature: &[u8],
    log_index: i32,
    slot: i64,
    data: &[u8],
) -> Event {
    let signature = Signature::try_from(tx_signature)
        .map(|sig| sig.to_string())
        .unwrap_or_else(|_| STANDARD.encode(tx_signature));
    build_event(
        "event",
        json!({
            "program_id": program_id,
            "signature": signature,
            "log_index": log_index,
            "slot": slot,
            "data": STANDARD.encode(data),
        }),
    )
}

fn transaction_event(
    program_id: i32,
    signature: &[u8],
    slot: i64,
    success: bool,
    err: &Option<Value>,
    fee: Option<i64>,
    fee_payer: &Option<Vec<u8>>,
) -> Event {
    let signature = Signature::try_from(signature)
        .map(|sig| sig.to_string())
        .unwrap_or_else(|_| STANDARD.encode(signature));
    let fee_payer = fee_payer.as_ref().map(|bytes| {
        Pubkey::try_from(bytes.as_slice())
            .map(|pk| pk.to_string())
            .unwrap_or_else(|_| STANDARD.encode(bytes))
    });
    build_event(
        "transaction",
        json!({
            "program_id": program_id,
            "signature": signature,
            "slot": slot,
            "success": success,
            "err": err,
            "fee": fee,
            "fee_payer": fee_payer,
        }),
    )
}

/// A lagged consumer past the channel buffer gets a `resync` frame carrying the last slot it did
/// see, so it re-pulls the gap via the read API's `since_slot` instead of silently losing changes.
fn resync_event(last_slot: i64, missed: u64) -> Event {
    build_event(
        "resync",
        json!({ "since_slot": last_slot, "missed": missed }),
    )
}

fn subscriber_stream<F>(
    rx: Receiver<SseEvent>,
    shape: F,
) -> impl Stream<Item = Result<Event, Infallible>>
where
    F: Fn(&SseEvent) -> Option<(i64, Event)> + Send + 'static,
{
    unfold(
        (rx, 0_i64, shape),
        |(mut rx, mut last_slot, shape)| async move {
            loop {
                match rx.recv().await {
                    Ok(event) => {
                        if let Some((slot, out)) = shape(&event) {
                            last_slot = slot;
                            return Some((Ok(out), (rx, last_slot, shape)));
                        }
                    }
                    Err(RecvError::Lagged(missed)) => {
                        return Some((Ok(resync_event(last_slot, missed)), (rx, last_slot, shape)));
                    }
                    Err(RecvError::Closed) => return None,
                }
            }
        },
    )
}

async fn sse_account_handler(
    State(state): State<AppState>,
    Path(address): Path<String>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, SvmIndexerError> {
    let target = address.parse::<Pubkey>().map_err(|_| {
        SvmIndexerError::InvalidPublicKey(
            "Provided address is not a standard solana address!".to_owned(),
        )
    })?;
    let stream = subscriber_stream(state.sse_tx.subscribe(), move |event| match event {
        SseEvent::AccountChanged {
            program_id,
            address,
            slot,
            write_version,
            data,
        } if *address == target => Some((
            *slot,
            account_event(*program_id, address, *slot, *write_version, data),
        )),
        _ => None,
    });
    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}

async fn sse_program_accounts_handler(
    State(state): State<AppState>,
    Path(program_id): Path<i32>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let stream = subscriber_stream(state.sse_tx.subscribe(), move |event| match event {
        SseEvent::AccountChanged {
            program_id: pid,
            address,
            slot,
            write_version,
            data,
        } if *pid == program_id => Some((
            *slot,
            account_event(*pid, address, *slot, *write_version, data),
        )),
        _ => None,
    });
    Sse::new(stream).keep_alive(KeepAlive::default())
}

async fn sse_program_events_handler(
    State(state): State<AppState>,
    Path(program_id): Path<i32>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let stream = subscriber_stream(state.sse_tx.subscribe(), move |event| match event {
        SseEvent::ProgramEvent {
            program_id: pid,
            tx_signature,
            log_index,
            slot,
            data,
        } if *pid == program_id => Some((
            *slot,
            program_event(*pid, tx_signature, *log_index, *slot, data),
        )),
        _ => None,
    });
    Sse::new(stream).keep_alive(KeepAlive::default())
}

async fn sse_program_transactions_handler(
    State(state): State<AppState>,
    Path(program_id): Path<i32>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let stream = subscriber_stream(state.sse_tx.subscribe(), move |event| match event {
        SseEvent::TransactionIndexed {
            program_id: pid,
            signature,
            slot,
            success,
            err,
            fee,
            fee_payer,
        } if *pid == program_id => Some((
            *slot,
            transaction_event(*pid, signature, *slot, *success, err, *fee, fee_payer),
        )),
        _ => None,
    });
    Sse::new(stream).keep_alive(KeepAlive::default())
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/accounts/{address}", get(sse_account_handler))
        .route(
            "/programs/{program_id}/accounts",
            get(sse_program_accounts_handler),
        )
        .route(
            "/programs/{program_id}/events",
            get(sse_program_events_handler),
        )
        .route(
            "/programs/{program_id}/transactions",
            get(sse_program_transactions_handler),
        )
}
