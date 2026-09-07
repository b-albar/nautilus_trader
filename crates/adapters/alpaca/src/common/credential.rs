// -------------------------------------------------------------------------------------------------
//  Copyright (C) 2015-2026 Nautech Systems Pty Ltd. All rights reserved.
//  https://nautechsystems.io
//
//  Licensed under the GNU Lesser General Public License Version 3.0 (the "License");
// -------------------------------------------------------------------------------------------------

use core::fmt::Debug;
use std::collections::HashMap;

use nautilus_core::{
    env::resolve_env_var_pair,
    string::secret::{REDACTED, mask_api_key},
};
use zeroize::ZeroizeOnDrop;

use super::consts::{ALPACA_API_KEY_HEADER, ALPACA_API_SECRET_HEADER};

#[must_use]
pub const fn credential_env_vars() -> (&'static str, &'static str) {
    ("ALPACA_API_KEY", "ALPACA_API_SECRET")
}

#[derive(Clone, ZeroizeOnDrop)]
pub struct AlpacaCredential {
    api_key: Box<str>,
    api_secret: Box<str>,
}

impl Debug for AlpacaCredential {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct(stringify!(AlpacaCredential))
            .field("api_key", &REDACTED)
            .field("api_secret", &REDACTED)
            .finish()
    }
}

impl AlpacaCredential {
    #[must_use]
    pub fn new(api_key: impl Into<String>, api_secret: impl Into<String>) -> Self {
        Self {
            api_key: api_key.into().into_boxed_str(),
            api_secret: api_secret.into().into_boxed_str(),
        }
    }

    #[must_use]
    pub fn resolve(api_key: Option<String>, api_secret: Option<String>) -> Option<Self> {
        let (key_var, secret_var) = credential_env_vars();
        let (key, secret) = resolve_env_var_pair(api_key, api_secret, key_var, secret_var)?;
        Some(Self::new(key, secret))
    }

    #[must_use]
    pub fn masked_api_key(&self) -> String {
        mask_api_key(&self.api_key)
    }

    #[must_use]
    pub fn api_key(&self) -> &str {
        &self.api_key
    }

    /// Returns the API secret for authentication transport boundaries.
    ///
    /// # Security
    ///
    /// Never log or persist the returned value.
    #[must_use]
    pub fn api_secret(&self) -> &str {
        &self.api_secret
    }

    #[must_use]
    pub fn headers(&self) -> HashMap<String, String> {
        HashMap::from([
            (ALPACA_API_KEY_HEADER.to_string(), self.api_key.to_string()),
            (
                ALPACA_API_SECRET_HEADER.to_string(),
                self.api_secret.to_string(),
            ),
        ])
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    #[rstest]
    fn test_debug_redacts_both_credentials() {
        let credential = AlpacaCredential::new("PKTEST123456", "secret-value");
        let debug = format!("{credential:?}");

        assert_eq!(debug.matches(REDACTED).count(), 2);
        assert!(!debug.contains("PKTEST123456"));
        assert!(!debug.contains("secret-value"));
    }

    #[rstest]
    fn test_headers_use_alpaca_names() {
        let headers = AlpacaCredential::new("key", "secret").headers();

        assert_eq!(
            headers.get(ALPACA_API_KEY_HEADER).map(String::as_str),
            Some("key")
        );
        assert_eq!(
            headers.get(ALPACA_API_SECRET_HEADER).map(String::as_str),
            Some("secret")
        );
    }
}
