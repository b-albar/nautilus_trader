// -------------------------------------------------------------------------------------------------
//  Copyright (C) 2015-2026 Nautech Systems Pty Ltd. All rights reserved.
//  https://nautechsystems.io
//
//  Licensed under the GNU Lesser General Public License Version 3.0 (the "License");
// -------------------------------------------------------------------------------------------------

use nautilus_core::{python::to_pyvalue_err, string::secret::SecretString};
use nautilus_model::{
    enums::AccountType,
    identifiers::{AccountId, InstrumentId},
};
use nautilus_network::websocket::TransportBackend;
use pyo3::{PyResult, pymethods};

use crate::{
    common::enums::{
        AlpacaBarAdjustment, AlpacaDataEnvironment, AlpacaDataFeed, AlpacaEnvironment,
    },
    config::{AlpacaDataClientConfig, AlpacaExecutionClientConfig},
};

#[pymethods]
#[pyo3_stub_gen::derive::gen_stub_pymethods]
impl AlpacaDataClientConfig {
    #[new]
    #[pyo3(signature = (api_key=None, api_secret=None, base_url_data_http=None, base_url_trading_http=None, base_url_data_ws=None, proxy_url=None, instrument_ids=None, data_environment=None, trading_environment=None, feed=None, historical_feed=None, bar_adjustment=None, http_timeout_secs=None, http_max_retries=None, transport_backend=None))]
    #[expect(clippy::too_many_arguments)]
    fn py_new(
        api_key: Option<String>,
        api_secret: Option<String>,
        base_url_data_http: Option<String>,
        base_url_trading_http: Option<String>,
        base_url_data_ws: Option<String>,
        proxy_url: Option<String>,
        instrument_ids: Option<Vec<InstrumentId>>,
        data_environment: Option<AlpacaDataEnvironment>,
        trading_environment: Option<AlpacaEnvironment>,
        feed: Option<AlpacaDataFeed>,
        historical_feed: Option<AlpacaDataFeed>,
        bar_adjustment: Option<AlpacaBarAdjustment>,
        http_timeout_secs: Option<u64>,
        http_max_retries: Option<u32>,
        transport_backend: Option<TransportBackend>,
    ) -> Self {
        let defaults = Self::default();
        Self {
            api_key: api_key.map(SecretString::from),
            api_secret: api_secret.map(SecretString::from),
            base_url_data_http,
            base_url_trading_http,
            base_url_data_ws,
            proxy_url: proxy_url.map(SecretString::from),
            instrument_ids,
            data_environment: data_environment.unwrap_or(defaults.data_environment),
            trading_environment: trading_environment.unwrap_or(defaults.trading_environment),
            feed: feed.unwrap_or(defaults.feed),
            historical_feed,
            bar_adjustment: bar_adjustment.unwrap_or(defaults.bar_adjustment),
            http_timeout_secs: http_timeout_secs.unwrap_or(defaults.http_timeout_secs),
            http_max_retries: http_max_retries.unwrap_or(defaults.http_max_retries),
            transport_backend: transport_backend.unwrap_or(defaults.transport_backend),
        }
    }

    #[getter]
    fn has_credentials(&self) -> bool {
        self.api_key
            .as_ref()
            .is_some_and(|value| !value.expose_secret().trim().is_empty())
            && self
                .api_secret
                .as_ref()
                .is_some_and(|value| !value.expose_secret().trim().is_empty())
    }

    #[getter]
    const fn has_proxy_url(&self) -> bool {
        self.proxy_url.is_some()
    }

    /// Validates this configuration without opening a network connection.
    #[pyo3(name = "validate")]
    fn py_validate(&self) -> PyResult<()> {
        self.validate().map_err(to_pyvalue_err)
    }

    fn __repr__(&self) -> String {
        self.safe_repr()
    }
}

#[pymethods]
#[pyo3_stub_gen::derive::gen_stub_pymethods]
impl AlpacaExecutionClientConfig {
    #[new]
    #[pyo3(signature = (account_id=None, api_key=None, api_secret=None, base_url_trading_http=None, base_url_trading_ws=None, proxy_url=None, environment=None, account_type=None, http_timeout_secs=None, http_max_retries=None, extended_hours=None, transport_backend=None))]
    #[expect(clippy::too_many_arguments)]
    fn py_new(
        account_id: Option<AccountId>,
        api_key: Option<String>,
        api_secret: Option<String>,
        base_url_trading_http: Option<String>,
        base_url_trading_ws: Option<String>,
        proxy_url: Option<String>,
        environment: Option<AlpacaEnvironment>,
        account_type: Option<AccountType>,
        http_timeout_secs: Option<u64>,
        http_max_retries: Option<u32>,
        extended_hours: Option<bool>,
        transport_backend: Option<TransportBackend>,
    ) -> Self {
        let defaults = Self::default();
        Self {
            account_id: account_id.unwrap_or(defaults.account_id),
            api_key: api_key.map(SecretString::from),
            api_secret: api_secret.map(SecretString::from),
            base_url_trading_http,
            base_url_trading_ws,
            proxy_url: proxy_url.map(SecretString::from),
            environment: environment.unwrap_or(defaults.environment),
            account_type: account_type.unwrap_or(defaults.account_type),
            http_timeout_secs: http_timeout_secs.unwrap_or(defaults.http_timeout_secs),
            http_max_retries: http_max_retries.unwrap_or(defaults.http_max_retries),
            extended_hours: extended_hours.unwrap_or(defaults.extended_hours),
            transport_backend: transport_backend.unwrap_or(defaults.transport_backend),
        }
    }

    #[getter]
    fn has_credentials(&self) -> bool {
        self.api_key
            .as_ref()
            .is_some_and(|value| !value.expose_secret().trim().is_empty())
            && self
                .api_secret
                .as_ref()
                .is_some_and(|value| !value.expose_secret().trim().is_empty())
    }

    #[getter]
    const fn has_proxy_url(&self) -> bool {
        self.proxy_url.is_some()
    }

    /// Validates this configuration without opening a network connection.
    #[pyo3(name = "validate")]
    fn py_validate(&self) -> PyResult<()> {
        self.validate().map_err(to_pyvalue_err)
    }

    fn __repr__(&self) -> String {
        self.safe_repr()
    }
}
