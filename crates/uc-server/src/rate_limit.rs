use std::collections::HashMap;
use tokio::sync::RwLock;
use tokio::time::Instant;

pub struct RateLimiter {
    buckets: RwLock<HashMap<String, TokenBucket>>,
    rate_per_second: f64,
    burst: f64,
}

struct TokenBucket {
    tokens: f64,
    last_refill: Instant,
}

impl RateLimiter {
    pub fn new(rate_per_second: u32) -> Self {
        Self {
            buckets: RwLock::new(HashMap::new()),
            rate_per_second: rate_per_second as f64,
            burst: rate_per_second as f64 * 2.0, // allow 2x burst
        }
    }

    /// Check if a request is allowed. Returns true if allowed.
    pub fn check(&self, user_id: &str) -> bool {
        // Use try_write to avoid blocking — if contended, allow the request
        let mut buckets = match self.buckets.try_write() {
            Ok(b) => b,
            Err(_) => return true,
        };

        let now = Instant::now();
        let bucket = buckets.entry(user_id.to_string()).or_insert(TokenBucket {
            tokens: self.burst,
            last_refill: now,
        });

        // Refill tokens based on elapsed time
        let elapsed = now.duration_since(bucket.last_refill).as_secs_f64();
        bucket.tokens = (bucket.tokens + elapsed * self.rate_per_second).min(self.burst);
        bucket.last_refill = now;

        // Try to consume one token
        if bucket.tokens >= 1.0 {
            bucket.tokens -= 1.0;
            true
        } else {
            false
        }
    }
}
