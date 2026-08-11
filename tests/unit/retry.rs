use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use tokio::time::sleep;

use crate::core::retry::{RetryPolicy, jitter, retry};

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

#[test]
fn new_derives_the_delay_bounds() {
    let policy = RetryPolicy::new(5);
    assert_eq!(policy.base_delay, Duration::from_millis(500));
    assert_eq!(policy.max_delay, Duration::from_secs(30));
    assert_eq!(policy.reset_after, Duration::from_secs(60)); // max_delay * 2
}

#[test]
fn backoff_stays_within_its_jittered_window_and_saturates() {
    let policy = RetryPolicy::new(5);
    for retry in 0..20 {
        let capped = (policy.base_delay * (1u32 << retry.min(16))).min(policy.max_delay);
        let delay = policy.backoff(retry);
        assert!(delay >= capped / 2 && delay <= capped);
    }
    // a huge retry count saturates at the max_delay window instead of overflowing
    let delay = policy.backoff(64);
    assert!(delay >= policy.max_delay / 2 && delay <= policy.max_delay);
}

#[test]
fn jitter_is_bounded_and_decorrelated() {
    let span = Duration::from_secs(10);
    let mut seen = std::collections::HashSet::new();
    for _ in 0..64 {
        let j = jitter(span);
        assert!(j < span);
        seen.insert(j.as_nanos());
    }
    assert!(seen.len() > 1); // independent draws, not the lockstep the wall clock produced
    assert_eq!(jitter(Duration::ZERO), Duration::ZERO);
}
