use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use tokio::time::sleep;

use crate::core::retry::{RetryPolicy, retry};

/// Zero delays so the retry loop spins without real sleeping; `reset_after` far above any test run
/// so failures always accumulate unless a test deliberately outlasts it.
fn fast_policy(max_attempts: u32) -> RetryPolicy {
    RetryPolicy {
        max_attempts,
        base_delay: Duration::ZERO,
        max_delay: Duration::ZERO,
        reset_after: Duration::from_secs(3600),
    }
}

#[tokio::test]
async fn succeeds_without_retrying() {
    let calls = AtomicU32::new(0);
    let result: Result<u32, String> = retry(fast_policy(5), "op", || {
        calls.fetch_add(1, Ordering::SeqCst);
        async { Ok(7) }
    })
    .await;
    assert_eq!(result.unwrap(), 7);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn retries_until_success() {
    let calls = AtomicU32::new(0);
    let result: Result<u32, String> = retry(fast_policy(5), "op", || {
        let n = calls.fetch_add(1, Ordering::SeqCst);
        async move {
            if n < 2 {
                Err(format!("transient {n}"))
            } else {
                Ok(n)
            }
        }
    })
    .await;
    assert_eq!(result.unwrap(), 2);
    assert_eq!(calls.load(Ordering::SeqCst), 3);
}

#[tokio::test]
async fn gives_up_after_max_attempts() {
    let calls = AtomicU32::new(0);
    let result: Result<u32, String> = retry(fast_policy(3), "op", || {
        calls.fetch_add(1, Ordering::SeqCst);
        async { Err("always".to_owned()) }
    })
    .await;
    assert_eq!(result.unwrap_err(), "always");
    assert_eq!(calls.load(Ordering::SeqCst), 3);
}

#[tokio::test]
async fn a_healthy_run_resets_the_failure_budget() {
    // each run outlasts reset_after before failing, so the streak never reaches max_attempts
    let policy = RetryPolicy {
        max_attempts: 3,
        base_delay: Duration::ZERO,
        max_delay: Duration::ZERO,
        reset_after: Duration::from_millis(5),
    };
    let calls = AtomicU32::new(0);
    let result: Result<u32, String> = retry(policy, "op", || {
        let n = calls.fetch_add(1, Ordering::SeqCst);
        async move {
            if n < 5 {
                sleep(Duration::from_millis(20)).await; // stayed healthy past reset_after
                Err(format!("drop {n}"))
            } else {
                Ok(n)
            }
        }
    })
    .await;
    // without the reset it would give up at attempt 3; instead it retries through all five drops
    assert_eq!(result.unwrap(), 5);
    assert_eq!(calls.load(Ordering::SeqCst), 6);
}

#[test]
fn policy_clamps_attempts_to_at_least_one() {
    assert_eq!(RetryPolicy::new(0).max_attempts, 1);
    assert_eq!(RetryPolicy::new(5).max_attempts, 5);
}
