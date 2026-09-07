// -------------------------------------------------------------------------------------------------
//  Copyright (C) 2015-2026 Nautech Systems Pty Ltd. All rights reserved.
//  https://nautechsystems.io
//
//  Licensed under the GNU Lesser General Public License Version 3.0 (the "License");
// -------------------------------------------------------------------------------------------------

//! Live-node factories for native Alpaca clients.

use std::{any::Any, cell::RefCell, rc::Rc};

use nautilus_common::{
    cache::CacheView,
    clients::{DataClient, ExecutionClient},
    clock::Clock,
    factories::{ClientConfig, DataClientFactory, ExecutionClientFactory},
};
use nautilus_core::string::secret::SecretString;
use nautilus_live::ExecutionClientCore;
use nautilus_model::{
    enums::OmsType,
    identifiers::{ClientId, TraderId},
};

use crate::{
    common::{
        consts::{ALPACA, ALPACA_VENUE},
        credential::AlpacaCredential,
        urls::data_http_url,
    },
    config::{AlpacaDataClientConfig, AlpacaExecutionClientConfig},
    data::AlpacaDataClient,
    execution::AlpacaExecutionClient,
    http::client::AlpacaRawHttpClient,
    provider::AlpacaInstrumentProvider,
    websocket::{AlpacaTradingWebSocketClient, AlpacaWebSocketClient},
};

impl ClientConfig for AlpacaDataClientConfig {
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl ClientConfig for AlpacaExecutionClientConfig {
    fn as_any(&self) -> &dyn Any {
        self
    }
}

fn credential(
    key: &Option<SecretString>,
    secret: &Option<SecretString>,
) -> anyhow::Result<AlpacaCredential> {
    let key = key
        .as_ref()
        .map(SecretString::expose_secret)
        .filter(|value| !value.trim().is_empty())
        .map(ToOwned::to_owned);
    let secret = secret
        .as_ref()
        .map(SecretString::expose_secret)
        .filter(|value| !value.trim().is_empty())
        .map(ToOwned::to_owned);
    AlpacaCredential::resolve(
        key,
        secret,
    ).ok_or_else(|| anyhow::anyhow!("Alpaca credentials unavailable; set ALPACA_API_KEY and ALPACA_API_SECRET or configure both explicitly"))
}

#[derive(Clone, Debug, Default)]
#[cfg_attr(
    feature = "python",
    pyo3::pyclass(module = "nautilus_trader.adapters.alpaca", from_py_object)
)]
#[cfg_attr(
    feature = "python",
    pyo3_stub_gen::derive::gen_stub_pyclass(module = "nautilus_trader.adapters.alpaca")
)]
pub struct AlpacaDataClientFactory;

impl DataClientFactory for AlpacaDataClientFactory {
    fn create(
        &self,
        name: &str,
        config: &dyn ClientConfig,
        _cache: CacheView,
        _clock: Rc<RefCell<dyn Clock>>,
    ) -> anyhow::Result<Box<dyn DataClient>> {
        let config = config
            .as_any()
            .downcast_ref::<AlpacaDataClientConfig>()
            .ok_or_else(|| anyhow::anyhow!("Invalid config type for AlpacaDataClientFactory"))?
            .clone();
        config.validate()?;
        let credential = credential(&config.api_key, &config.api_secret)?;
        let data_http = config.data_http_url();
        let trading_http = config.trading_http_url();
        let proxy = config
            .proxy_url
            .as_ref()
            .map(|value| value.expose_secret().to_owned());
        let http = AlpacaRawHttpClient::with_base_urls_and_proxy(
            credential.clone(),
            data_http,
            trading_http,
            config.http_timeout_secs,
            proxy.clone(),
        )?
        .with_max_retries(config.http_max_retries);
        let provider = AlpacaInstrumentProvider::new(http.clone());
        let ws_url = config.data_ws_url();
        let socket =
            AlpacaWebSocketClient::new(ws_url, credential, config.transport_backend, proxy);
        Ok(Box::new(AlpacaDataClient::new(
            ClientId::from(name),
            config,
            provider,
            http,
            socket,
        )))
    }

    fn name(&self) -> &'static str {
        ALPACA
    }
    fn config_type(&self) -> &'static str {
        "AlpacaDataClientConfig"
    }
}

#[derive(Clone, Debug, Default)]
#[cfg_attr(
    feature = "python",
    pyo3::pyclass(module = "nautilus_trader.adapters.alpaca", from_py_object)
)]
#[cfg_attr(
    feature = "python",
    pyo3_stub_gen::derive::gen_stub_pyclass(module = "nautilus_trader.adapters.alpaca")
)]
pub struct AlpacaExecutionClientFactory;

impl ExecutionClientFactory for AlpacaExecutionClientFactory {
    fn create(
        &self,
        trader_id: TraderId,
        name: &str,
        config: &dyn ClientConfig,
        cache: CacheView,
    ) -> anyhow::Result<Box<dyn ExecutionClient>> {
        let config = config
            .as_any()
            .downcast_ref::<AlpacaExecutionClientConfig>()
            .ok_or_else(|| anyhow::anyhow!("Invalid config type for AlpacaExecutionClientFactory"))?
            .clone();
        config.validate()?;
        let credential = credential(&config.api_key, &config.api_secret)?;
        let trading_http = config.trading_http_url();
        let proxy = config
            .proxy_url
            .as_ref()
            .map(|value| value.expose_secret().to_owned());
        let http = AlpacaRawHttpClient::with_base_urls_and_proxy(
            credential.clone(),
            data_http_url().to_string(),
            trading_http,
            config.http_timeout_secs,
            proxy.clone(),
        )?
        .with_max_retries(config.http_max_retries);
        let ws_url = config.trading_ws_url();
        let socket =
            AlpacaTradingWebSocketClient::new(ws_url, credential, config.transport_backend, proxy);
        let core = ExecutionClientCore::new(
            trader_id,
            ClientId::from(name),
            *ALPACA_VENUE,
            OmsType::Netting,
            config.account_id,
            config.account_type,
            Some(nautilus_model::types::Currency::USD()),
            cache,
        );
        Ok(Box::new(AlpacaExecutionClient::new(
            core, config, http, socket,
        )))
    }

    fn name(&self) -> &'static str {
        ALPACA
    }
    fn config_type(&self) -> &'static str {
        "AlpacaExecutionClientConfig"
    }
}
