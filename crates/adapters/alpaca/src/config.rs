// -------------------------------------------------------------------------------------------------
//  Copyright (C) 2015-2026 Nautech Systems Pty Ltd. All rights reserved.
//  https://nautechsystems.io
//
//  Licensed under the GNU Lesser General Public License Version 3.0 (the "License");
// -------------------------------------------------------------------------------------------------

//! User-facing Alpaca client configuration.

use nautilus_core::string::secret::SecretString;
use nautilus_model::{
    enums::AccountType,
    identifiers::{AccountId, InstrumentId},
};
use nautilus_network::websocket::TransportBackend;
use serde::{Deserialize, Serialize};

use crate::common::enums::{
    AlpacaBarAdjustment, AlpacaDataEnvironment, AlpacaDataFeed, AlpacaEnvironment,
};

#[derive(Clone, Debug, Serialize, Deserialize, bon::Builder)]
#[serde(default, deny_unknown_fields)]
#[cfg_attr(
    feature = "python",
    pyo3::pyclass(module = "nautilus_trader.adapters.alpaca", from_py_object)
)]
#[cfg_attr(
    feature = "python",
    pyo3_stub_gen::derive::gen_stub_pyclass(module = "nautilus_trader.adapters.alpaca")
)]
pub struct AlpacaDataClientConfig {
    pub api_key: Option<SecretString>,
    pub api_secret: Option<SecretString>,
    pub base_url_data_http: Option<String>,
    pub base_url_trading_http: Option<String>,
    pub base_url_data_ws: Option<String>,
    pub proxy_url: Option<SecretString>,
    /// Instruments to load on connect. When omitted, loads Alpaca's full active equity universe.
    pub instrument_ids: Option<Vec<InstrumentId>>,
    #[builder(default)]
    pub data_environment: AlpacaDataEnvironment,
    #[builder(default)]
    pub trading_environment: AlpacaEnvironment,
    #[builder(default)]
    pub feed: AlpacaDataFeed,
    /// Optional historical range feed. Defaults to `feed` when omitted.
    pub historical_feed: Option<AlpacaDataFeed>,
    /// Default corporate-action adjustment for historical bar requests.
    #[builder(default)]
    pub bar_adjustment: AlpacaBarAdjustment,
    #[builder(default = 10)]
    pub http_timeout_secs: u64,
    /// Retries for idempotent HTTP reads. Set to zero for latency-sensitive workflows.
    #[builder(default = 3)]
    pub http_max_retries: u32,
    #[builder(default)]
    pub transport_backend: TransportBackend,
}

impl Default for AlpacaDataClientConfig {
    fn default() -> Self {
        Self::builder().build()
    }
}

impl AlpacaDataClientConfig {
    /// Returns a diagnostic summary which deliberately excludes credentials, proxy values, and
    /// custom URLs because those may contain secrets.
    #[must_use]
    pub fn safe_repr(&self) -> String {
        format!(
            "AlpacaDataClientConfig(data_environment={}, trading_environment={}, feed={}, historical_feed={}, bar_adjustment={}, instrument_count={}, http_timeout_secs={}, http_max_retries={}, transport_backend={:?}, has_credentials={}, has_proxy_url={})",
            self.data_environment,
            self.trading_environment,
            self.feed,
            self.effective_historical_feed(),
            self.bar_adjustment,
            self.instrument_ids.as_ref().map_or(0, Vec::len),
            self.http_timeout_secs,
            self.http_max_retries,
            self.transport_backend,
            self.api_key.is_some() && self.api_secret.is_some(),
            self.proxy_url.is_some(),
        )
    }

    /// Validates configuration combinations before any network connection is attempted.
    ///
    /// # Errors
    ///
    /// Returns an error for unsupported live or historical feeds, empty instrument filters,
    /// cross-venue instrument IDs, or a zero HTTP timeout.
    pub fn validate(&self) -> anyhow::Result<()> {
        anyhow::ensure!(
            self.feed.supports_live_streaming(),
            "Alpaca OTC is historical-only; select a live feed and set historical_feed=OTC"
        );
        anyhow::ensure!(
            self.effective_historical_feed()
                .supports_historical_ranges(),
            "Alpaca historical ranges support IEX, SIP, BOATS, or OTC; configured {}",
            self.effective_historical_feed()
        );
        anyhow::ensure!(
            self.http_timeout_secs > 0,
            "Alpaca HTTP timeout must be positive"
        );
        if let Some(instrument_ids) = &self.instrument_ids {
            anyhow::ensure!(
                !instrument_ids.is_empty(),
                "Alpaca instrument_ids cannot be empty"
            );
            for instrument_id in instrument_ids {
                anyhow::ensure!(
                    instrument_id.venue.as_str() == "ALPACA",
                    "Alpaca instrument_ids require venue ALPACA, received {instrument_id}"
                );
            }
        }
        Ok(())
    }

    #[must_use]
    pub fn data_http_url(&self) -> String {
        self.base_url_data_http
            .clone()
            .unwrap_or_else(|| crate::common::urls::data_http_url().to_string())
    }

