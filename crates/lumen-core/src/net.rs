use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Clone)]
pub struct GlobalRateLimiter {
    inner: Arc<Mutex<RateLimiterState>>,
}

struct RateLimiterState {
    bytes_per_sec: u64, // 0 = unlimited
    tokens: f64,
    capacity: f64,
    last_refill: Instant,
}

impl GlobalRateLimiter {
    pub fn new(bytes_per_sec: u64) -> Self {
        let mut limiter = Self {
            inner: Arc::new(Mutex::new(RateLimiterState {
                bytes_per_sec: 0,
                tokens: 0.0,
                capacity: 0.0,
                last_refill: Instant::now(),
            })),
        };
        limiter.set_limit(bytes_per_sec);
        limiter
    }

    pub fn set_limit(&mut self, bytes_per_sec: u64) {
        let mut state = self.inner.lock().unwrap();
        state.bytes_per_sec = bytes_per_sec;
        if bytes_per_sec == 0 {
            state.tokens = 0.0;
            state.capacity = 0.0;
            return;
        }

        let cap = ((bytes_per_sec as f64) / 2.0).clamp(1024.0, 64.0 * 1024.0);
        state.capacity = cap;

        let mut init_tokens = (bytes_per_sec as f64) / 10.0;
        if init_tokens < 512.0 {
            init_tokens = 512.0;
        }
        state.tokens = init_tokens;
        state.last_refill = Instant::now();
    }

    pub fn get_limit(&self) -> u64 {
        self.inner.lock().unwrap().bytes_per_sec
    }

    pub async fn wait(&self, n: usize) {
        loop {
            let wait_dur = {
                let mut state = self.inner.lock().unwrap();
                if state.bytes_per_sec == 0 {
                    return;
                }

                let now = Instant::now();
                let elapsed = now.duration_since(state.last_refill).as_secs_f64();
                state.last_refill = now;

                state.tokens += elapsed * (state.bytes_per_sec as f64);
                if state.tokens > state.capacity {
                    state.tokens = state.capacity;
                }

                let needed = n as f64;
                if state.tokens >= needed {
                    state.tokens -= needed;
                    return;
                }

                let deficit = needed - state.tokens;
                Duration::from_secs_f64(deficit / (state.bytes_per_sec as f64))
            };

            tokio::time::sleep(wait_dur).await;
        }
    }
}

pub fn create_http_client(proxy: Option<&str>) -> Result<reqwest::Client, String> {
    let mut builder = reqwest::Client::builder()
        .timeout(Duration::from_secs(60))
        .redirect(reqwest::redirect::Policy::none());

    if let Some(p) = proxy {
        let trimmed = p.trim();
        if !trimmed.is_empty() {
            let proxy_obj = reqwest::Proxy::all(trimmed).map_err(|e| e.to_string())?;
            builder = builder.proxy(proxy_obj);
        }
    }

    builder.build().map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_rate_limiter_limit() {
        let mut limiter = GlobalRateLimiter::new(100_000);
        assert_eq!(limiter.get_limit(), 100_000);

        limiter.set_limit(0);
        assert_eq!(limiter.get_limit(), 0);
        limiter.wait(50_000).await; // Unlimited should return immediately
    }
}
