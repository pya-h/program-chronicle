use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use crate::core::retry::{RetryPolicy, retry};

/// Zero delays so the retry loop spins without real sleeping.
fn fast_policy(max_attempts: u32) -> RetryPolicy {
    RetryPolicy {
        max_attempts,
        base_delay: Duration::ZERO,
        max_delay: Duration::ZERO,
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

#[test]
fn policy_clamps_attempts_to_at_least_one() {
    assert_eq!(RetryPolicy::new(0).max_attempts, 1);
    assert_eq!(RetryPolicy::new(5).max_attempts, 5);
}