    #[must_use]
    pub fn trading_http_url(&self) -> String {
        self.base_url_trading_http.clone().unwrap_or_else(|| {
            crate::common::urls::trading_http_url(self.trading_environment).to_string()
        })
    }

    #[must_use]
    pub fn data_ws_url(&self) -> String {
        self.base_url_data_ws
            .clone()
            .unwrap_or_else(|| crate::common::urls::data_ws_url(self.data_environment, self.feed))
    }

    #[must_use]
    pub fn effective_historical_feed(&self) -> AlpacaDataFeed {
        self.historical_feed.unwrap_or(self.feed)
    }
}

#[cfg(feature = "python")]
nautilus_core::impl_pyo3_config_getters!(AlpacaDataClientConfig {
    base_url_data_http: Option<String>,
    base_url_trading_http: Option<String>,
    base_url_data_ws: Option<String>,
    instrument_ids: Option<Vec<InstrumentId>>,
    data_environment: AlpacaDataEnvironment,
    trading_environment: AlpacaEnvironment,
    feed: AlpacaDataFeed,
    historical_feed: Option<AlpacaDataFeed>,
    bar_adjustment: AlpacaBarAdjustment,
    http_timeout_secs: u64,
    http_max_retries: u32,
    transport_backend: TransportBackend,
});

#[derive(Clone, Debug, Serialize, Deserialize, bon::Builder)]
#[serde(default, deny_unknown_fields)]
#[cfg_attr(
    feature = "python",
    pyo3::pyclass(module = "nautilus_trader.adapters.alpaca", from_py_object)
)]
#[cfg_attr(
    feature = "python",
    pyo3_stub_gen::derive::gen_stub_pyclass(module = "nautilus_trader.adapters.alpaca")
)]
pub struct AlpacaExecutionClientConfig {
    #[builder(default = AccountId::from("ALPACA-001"))]
    pub account_id: AccountId,
    pub api_key: Option<SecretString>,
    pub api_secret: Option<SecretString>,
    pub base_url_trading_http: Option<String>,
    pub base_url_trading_ws: Option<String>,
    pub proxy_url: Option<SecretString>,
    #[builder(default)]
    pub environment: AlpacaEnvironment,
    #[builder(default = AccountType::Margin)]
    pub account_type: AccountType,
    #[builder(default = 10)]
    pub http_timeout_secs: u64,
    /// Retries for idempotent HTTP reads. Trading writes are never retried.
    #[builder(default = 3)]
    pub http_max_retries: u32,
    #[builder(default)]
    pub extended_hours: bool,
    #[builder(default)]
    pub transport_backend: TransportBackend,
}

impl Default for AlpacaExecutionClientConfig {
    fn default() -> Self {
        Self::builder().build()
    }
}

impl AlpacaExecutionClientConfig {
    /// Returns a diagnostic summary which deliberately excludes credentials, proxy values, and
    /// custom URLs because those may contain secrets.
    #[must_use]
    pub fn safe_repr(&self) -> String {
        format!(
            "AlpacaExecutionClientConfig(account_id={}, environment={}, account_type={}, extended_hours={}, http_timeout_secs={}, http_max_retries={}, transport_backend={:?}, has_credentials={}, has_proxy_url={})",
            self.account_id,
            self.environment,
            self.account_type,
            self.extended_hours,
            self.http_timeout_secs,
            self.http_max_retries,
            self.transport_backend,
            self.api_key.is_some() && self.api_secret.is_some(),
            self.proxy_url.is_some(),
        )
    }

    /// Validates execution settings before any network connection is attempted.
    ///
    /// # Errors
    ///
    /// Returns an error for unsupported account types or a zero HTTP timeout.
    pub fn validate(&self) -> anyhow::Result<()> {
        anyhow::ensure!(
            matches!(self.account_type, AccountType::Cash | AccountType::Margin),
            "Alpaca account_type must be Cash or Margin"
        );
        anyhow::ensure!(
            self.http_timeout_secs > 0,
            "Alpaca HTTP timeout must be positive"
        );
        Ok(())
    }

    #[must_use]
    pub fn trading_http_url(&self) -> String {
        self.base_url_trading_http
            .clone()
            .unwrap_or_else(|| crate::common::urls::trading_http_url(self.environment).to_string())
    }

    #[must_use]
    pub fn trading_ws_url(&self) -> String {
        self.base_url_trading_ws
            .clone()
            .unwrap_or_else(|| crate::common::urls::trading_ws_url(self.environment))
    }
}

