use reqwest::{Client, RequestBuilder, Response, StatusCode};
use std::{
    sync::LazyLock,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

const MAX_ATTEMPTS: u32 = 4;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// One pooled client, so requests reuse connections instead of paying for a
/// new TLS handshake each time.
static CLIENT: LazyLock<Client> = LazyLock::new(|| {
    Client::builder()
        .timeout(REQUEST_TIMEOUT)
        .build()
        .expect("HTTP client should build")
});

pub fn client() -> &'static Client {
    &CLIENT
}

pub enum SendError {
    Status(StatusCode),
    Network(reqwest::Error),
}

/// Sends a request and retries rate limits, server errors and network
/// failures with exponential backoff. `build` runs once per attempt.
pub async fn send_with_retry<F>(mut build: F) -> Result<Response, SendError>
where
    F: FnMut() -> RequestBuilder,
{
    let mut attempt = 0;
    loop {
        attempt += 1;
        match build().send().await {
            Ok(response) if response.status().is_success() => return Ok(response),
            Ok(response) if is_retryable(response.status()) && attempt < MAX_ATTEMPTS => {}
            Ok(response) => return Err(SendError::Status(response.status())),
            Err(_) if attempt < MAX_ATTEMPTS => {}
            Err(err) => return Err(SendError::Network(err)),
        }
        tokio::time::sleep(backoff(attempt)).await;
    }
}

fn is_retryable(status: StatusCode) -> bool {
    status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error()
}

fn backoff(attempt: u32) -> Duration {
    let base_ms = (500u64 << (attempt - 1)).min(8_000);
    let jitter_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| u64::from(elapsed.subsec_millis()) % 500)
        .unwrap_or(0);
    Duration::from_millis(base_ms + jitter_ms)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backs_off_exponentially_with_cap() {
        assert!(backoff(1) >= Duration::from_millis(500));
        assert!(backoff(3) >= Duration::from_millis(2_000));
        assert!(backoff(10) < Duration::from_millis(8_500));
    }

    #[test]
    fn retries_only_transient_statuses() {
        assert!(is_retryable(StatusCode::TOO_MANY_REQUESTS));
        assert!(is_retryable(StatusCode::SERVICE_UNAVAILABLE));
        assert!(!is_retryable(StatusCode::UNAUTHORIZED));
        assert!(!is_retryable(StatusCode::BAD_REQUEST));
    }
}
