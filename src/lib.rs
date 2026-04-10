pub const API_KEY: &str = "c0a2f304-551a-4d08-b8df-2c53ecd57f9f";
pub const BASE_URL: &str = "https://transit.ttc.com.ge/pis-gateway/api";

use log::warn;

use std::sync::Mutex;
use std::time::Duration;
use std::time::Instant;

/// Minimum delay between consecutive API requests across all threads.
const REQUEST_INTERVAL: Duration = Duration::from_millis(200);

pub struct RateLimiter {
    last_request: Mutex<Instant>,
}

impl Default for RateLimiter {
    fn default() -> Self {
        Self::new()
    }
}

impl RateLimiter {
    pub fn new() -> Self {
        Self {
            // Initialise in the past so the first request is never delayed.
            last_request: Mutex::new(Instant::now() - REQUEST_INTERVAL),
        }
    }

    pub fn wait(&self) {
        let mut last = self.last_request.lock().unwrap();
        let elapsed = last.elapsed();
        if elapsed < REQUEST_INTERVAL {
            std::thread::sleep(REQUEST_INTERVAL - elapsed);
        }
        *last = Instant::now();
    }
}

pub fn fetch_with_retry(
    agent: &ureq::Agent,
    url: &str,
    rate_limiter: &RateLimiter,
) -> Result<ureq::http::Response<ureq::Body>, Box<dyn std::error::Error + Send + Sync>> {
    const MAX_RETRIES: u32 = 12;
    let mut last_err: Option<ureq::Error> = None;
    let mut backoff = Duration::from_secs(1);
    for attempt in 0..MAX_RETRIES {
        if attempt > 0 {
            warn!("Retrying {url} (attempt {}/{})", attempt + 1, MAX_RETRIES);
        }
        rate_limiter.wait();
        match agent.get(url).header("X-api-key", API_KEY).call() {
            Ok(resp) => return Ok(resp),
            Err(e) => {
                // API seems to return 520 in case we're too fast,
                // if that's the case practice exponential backoff
                let is_520 = matches!(&e, ureq::Error::StatusCode(520));
                if is_520 {
                    warn!(
                        "Got 520 for {url} (attempt {}/{}), backing off for {:?}",
                        attempt + 1,
                        MAX_RETRIES,
                        backoff
                    );
                    std::thread::sleep(backoff);
                    backoff *= 2;
                } else {
                    warn!(
                        "Attempt {}/{} failed for {url}: {e:?}",
                        attempt + 1,
                        MAX_RETRIES
                    );
                    // Assume this is a brief transient issue, fixed-duration wait
                    std::thread::sleep(Duration::from_secs(1));
                }
                last_err = Some(e);
            }
        }
    }
    Err(Box::new(last_err.unwrap()))
}
