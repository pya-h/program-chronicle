use std::fmt::Display;
use std::future::Future;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use tokio::time::{Instant, sleep};

#[derive(Clone, Copy, Debug)]
pub struct RetryPolicy {
    pub max_attempts: u32,
    pub base_delay: Duration,
    pub max_delay: Duration,
    pub reset_after: Duration,
}

impl RetryPolicy {
    pub fn new(max_attempts: u32) -> Self {
        let max_delay = Duration::from_secs(30);
        Self {
            max_attempts: max_attempts.max(1),
            base_delay: Duration::from_millis(500),
            max_delay,
            reset_after: max_delay * 2,
        }
    }

    fn backoff(&self, retry: u32) -> Duration {
        let exp = self.base_delay.saturating_mul(1u32 << retry.min(16));
        let capped = exp.min(self.max_delay);
        let half = capped / 2;
        half + jitter(capped - half)
    }
}

fn jitter(span: Duration) -> Duration {
    if span.is_zero() {
        return Duration::ZERO;
    }
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.subsec_nanos());
    span.mul_f64(nanos as f64 / 1_000_000_000.0)
}

pub async fn retry<T, E, F, Fut>(
    policy: RetryPolicy,
    operation_name: &str,
    mut operation: F,
) -> Result<T, E>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, E>>,
    E: Display,
{
    let mut attempt = 1;
    loop {
        let started = Instant::now();
        match operation().await {
            Ok(value) => return Ok(value),
            Err(err) => {
                // a run that did real work before failing shouldn't burn the budget
                if started.elapsed() >= policy.reset_after {
                    attempt = 1;
                }
                if attempt >= policy.max_attempts {
                    tracing::error!(
                        operation = operation_name,
                        attempts = attempt,
                        error = %err,
                        "Operation failed after exhausting retries."
                    );
                    return Err(err);
                }
                let delay = policy.backoff(attempt - 1);
                tracing::warn!(
                    operation = operation_name,
                    attempt,
                    retry_in_ms = delay.as_millis(),
                    error = %err,
                    "Operation failed; retrying after backoff."
                );
                sleep(delay).await;
                attempt += 1;
            }
        }
    }
}
