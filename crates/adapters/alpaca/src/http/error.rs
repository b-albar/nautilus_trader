// -------------------------------------------------------------------------------------------------
//  Copyright (C) 2015-2026 Nautech Systems Pty Ltd. All rights reserved.
//  https://nautechsystems.io
//
//  Licensed under the GNU Lesser General Public License Version 3.0 (the "License");
// -------------------------------------------------------------------------------------------------

use std::{collections::HashMap, time::Duration};

use nautilus_network::http::HttpClientError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AlpacaHttpError {
    #[error("transport failed: {0}")]
    Transport(String),
    #[error("authentication failed: {0}")]
    Authentication(String),
    #[error("rate limited")]
    RateLimited { retry_after: Option<Duration> },
    #[error("HTTP {status}: {message}")]
    Http { status: u16, message: String },
    #[error("response decoding failed: {0}")]
    Decode(#[from] serde_json::Error),
}

impl From<HttpClientError> for AlpacaHttpError {
    fn from(value: HttpClientError) -> Self {
        Self::Transport(value.to_string())
    }
}

impl AlpacaHttpError {
    #[must_use]
    pub fn from_http_status(status: u16, body: &[u8]) -> Self {
        let message = String::from_utf8_lossy(body).into_owned();
        match status {
            401 | 403 => Self::Authentication(message),
            429 => Self::RateLimited { retry_after: None },
            _ => Self::Http { status, message },
        }
    }

    #[must_use]
    pub const fn is_retryable(&self) -> bool {
        matches!(
            self,
            Self::Transport(_)
                | Self::RateLimited { .. }
                | Self::Http {
                    status: 500..=599,
                    ..
                }
        )
    }

    /// Returns whether a failed trading write may still have reached Alpaca.
    ///
    /// HTTP 4xx responses are definitive rejections. Transport failures and 5xx responses are
    /// ambiguous because the connection can fail after the broker has accepted the command.
    #[must_use]
    pub const fn is_ambiguous_write(&self) -> bool {
        matches!(
            self,
            Self::Transport(_)
                | Self::Http {
                    status: 500..=599,
                    ..
                }
        )
    }

    #[must_use]
    pub fn from_http_response(status: u16, body: &[u8], headers: &HashMap<String, String>) -> Self {
        if status != 429 {
            return Self::from_http_status(status, body);
        }
        Self::RateLimited {
            retry_after: rate_limit_delay(headers),
        }
    }

    #[must_use]
    pub const fn retry_after(&self) -> Option<Duration> {
        match self {
            Self::RateLimited { retry_after } => *retry_after,
            _ => None,
        }
    }
}

fn rate_limit_delay(headers: &HashMap<String, String>) -> Option<Duration> {
    if let Some(seconds) = headers
        .get("retry-after")
        .and_then(|value| value.parse::<u64>().ok())
    {
        return Some(Duration::from_secs(seconds));
    }
    let reset = headers.get("x-ratelimit-reset")?.parse::<u64>().ok()?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_secs();
    Some(Duration::from_secs(reset.saturating_sub(now)))
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    #[rstest]
    #[case(AlpacaHttpError::Transport("connection reset".to_string()), true)]
    #[case(AlpacaHttpError::Http { status: 503, message: "unavailable".to_string() }, true)]
    #[case(AlpacaHttpError::RateLimited { retry_after: None }, false)]
    #[case(AlpacaHttpError::Authentication("invalid key".to_string()), false)]
    #[case(AlpacaHttpError::Http { status: 422, message: "invalid order".to_string() }, false)]
    fn test_ambiguous_write_classification(#[case] error: AlpacaHttpError, #[case] expected: bool) {
        assert_eq!(error.is_ambiguous_write(), expected);
    }

    #[rstest]
    fn test_rate_limit_delay_prefers_retry_after() {
        let headers = HashMap::from([
            ("retry-after".to_string(), "2".to_string()),
            ("x-ratelimit-reset".to_string(), "9999999999".to_string()),
        ]);

        let error = AlpacaHttpError::from_http_response(429, b"limited", &headers);

        assert_eq!(error.retry_after(), Some(Duration::from_secs(2)));
    }
}