#[cfg(feature = "python")]
nautilus_core::impl_pyo3_config_getters!(AlpacaExecutionClientConfig {
    account_id: AccountId,
    base_url_trading_http: Option<String>,
    base_url_trading_ws: Option<String>,
    environment: AlpacaEnvironment,
    account_type: AccountType,
    http_timeout_secs: u64,
    http_max_retries: u32,
    extended_hours: bool,
    transport_backend: TransportBackend,
});

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    #[rstest]
    fn test_execution_defaults_are_paper_and_never_implicitly_live() {
        let config = AlpacaExecutionClientConfig::default();
        assert_eq!(config.environment, AlpacaEnvironment::Paper);
        assert_eq!(
            config.trading_http_url(),
            "https://paper-api.alpaca.markets"
        );
        assert_eq!(
            config.trading_ws_url(),
            "wss://paper-api.alpaca.markets/stream"
        );
    }

    #[rstest]
    fn test_live_execution_requires_explicit_environment() {
        let config = AlpacaExecutionClientConfig {
            environment: AlpacaEnvironment::Live,
            ..Default::default()
        };
        assert_eq!(config.trading_http_url(), "https://api.alpaca.markets");
        assert_eq!(config.trading_ws_url(), "wss://api.alpaca.markets/stream");
    }

    #[rstest]
    fn test_data_urls_follow_feed_and_allow_isolated_overrides() {
        let config = AlpacaDataClientConfig {
            feed: AlpacaDataFeed::Sip,
            base_url_data_http: Some("http://history.test".to_string()),
            ..Default::default()
        };
        assert_eq!(config.data_http_url(), "http://history.test");
        assert_eq!(
            config.data_ws_url(),
            "wss://stream.data.alpaca.markets/v2/sip"
        );
        assert_eq!(config.bar_adjustment, AlpacaBarAdjustment::Raw);
    }

    #[rstest]
    fn test_bar_adjustment_deserializes_as_an_explicit_research_choice() {
        let config: AlpacaDataClientConfig =
            serde_json::from_str(r#"{"bar_adjustment":"split"}"#).unwrap();

        assert_eq!(config.bar_adjustment, AlpacaBarAdjustment::Split);
    }

    #[rstest]
    fn test_historical_feed_can_differ_from_live_feed() {
        let config: AlpacaDataClientConfig =
            serde_json::from_str(r#"{"feed":"delayed_sip","historical_feed":"otc"}"#).unwrap();

        assert_eq!(config.feed, AlpacaDataFeed::DelayedSip);
        assert_eq!(config.effective_historical_feed(), AlpacaDataFeed::Otc);
        assert!(config.feed.supports_live_streaming());
        assert!(
            config
                .effective_historical_feed()
                .supports_historical_ranges()
        );
        assert!(!AlpacaDataFeed::Overnight.supports_historical_ranges());
    }

    #[rstest]
    fn test_unknown_config_fields_are_rejected() {
        let error =
            serde_json::from_str::<AlpacaExecutionClientConfig>(r#"{"unexpected_live_flag":true}"#)
                .unwrap_err();
        assert!(error.to_string().contains("unknown field"));
    }

    #[rstest]
    fn test_data_config_validation_fails_before_connect_for_invalid_combinations() {
        let wrong_venue = AlpacaDataClientConfig {
            instrument_ids: Some(vec![InstrumentId::from("AAPL.XNAS")]),
            ..Default::default()
        };
        let no_historical_feed = AlpacaDataClientConfig {
            feed: AlpacaDataFeed::DelayedSip,
            ..Default::default()
        };
        let empty_universe = AlpacaDataClientConfig {
            instrument_ids: Some(Vec::new()),
            ..Default::default()
        };

        assert!(
            wrong_venue
                .validate()
                .unwrap_err()
                .to_string()
                .contains("AAPL.XNAS")
        );
        assert!(
            no_historical_feed
                .validate()
                .unwrap_err()
                .to_string()
                .contains("historical ranges")
        );
        assert!(
            empty_universe
                .validate()
                .unwrap_err()
                .to_string()
                .contains("cannot be empty")
        );
    }

    #[rstest]
    fn test_execution_config_rejects_zero_timeout() {
        let config = AlpacaExecutionClientConfig {
            http_timeout_secs: 0,
            ..Default::default()
        };

        assert_eq!(
            config.validate().unwrap_err().to_string(),
            "Alpaca HTTP timeout must be positive"
        );
    }

    #[rstest]
    fn test_safe_config_representations_are_informative_and_redacted() {
        let data = AlpacaDataClientConfig {
            api_key: Some(SecretString::from("visible-key")),
            api_secret: Some(SecretString::from("visible-secret")),
            base_url_data_http: Some("https://token@data.example".to_string()),
            proxy_url: Some(SecretString::from("https://proxy-secret")),
            instrument_ids: Some(vec![InstrumentId::from("AAPL.ALPACA")]),
            feed: AlpacaDataFeed::Sip,
            ..Default::default()
        };
        let execution = AlpacaExecutionClientConfig {
            api_key: Some(SecretString::from("visible-key")),
            api_secret: Some(SecretString::from("visible-secret")),
            base_url_trading_http: Some("https://token@trading.example".to_string()),
            proxy_url: Some(SecretString::from("https://proxy-secret")),
            extended_hours: true,
            ..Default::default()
        };

        let data_repr = data.safe_repr();
        let execution_repr = execution.safe_repr();

        assert!(data_repr.contains("feed=sip"));
        assert!(data_repr.contains("instrument_count=1"));
        assert!(execution_repr.contains("environment=paper"));
        assert!(execution_repr.contains("extended_hours=true"));
        for secret in ["visible-key", "visible-secret", "token@", "proxy-secret"] {
            assert!(!data_repr.contains(secret));
            assert!(!execution_repr.contains(secret));
        }
    }
}
