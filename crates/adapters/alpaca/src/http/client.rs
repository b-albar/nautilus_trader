// -------------------------------------------------------------------------------------------------
//  Copyright (C) 2015-2026 Nautech Systems Pty Ltd. All rights reserved.
//  https://nautechsystems.io
//
//  Licensed under the GNU Lesser General Public License Version 3.0 (the "License");
// -------------------------------------------------------------------------------------------------

use std::{fmt::Debug, future::Future, sync::Arc};

use nautilus_network::http::{HttpClient, HttpResponse};
use nautilus_network::retry::{RetryConfig, RetryError, RetryManager};
use reqwest::Method;

use super::{
    error::AlpacaHttpError,
    models::{
        AlpacaAccount, AlpacaAsset, AlpacaAuctionsResponse, AlpacaBarsResponse, AlpacaCalendarDay,
        AlpacaCorporateActionsResponse, AlpacaMarketClock, AlpacaMarketMoversResponse,
        AlpacaMostActivesResponse, AlpacaNewsResponse, AlpacaNonTradeActivity,
        AlpacaOptionBarsResponse, AlpacaOptionContract, AlpacaOptionContractsResponse,
        AlpacaOptionSnapshotsResponse, AlpacaOptionTradesResponse, AlpacaOrder, AlpacaOrderRequest,
        AlpacaPortfolioHistory, AlpacaPosition, AlpacaQuotesResponse, AlpacaReplaceOrderRequest,
        AlpacaStockSnapshot, AlpacaTradeActivity, AlpacaTradesResponse, AlpacaWatchlist,
    },
    query::{
        AlpacaActivitiesQuery, AlpacaAssetsQuery, AlpacaBarsQuery, AlpacaCalendarQuery,
        AlpacaClientOrderQuery, AlpacaConditionsQuery, AlpacaCorporateActionsQuery,
        AlpacaEmptyQuery, AlpacaFeedQuery, AlpacaMarketMoversQuery, AlpacaMostActivesQuery,
        AlpacaNewsQuery, AlpacaOptionBarsQuery, AlpacaOptionChainQuery, AlpacaOptionContractsQuery,
        AlpacaOptionSnapshotsQuery, AlpacaOptionTradesQuery, AlpacaOrderQuery, AlpacaOrdersQuery,
        AlpacaPortfolioHistoryQuery, AlpacaSnapshotsQuery, AlpacaTicksQuery,
        AlpacaWatchlistNameQuery,
    },
};
use crate::common::{
    credential::AlpacaCredential,
    enums::AlpacaEnvironment,
    urls::{data_http_url, trading_http_url},
};

/// Low-level authenticated Alpaca REST client.
#[derive(Clone)]
pub struct AlpacaRawHttpClient {
    base_url: String,
    trading_base_url: String,
    client: HttpClient,
    credential: AlpacaCredential,
    retry_manager: Arc<RetryManager<AlpacaHttpError>>,
}

impl Debug for AlpacaRawHttpClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct(stringify!(AlpacaRawHttpClient))
            .field("base_url", &self.base_url)
            .field("trading_base_url", &self.trading_base_url)
            .field("api_key", &self.credential.masked_api_key())
            .finish()
    }
}

impl AlpacaRawHttpClient {
    fn default_retry_config() -> RetryConfig {
        RetryConfig {
            max_retries: 3,
            initial_delay_ms: 250,
            max_delay_ms: 5_000,
            backoff_factor: 2.0,
            jitter_ms: 100,
            operation_timeout_ms: None,
            immediate_first: false,
            max_elapsed_ms: Some(30_000),
        }
    }

    async fn retry_read<F, Fut, T>(
        &self,
        operation_name: &str,
        operation: F,
    ) -> Result<T, AlpacaHttpError>
    where
        F: FnMut() -> Fut,
        Fut: Future<Output = Result<T, AlpacaHttpError>>,
    {
        self.retry_manager
            .execute_with_retry_with_delay(
                operation_name,
                operation,
                AlpacaHttpError::is_retryable,
                AlpacaHttpError::retry_after,
                |error: RetryError| AlpacaHttpError::Transport(error.to_string()),
            )
            .await
    }

    fn trading_url(&self, path: &str) -> String {
        format!("{}{}", self.trading_base_url.trim_end_matches('/'), path)
    }

    fn trading_headers(&self) -> std::collections::HashMap<String, String> {
        let mut headers = self.credential.headers();
        headers.insert("Content-Type".to_string(), "application/json".to_string());
        headers
    }

    /// Creates a client for the Alpaca market data API.
    ///
    /// # Errors
    ///
    /// Returns an error when the shared HTTP transport cannot be constructed.
    pub fn new(
        credential: AlpacaCredential,
        base_url: Option<String>,
        timeout_secs: u64,
    ) -> Result<Self, AlpacaHttpError> {
        Self::new_with_proxy(credential, base_url, timeout_secs, None)
    }

    /// Creates a client for the Alpaca market data API with an optional HTTP proxy.
    ///
    /// # Errors
    ///
    /// Returns an error when the shared HTTP transport or proxy configuration is invalid.
    pub fn new_with_proxy(
        credential: AlpacaCredential,
        base_url: Option<String>,
        timeout_secs: u64,
        proxy_url: Option<String>,
    ) -> Result<Self, AlpacaHttpError> {
        let client = HttpClient::builder()
            .timeout_secs(timeout_secs)
            .maybe_proxy_url(proxy_url)
            .header_keys(vec![
                "retry-after".to_string(),
                "x-ratelimit-reset".to_string(),
            ])
            .build()
            .map_err(|error| AlpacaHttpError::Transport(error.to_string()))?;

        Ok(Self {
            base_url: base_url.unwrap_or_else(|| data_http_url().to_string()),
            trading_base_url: trading_http_url(AlpacaEnvironment::Paper).to_string(),
            client,
            credential,
            retry_manager: Arc::new(RetryManager::new(Self::default_retry_config())),
        })
    }

    /// Creates a client with explicit data and trading URLs for environment selection or tests.
    ///
    /// # Errors
    ///
    /// Returns an error when the shared HTTP transport cannot be constructed.
    pub fn with_base_urls(
        credential: AlpacaCredential,
        data_base_url: String,
        trading_base_url: String,
        timeout_secs: u64,
    ) -> Result<Self, AlpacaHttpError> {
        let mut client = Self::new(credential, Some(data_base_url), timeout_secs)?;
        client.trading_base_url = trading_base_url;
        Ok(client)
    }

    /// Creates a client with explicit URLs and an optional proxy.
    ///
    /// # Errors
    ///
    /// Returns an error when the shared HTTP transport or proxy configuration is invalid.
    pub fn with_base_urls_and_proxy(
        credential: AlpacaCredential,
        data_base_url: String,
        trading_base_url: String,
        timeout_secs: u64,
        proxy_url: Option<String>,
    ) -> Result<Self, AlpacaHttpError> {
        let mut client =
            Self::new_with_proxy(credential, Some(data_base_url), timeout_secs, proxy_url)?;
        client.trading_base_url = trading_base_url;
        Ok(client)
    }

    /// Overrides the number of retries for idempotent HTTP reads.
    #[must_use]
    pub fn with_max_retries(mut self, max_retries: u32) -> Self {
        let mut config = Self::default_retry_config();
        config.max_retries = max_retries;
        self.retry_manager = Arc::new(RetryManager::new(config));
        self
    }

    #[must_use]
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    #[must_use]
    pub fn trading_base_url(&self) -> &str {
        &self.trading_base_url
    }

    /// Fetches the Alpaca asset master used for instrument bootstrap.
    ///
    /// # Errors
    ///
    /// Returns a transport, HTTP status, authentication, rate-limit, or decode error.
    pub async fn get_assets(
        &self,
        query: &AlpacaAssetsQuery<'_>,
    ) -> Result<Vec<AlpacaAsset>, AlpacaHttpError> {
        self.retry_read("get Alpaca assets", || async {
            let response = self
                .client
                .request_with_params(
                    Method::GET,
                    format!("{}/v2/assets", self.trading_base_url.trim_end_matches('/')),
                    Some(query),
                    Some(self.credential.headers()),
                    None,
                    None,
                    Some(vec!["alpaca:trading".to_string()]),
                )
                .await?;

            if !response.status.is_success() {
                return Err(AlpacaHttpError::from_http_response(
                    response.status.as_u16(),
                    &response.body,
                    &response.headers,
                ));
            }

            serde_json::from_slice(&response.body).map_err(AlpacaHttpError::from)
        })
        .await
    }

    /// Fetches one asset by its symbol or Alpaca asset ID.
    ///
    /// # Errors
    ///
    /// Returns a transport, HTTP status, authentication, rate-limit, or decode error.
    pub async fn get_asset(
        &self,
        symbol_or_asset_id: &str,
    ) -> Result<AlpacaAsset, AlpacaHttpError> {
        self.retry_read("get Alpaca asset", || async {
            let response = self
                .client
                .get(
                    format!(
                        "{}/v2/assets/{symbol_or_asset_id}",
                        self.trading_base_url.trim_end_matches('/')
                    ),
                    None,
                    Some(self.credential.headers()),
                    None,
                    Some(vec!["alpaca:trading".to_string()]),
                )
                .await?;

            if !response.status.is_success() {
                return Err(AlpacaHttpError::from_http_response(
                    response.status.as_u16(),
                    &response.body,
                    &response.headers,
                ));
            }

            serde_json::from_slice(&response.body).map_err(AlpacaHttpError::from)
        })
        .await
    }

    /// Fetches one option contract by OCC symbol or Alpaca contract ID.
    pub async fn get_option_contract(
        &self,
        symbol_or_contract_id: &str,
    ) -> Result<AlpacaOptionContract, AlpacaHttpError> {
        self.retry_read("get Alpaca option contract", || async {
            let response = self
                .client
                .get(
                    self.trading_url(&format!("/v2/options/contracts/{symbol_or_contract_id}")),
                    None,
                    Some(self.credential.headers()),
                    None,
                    Some(vec!["alpaca:trading".to_string()]),
                )
                .await?;
            decode_trading_response(&response)
        })
        .await
    }

    /// Fetches one cursor-paginated page of option contracts.
    pub async fn get_option_contracts(
        &self,
        query: &AlpacaOptionContractsQuery<'_>,
    ) -> Result<AlpacaOptionContractsResponse, AlpacaHttpError> {
        self.retry_read("get Alpaca option contracts", || async {
            let response = self
                .client
                .request_with_params(
                    Method::GET,
                    self.trading_url("/v2/options/contracts"),
                    Some(query),
                    Some(self.credential.headers()),
                    None,
                    None,
                    Some(vec!["alpaca:trading".to_string()]),
                )
                .await?;
            decode_trading_response(&response)
        })
        .await
    }

    /// Fetches every watchlist registered under the configured account.
    pub async fn get_watchlists(&self) -> Result<Vec<AlpacaWatchlist>, AlpacaHttpError> {
        self.retry_read("get Alpaca watchlists", || async {
            let response = self
                .client
                .get(
                    self.trading_url("/v2/watchlists"),
                    None,
                    Some(self.credential.headers()),
                    None,
                    Some(vec!["alpaca:trading".to_string()]),
                )
                .await?;
            decode_trading_response(&response)
        })
        .await
    }

    /// Fetches one watchlist by its Alpaca ID.
    pub async fn get_watchlist(
        &self,
        watchlist_id: &str,
    ) -> Result<AlpacaWatchlist, AlpacaHttpError> {
        self.retry_read("get Alpaca watchlist", || async {
            let response = self
                .client
                .get(
                    self.trading_url(&format!("/v2/watchlists/{watchlist_id}")),
                    None,
                    Some(self.credential.headers()),
                    None,
                    Some(vec!["alpaca:trading".to_string()]),
                )
                .await?;
            decode_trading_response(&response)
        })
        .await
    }

    /// Fetches one watchlist by its user-visible name.
    pub async fn get_watchlist_by_name(
        &self,
        name: &str,
    ) -> Result<AlpacaWatchlist, AlpacaHttpError> {
        self.retry_read("get Alpaca watchlist by name", || async {
            let response = self
                .client
                .request_with_params(
                    Method::GET,
                    self.trading_url("/v2/watchlists:by_name"),
                    Some(&AlpacaWatchlistNameQuery { name }),
                    Some(self.credential.headers()),
                    None,
                    None,
                    Some(vec!["alpaca:trading".to_string()]),
                )
                .await?;
            decode_trading_response(&response)
        })
        .await
    }

    /// Fetches one page from `GET /v2/stocks/bars`.
    ///
    /// Pagination is explicit: pass the returned token in the next query so callers retain
    /// control over request bounds and cancellation.
    ///
    /// # Errors
    ///
    /// Returns a transport, HTTP status, authentication, rate-limit, or decode error.
    pub async fn get_stock_bars(
        &self,
        query: &AlpacaBarsQuery<'_>,
    ) -> Result<AlpacaBarsResponse, AlpacaHttpError> {
        self.retry_read("get Alpaca stock bars", || async {
            let response = self
                .client
                .request_with_params(
                    Method::GET,
                    format!("{}/v2/stocks/bars", self.base_url.trim_end_matches('/')),
                    Some(query),
                    Some(self.credential.headers()),
                    None,
                    None,
                    Some(vec!["alpaca:data".to_string()]),
                )
                .await?;

            if !response.status.is_success() {
                return Err(AlpacaHttpError::from_http_response(
                    response.status.as_u16(),
                    &response.body,
                    &response.headers,
                ));
            }

            serde_json::from_slice(&response.body).map_err(AlpacaHttpError::from)
        })
        .await
    }

    pub async fn get_stock_trades(
        &self,
        query: &AlpacaTicksQuery<'_>,
    ) -> Result<AlpacaTradesResponse, AlpacaHttpError> {
        self.get_market_data("/v2/stocks/trades", query).await
    }

    pub async fn get_stock_quotes(
        &self,
        query: &AlpacaTicksQuery<'_>,
    ) -> Result<AlpacaQuotesResponse, AlpacaHttpError> {
        self.get_market_data("/v2/stocks/quotes", query).await
    }

    /// Fetches one cursor-paginated page of historical opening and closing auctions.
    pub async fn get_stock_auctions(
        &self,
        query: &AlpacaTicksQuery<'_>,
    ) -> Result<AlpacaAuctionsResponse, AlpacaHttpError> {
        self.get_market_data("/v2/stocks/auctions", query).await
    }

    /// Fetches the latest trade, quote, minute bar, daily bar, and previous daily bar for a symbol.
    pub async fn get_stock_snapshot(
        &self,
        symbol: &str,
        feed: crate::common::enums::AlpacaDataFeed,
    ) -> Result<AlpacaStockSnapshot, AlpacaHttpError> {
        self.retry_read("get Alpaca stock snapshot", || async {
            let response = self
                .client
                .request_with_params(
                    Method::GET,
                    format!(
                        "{}/v2/stocks/{symbol}/snapshot",
                        self.base_url.trim_end_matches('/')
                    ),
                    Some(&AlpacaFeedQuery { feed }),
                    Some(self.credential.headers()),
                    None,
                    None,
                    Some(vec!["alpaca:data".to_string()]),
                )
                .await?;
            if !response.status.is_success() {
                return Err(AlpacaHttpError::from_http_response(
                    response.status.as_u16(),
                    &response.body,
                    &response.headers,
                ));
            }
            serde_json::from_slice(&response.body).map_err(AlpacaHttpError::from)
        })
        .await
    }

    /// Fetches the latest consolidated market state for multiple US equity symbols.
    pub async fn get_stock_snapshots(
        &self,
        query: &AlpacaSnapshotsQuery<'_>,
    ) -> Result<std::collections::BTreeMap<String, AlpacaStockSnapshot>, AlpacaHttpError> {
        self.retry_read("get Alpaca stock snapshots", || async {
            let response = self
                .client
                .request_with_params(
                    Method::GET,
                    format!(
                        "{}/v2/stocks/snapshots",
                        self.base_url.trim_end_matches('/')
                    ),
                    Some(query),
                    Some(self.credential.headers()),
                    None,
                    None,
                    Some(vec!["alpaca:data".to_string()]),
                )
                .await?;
            if !response.status.is_success() {
                return Err(AlpacaHttpError::from_http_response(
                    response.status.as_u16(),
                    &response.body,
                    &response.headers,
                ));
            }
            serde_json::from_slice(&response.body).map_err(AlpacaHttpError::from)
        })
        .await
    }

    /// Fetches one page of latest option snapshots and Greeks for an underlying symbol.
    pub async fn get_option_chain(
        &self,
        underlying_symbol: &str,
        query: &AlpacaOptionChainQuery<'_>,
    ) -> Result<AlpacaOptionSnapshotsResponse, AlpacaHttpError> {
        self.get_data(
            &format!("/v1beta1/options/snapshots/{underlying_symbol}"),
            query,
            "get Alpaca option chain",
        )
        .await
    }

    /// Fetches one page of latest option snapshots for explicit contract symbols.
    pub async fn get_option_snapshots(
        &self,
        query: &AlpacaOptionSnapshotsQuery<'_>,
    ) -> Result<AlpacaOptionSnapshotsResponse, AlpacaHttpError> {
        self.get_data(
            "/v1beta1/options/snapshots",
            query,
            "get Alpaca option snapshots",
        )
        .await
    }

    /// Fetches one page of historical option bars grouped by contract symbol.
    pub async fn get_option_bars(
        &self,
        query: &AlpacaOptionBarsQuery<'_>,
    ) -> Result<AlpacaOptionBarsResponse, AlpacaHttpError> {
        self.get_data(
            "/v1beta1/options/bars",
            query,
            "get Alpaca historical option bars",
        )
        .await
    }

    /// Fetches one page of historical option trades grouped by contract symbol.
    pub async fn get_option_trades(
        &self,
        query: &AlpacaOptionTradesQuery<'_>,
    ) -> Result<AlpacaOptionTradesResponse, AlpacaHttpError> {
        self.get_data(
            "/v1beta1/options/trades",
            query,
            "get Alpaca historical option trades",
        )
        .await
    }

    /// Fetches the current most-active US equities ranked by volume or trade count.
    pub async fn get_most_actives(
        &self,
        query: &AlpacaMostActivesQuery<'_>,
    ) -> Result<AlpacaMostActivesResponse, AlpacaHttpError> {
        self.get_data(
            "/v1beta1/screener/stocks/most-actives",
            query,
            "get Alpaca most actives",
        )
        .await
    }

    /// Fetches the current top US equity gainers and losers.
    pub async fn get_stock_movers(
        &self,
        query: &AlpacaMarketMoversQuery,
    ) -> Result<AlpacaMarketMoversResponse, AlpacaHttpError> {
        self.get_data(
            "/v1beta1/screener/stocks/movers",
            query,
            "get Alpaca stock movers",
        )
        .await
    }

    /// Fetches stock exchange code descriptions.
    pub async fn get_stock_exchanges(
        &self,
    ) -> Result<std::collections::BTreeMap<String, String>, AlpacaHttpError> {
        self.get_data(
            "/v2/stocks/meta/exchanges",
            &AlpacaEmptyQuery {},
            "get Alpaca stock exchanges",
        )
        .await
    }

    /// Fetches trade or quote condition descriptions for one SIP tape.
    pub async fn get_stock_conditions(
        &self,
        tick_type: &str,
        query: &AlpacaConditionsQuery<'_>,
    ) -> Result<std::collections::BTreeMap<String, String>, AlpacaHttpError> {
        self.get_data(
            &format!("/v2/stocks/meta/conditions/{tick_type}"),
            query,
            "get Alpaca stock conditions",
        )
        .await
    }

    /// Fetches one cursor-paginated page of historical news articles.
    pub async fn get_news(
        &self,
        query: &AlpacaNewsQuery<'_>,
    ) -> Result<AlpacaNewsResponse, AlpacaHttpError> {
        self.retry_read("get Alpaca news", || async {
            let response = self
                .client
                .request_with_params(
                    Method::GET,
                    format!("{}/v1beta1/news", self.base_url.trim_end_matches('/')),
                    Some(query),
                    Some(self.credential.headers()),
                    None,
                    None,
                    Some(vec!["alpaca:data".to_string()]),
                )
                .await?;
            if !response.status.is_success() {
                return Err(AlpacaHttpError::from_http_response(
                    response.status.as_u16(),
                    &response.body,
                    &response.headers,
                ));
            }
            serde_json::from_slice(&response.body).map_err(AlpacaHttpError::from)
        })
        .await
    }

    /// Fetches one cursor-paginated page of structured corporate actions.
    pub async fn get_corporate_actions(
        &self,
        query: &AlpacaCorporateActionsQuery<'_>,
    ) -> Result<AlpacaCorporateActionsResponse, AlpacaHttpError> {
        self.retry_read("get Alpaca corporate actions", || async {
            let response = self
                .client
                .request_with_params(
                    Method::GET,
                    format!(
                        "{}/v1/corporate-actions",
                        self.base_url.trim_end_matches('/')
                    ),
                    Some(query),
                    Some(self.credential.headers()),
                    None,
                    None,
                    Some(vec!["alpaca:data".to_string()]),
                )
                .await?;
            if !response.status.is_success() {
                return Err(AlpacaHttpError::from_http_response(
                    response.status.as_u16(),
                    &response.body,
                    &response.headers,
                ));
            }
            serde_json::from_slice(&response.body).map_err(AlpacaHttpError::from)
        })
        .await
    }

    async fn get_data<T, Q>(
        &self,
        path: &str,
        query: &Q,
        operation: &'static str,
    ) -> Result<T, AlpacaHttpError>
    where
        T: serde::de::DeserializeOwned,
        Q: serde::Serialize,
    {
        self.retry_read(operation, || async {
            let response = self
                .client
                .request_with_params(
                    Method::GET,
                    format!("{}{path}", self.base_url.trim_end_matches('/')),
                    Some(query),
                    Some(self.credential.headers()),
                    None,
                    None,
                    Some(vec!["alpaca:data".to_string()]),
                )
                .await?;
            if !response.status.is_success() {
                return Err(AlpacaHttpError::from_http_response(
                    response.status.as_u16(),
                    &response.body,
                    &response.headers,
                ));
            }
            serde_json::from_slice(&response.body).map_err(AlpacaHttpError::from)
        })
        .await
    }

    async fn get_market_data<T: serde::de::DeserializeOwned>(
        &self,
        path: &str,
        query: &AlpacaTicksQuery<'_>,
    ) -> Result<T, AlpacaHttpError> {
        self.retry_read("get Alpaca market data", || async {
            let response = self
                .client
                .request_with_params(
                    Method::GET,
                    format!("{}{path}", self.base_url.trim_end_matches('/')),
                    Some(query),
                    Some(self.credential.headers()),
                    None,
                    None,
                    Some(vec!["alpaca:data".to_string()]),
                )
                .await?;
            if !response.status.is_success() {
                return Err(AlpacaHttpError::from_http_response(
                    response.status.as_u16(),
                    &response.body,
                    &response.headers,
                ));
            }
            serde_json::from_slice(&response.body).map_err(AlpacaHttpError::from)
        })
        .await
    }

    /// Fetches the trading account associated with the configured API key.
    pub async fn get_account(&self) -> Result<AlpacaAccount, AlpacaHttpError> {
        self.retry_read("get Alpaca account", || async {
            let response = self
                .client
                .get(
                    self.trading_url("/v2/account"),
                    None,
                    Some(self.credential.headers()),
                    None,
                    Some(vec!["alpaca:trading".to_string()]),
                )
                .await?;
            decode_trading_response(&response)
        })
        .await
    }

    /// Fetches the current US equity market status and next open/close boundaries.
    pub async fn get_market_clock(&self) -> Result<AlpacaMarketClock, AlpacaHttpError> {
        self.retry_read("get Alpaca market clock", || async {
            let response = self
                .client
                .get(
                    self.trading_url("/v2/clock"),
                    None,
                    Some(self.credential.headers()),
                    None,
                    Some(vec!["alpaca:trading".to_string()]),
                )
                .await?;
            decode_trading_response(&response)
        })
        .await
    }

    /// Fetches account equity and profit/loss points over the requested period.
    pub async fn get_portfolio_history(
        &self,
        query: &AlpacaPortfolioHistoryQuery<'_>,
    ) -> Result<AlpacaPortfolioHistory, AlpacaHttpError> {
        self.retry_read("get Alpaca portfolio history", || async {
            let response = self
                .client
                .request_with_params(
                    Method::GET,
                    self.trading_url("/v2/account/portfolio/history"),
                    Some(query),
                    Some(self.credential.headers()),
                    None,
                    None,
                    Some(vec!["alpaca:trading".to_string()]),
                )
                .await?;
            decode_trading_response(&response)
        })
        .await
    }

    /// Fetches US equity sessions in an optional inclusive ISO-date range.
    pub async fn get_calendar(
        &self,
        query: &AlpacaCalendarQuery<'_>,
    ) -> Result<Vec<AlpacaCalendarDay>, AlpacaHttpError> {
        self.retry_read("get Alpaca market calendar", || async {
            let response = self
                .client
                .request_with_params(
                    Method::GET,
                    self.trading_url("/v2/calendar"),
                    Some(query),
                    Some(self.credential.headers()),
                    None,
                    None,
                    Some(vec!["alpaca:trading".to_string()]),
                )
                .await?;
            decode_trading_response(&response)
        })
        .await
    }

    /// Submits an order to the configured paper or live trading environment.
    pub async fn submit_order(
        &self,
        request: &AlpacaOrderRequest,
    ) -> Result<AlpacaOrder, AlpacaHttpError> {
        let response = self
            .client
            .post(
                self.trading_url("/v2/orders"),
                None,
                Some(self.trading_headers()),
                Some(serde_json::to_vec(request)?),
                None,
                Some(vec!["alpaca:trading".to_string()]),
            )
            .await?;
        decode_trading_response(&response)
    }

    /// Cancels an order by its Alpaca venue order ID.
    pub async fn cancel_order(&self, order_id: &str) -> Result<(), AlpacaHttpError> {
        let response = self
            .client
            .delete(
                self.trading_url(&format!("/v2/orders/{order_id}")),
                None,
                Some(self.credential.headers()),
                None,
                Some(vec!["alpaca:trading".to_string()]),
            )
            .await?;
        if !response.status.is_success() {
            return Err(AlpacaHttpError::from_http_response(
                response.status.as_u16(),
                &response.body,
                &response.headers,
            ));
        }
        Ok(())
    }

    /// Replaces the mutable fields of an existing order.
    pub async fn replace_order(
        &self,
        order_id: &str,
        request: &AlpacaReplaceOrderRequest,
    ) -> Result<AlpacaOrder, AlpacaHttpError> {
        let response = self
            .client
            .patch(
                self.trading_url(&format!("/v2/orders/{order_id}")),
                None,
                Some(self.trading_headers()),
                Some(serde_json::to_vec(request)?),
                None,
                Some(vec!["alpaca:trading".to_string()]),
            )
            .await?;
        decode_trading_response(&response)
    }

    pub async fn get_orders(
        &self,
        query: &AlpacaOrdersQuery<'_>,
    ) -> Result<Vec<AlpacaOrder>, AlpacaHttpError> {
        self.retry_read("get Alpaca orders", || async {
            let response = self
                .client
                .request_with_params(
                    Method::GET,
                    self.trading_url("/v2/orders"),
                    Some(query),
                    Some(self.credential.headers()),
                    None,
                    None,
                    Some(vec!["alpaca:trading".to_string()]),
                )
                .await?;
            decode_trading_response(&response)
        })
        .await
    }

    pub async fn get_order(&self, order_id: &str) -> Result<AlpacaOrder, AlpacaHttpError> {
        self.get_order_with_legs(order_id, false).await
    }

    /// Retrieves an order and optionally rolls advanced-order children into `legs`.
    pub async fn get_order_with_legs(
        &self,
        order_id: &str,
        nested: bool,
    ) -> Result<AlpacaOrder, AlpacaHttpError> {
        self.retry_read("get Alpaca order", || async {
            let response = self
                .client
                .request_with_params(
                    Method::GET,
                    self.trading_url(&format!("/v2/orders/{order_id}")),
                    Some(&AlpacaOrderQuery { nested }),
                    Some(self.credential.headers()),
                    None,
                    None,
                    Some(vec!["alpaca:trading".to_string()]),
                )
                .await?;
            decode_trading_response(&response)
        })
        .await
    }

    pub async fn get_order_by_client_order_id(
        &self,
        client_order_id: &str,
    ) -> Result<AlpacaOrder, AlpacaHttpError> {
        self.retry_read("get Alpaca order by client order ID", || async {
            let response = self
                .client
                .request_with_params(
                    Method::GET,
                    self.trading_url("/v2/orders:by_client_order_id"),
                    Some(&AlpacaClientOrderQuery { client_order_id }),
                    Some(self.credential.headers()),
                    None,
                    None,
                    Some(vec!["alpaca:trading".to_string()]),
                )
                .await?;
            decode_trading_response(&response)
        })
        .await
    }

    pub async fn get_positions(&self) -> Result<Vec<AlpacaPosition>, AlpacaHttpError> {
        self.retry_read("get Alpaca positions", || async {
            let response = self
                .client
                .get(
                    self.trading_url("/v2/positions"),
                    None,
                    Some(self.credential.headers()),
                    None,
                    Some(vec!["alpaca:trading".to_string()]),
                )
                .await?;
            decode_trading_response(&response)
        })
        .await
    }

    pub async fn get_fill_activities(
        &self,
        query: &AlpacaActivitiesQuery<'_>,
    ) -> Result<Vec<AlpacaTradeActivity>, AlpacaHttpError> {
        self.retry_read("get Alpaca fill activities", || async {
            let response = self
                .client
                .request_with_params(
                    Method::GET,
                    self.trading_url("/v2/account/activities/FILL"),
                    Some(query),
                    Some(self.credential.headers()),
                    None,
                    None,
                    Some(vec!["alpaca:trading".to_string()]),
                )
                .await?;
            decode_trading_response(&response)
        })
        .await
    }

    /// Retrieves cash-impacting account activities such as `FEE`, `DIV`, or `INT`.
    pub async fn get_non_trade_activities(
        &self,
        activity_type: &str,
        query: &AlpacaActivitiesQuery<'_>,
    ) -> Result<Vec<AlpacaNonTradeActivity>, AlpacaHttpError> {
        if activity_type.is_empty()
            || !activity_type
                .bytes()
                .all(|value| value.is_ascii_uppercase() || value.is_ascii_digit())
        {
            return Err(AlpacaHttpError::Transport(
                "Alpaca activity type must contain only uppercase ASCII letters or digits"
                    .to_string(),
            ));
        }
        self.retry_read("get Alpaca non-trade activities", || async {
            let response = self
                .client
                .request_with_params(
                    Method::GET,
                    self.trading_url(&format!("/v2/account/activities/{activity_type}")),
                    Some(query),
                    Some(self.credential.headers()),
                    None,
                    None,
                    Some(vec!["alpaca:trading".to_string()]),
                )
                .await?;
            decode_trading_response(&response)
        })
        .await
    }

    /// Retrieves a bounded non-trade activity range while following Alpaca page cursors.
    pub async fn get_all_non_trade_activities(
        &self,
        activity_type: &str,
        start: Option<jiff::Timestamp>,
        end: Option<jiff::Timestamp>,
        order_id: Option<&str>,
        max_items: usize,
    ) -> Result<Vec<AlpacaNonTradeActivity>, AlpacaHttpError> {
        if max_items == 0 || max_items > 100_000 {
            return Err(AlpacaHttpError::Transport(
                "Alpaca activity max_items must be between 1 and 100000".to_string(),
            ));
        }
        if start.zip(end).is_some_and(|(start, end)| start >= end) {
            return Err(AlpacaHttpError::Transport(
                "Alpaca activity start must be earlier than end".to_string(),
            ));
        }

        let mut activities = Vec::new();
        let mut cursor = None::<String>;
        loop {
            let remaining = max_items - activities.len();
            let page_size = remaining.min(100) as u8;
            let query = AlpacaActivitiesQuery {
                direction: "asc",
                page_size,
                after: start,
                until: end,
                order_id,
                page_token: cursor.as_deref(),
            };
            let page = self.get_non_trade_activities(activity_type, &query).await?;
            let page_len = page.len();
            let next_cursor = page.last().map(|activity| activity.id.clone());
            if page_len == usize::from(page_size) && next_cursor == cursor {
                return Err(AlpacaHttpError::Transport(
                    "Alpaca activity pagination cursor did not advance".to_string(),
                ));
            }
            activities.extend(page.into_iter().take(remaining));
            if activities.len() >= max_items || page_len < usize::from(page_size) {
                break;
            }
            cursor = next_cursor;
        }
        Ok(activities)
    }
}

fn decode_trading_response<T: serde::de::DeserializeOwned>(
    response: &HttpResponse,
) -> Result<T, AlpacaHttpError> {
    if !response.status.is_success() {
        return Err(AlpacaHttpError::from_http_response(
            response.status.as_u16(),
            &response.body,
            &response.headers,
        ));
    }
    serde_json::from_slice(&response.body).map_err(AlpacaHttpError::from)
}

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    };

    use axum::{
        Json, Router,
        body::Bytes,
        extract::{Query, State},
        http::{HeaderMap, StatusCode},
        response::{IntoResponse, Response},
        routing::{delete, get, patch},
    };
    use rstest::rstest;
    use rust_decimal::Decimal;
    use serde_json::{Value, json};
    use tokio::net::TcpListener;

    use super::*;

    #[rstest]
    fn test_debug_masks_api_key() {
        let client = AlpacaRawHttpClient::new(
            AlpacaCredential::new("PKTEST123456", "secret"),
            Some("http://127.0.0.1:1234".to_string()),
            10,
        )
        .unwrap();
        let debug = format!("{client:?}");

        assert!(debug.contains("PKTE...3456"));
        assert!(!debug.contains("PKTEST123456"));
        assert!(!debug.contains("secret"));
    }

    #[rstest]
    fn test_invalid_http_proxy_is_rejected_during_construction() {
        let error = AlpacaRawHttpClient::new_with_proxy(
            AlpacaCredential::new("test-key", "test-secret"),
            None,
            10,
            Some("://invalid-proxy".to_string()),
        )
        .unwrap_err();

        assert!(matches!(error, AlpacaHttpError::Transport(_)));
    }

    #[derive(Clone, Debug, Default)]
    struct TradingTestState {
        submitted: Arc<Mutex<Option<Value>>>,
        replaced: Arc<Mutex<Option<Value>>>,
        canceled: Arc<Mutex<Option<String>>>,
    }

    fn assert_auth(headers: &HeaderMap) {
        assert_eq!(headers["apca-api-key-id"], "test-key");
        assert_eq!(headers["apca-api-secret-key"], "test-secret");
    }

    async fn account_handler(headers: HeaderMap) -> Json<Value> {
        assert_auth(&headers);
        Json(json!({
            "id":"account-id", "account_number":"PA123", "status":"ACTIVE",
            "currency":"USD", "cash":"10000", "buying_power":"20000",
            "equity":"11000", "portfolio_value":"11000", "long_market_value":"1000",
            "short_market_value":"0", "pattern_day_trader":false,
            "trading_blocked":false, "transfers_blocked":false, "account_blocked":false,
            "trade_suspended_by_user":false, "shorting_enabled":true, "multiplier":"2",
            "created_at":"2024-01-01T00:00:00Z"
        }))
    }

    fn account_json() -> Value {
        json!({
            "id":"account-id", "account_number":"PA123", "status":"ACTIVE",
            "currency":"USD", "cash":"10000", "buying_power":"20000",
            "equity":"11000", "portfolio_value":"11000", "long_market_value":"1000",
            "short_market_value":"0", "pattern_day_trader":false,
            "trading_blocked":false, "transfers_blocked":false, "account_blocked":false,
            "trade_suspended_by_user":false, "shorting_enabled":true, "multiplier":"2",
            "created_at":"2024-01-01T00:00:00Z"
        })
    }

    async fn transient_account_handler(State(attempts): State<Arc<AtomicUsize>>) -> Response {
        if attempts.fetch_add(1, Ordering::SeqCst) == 0 {
            (
                StatusCode::TOO_MANY_REQUESTS,
                [("retry-after", "0")],
                Json(json!({"message":"slow down"})),
            )
                .into_response()
        } else {
            (StatusCode::OK, Json(account_json())).into_response()
        }
    }

    async fn unauthorized_account_handler(
        State(attempts): State<Arc<AtomicUsize>>,
    ) -> impl IntoResponse {
        attempts.fetch_add(1, Ordering::SeqCst);
        (
            StatusCode::UNAUTHORIZED,
            Json(json!({"message":"invalid key"})),
        )
    }

    async fn spawn_account_server(
        handler: axum::routing::MethodRouter<Arc<AtomicUsize>>,
        attempts: Arc<AtomicUsize>,
    ) -> (std::net::SocketAddr, tokio::task::JoinHandle<()>) {
        let app = Router::new()
            .route("/v2/account", handler)
            .with_state(attempts);
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        (address, server)
    }

    #[rstest]
    #[tokio::test]
    async fn test_idempotent_read_retries_rate_limit() {
        let attempts = Arc::new(AtomicUsize::new(0));
        let (address, server) =
            spawn_account_server(get(transient_account_handler), Arc::clone(&attempts)).await;
        let client = AlpacaRawHttpClient::with_base_urls(
            AlpacaCredential::new("test-key", "test-secret"),
            format!("http://{address}"),
            format!("http://{address}"),
            10,
        )
        .unwrap();

        let account = client.get_account().await.unwrap();

        assert_eq!(account.id, "account-id");
        assert_eq!(attempts.load(Ordering::SeqCst), 2);
        server.abort();
    }

    #[rstest]
    #[tokio::test]
    async fn test_idempotent_read_does_not_retry_authentication_failure() {
        let attempts = Arc::new(AtomicUsize::new(0));
        let (address, server) =
            spawn_account_server(get(unauthorized_account_handler), Arc::clone(&attempts)).await;
        let client = AlpacaRawHttpClient::with_base_urls(
            AlpacaCredential::new("test-key", "test-secret"),
            format!("http://{address}"),
            format!("http://{address}"),
            10,
        )
        .unwrap();

        let error = client.get_account().await.unwrap_err();

        assert!(matches!(error, AlpacaHttpError::Authentication(_)));
        assert_eq!(attempts.load(Ordering::SeqCst), 1);
        server.abort();
    }

    #[rstest]
    #[tokio::test]
    async fn test_idempotent_read_retries_can_be_disabled() {
        let attempts = Arc::new(AtomicUsize::new(0));
        let (address, server) =
            spawn_account_server(get(transient_account_handler), Arc::clone(&attempts)).await;
        let client = AlpacaRawHttpClient::with_base_urls(
            AlpacaCredential::new("test-key", "test-secret"),
            format!("http://{address}"),
            format!("http://{address}"),
            10,
        )
        .unwrap()
        .with_max_retries(0);

        let error = client.get_account().await.unwrap_err();

        assert!(matches!(
            error,
            AlpacaHttpError::RateLimited {
                retry_after: Some(delay)
            } if delay.is_zero()
        ));
        assert_eq!(attempts.load(Ordering::SeqCst), 1);
        server.abort();
    }

    async fn submit_handler(
        State(state): State<TradingTestState>,
        headers: HeaderMap,
        body: Bytes,
    ) -> Json<Value> {
        assert_auth(&headers);
        assert_eq!(headers["content-type"], "application/json");
        let request: Value = serde_json::from_slice(&body).unwrap();
        *state.submitted.lock().unwrap() = Some(request.clone());
        Json(json!({
            "id":"venue-order-id", "client_order_id":request["client_order_id"],
            "symbol":request["symbol"], "asset_class":"us_equity", "qty":request["qty"],
            "filled_qty":"0", "filled_avg_price":null, "side":request["side"],
            "type":request["type"], "time_in_force":request["time_in_force"],
            "limit_price":request.get("limit_price"), "stop_price":request.get("stop_price"),
            "status":"accepted", "extended_hours":false,
            "created_at":"2024-01-01T00:00:00Z", "updated_at":"2024-01-01T00:00:00Z",
            "submitted_at":"2024-01-01T00:00:00Z", "filled_at":null, "canceled_at":null,
            "expired_at":null, "failed_at":null
        }))
    }

    async fn cancel_handler(
        State(state): State<TradingTestState>,
        headers: HeaderMap,
        axum::extract::Path(order_id): axum::extract::Path<String>,
    ) -> StatusCode {
        assert_auth(&headers);
        *state.canceled.lock().unwrap() = Some(order_id);
        StatusCode::NO_CONTENT
    }

    async fn replace_handler(
        State(state): State<TradingTestState>,
        headers: HeaderMap,
        axum::extract::Path(order_id): axum::extract::Path<String>,
        body: Bytes,
    ) -> Json<Value> {
        assert_auth(&headers);
        assert_eq!(order_id, "venue-order-id");
        let request: Value = serde_json::from_slice(&body).unwrap();
        *state.replaced.lock().unwrap() = Some(request.clone());
        Json(json!({
            "id":"replacement-order-id", "client_order_id":"client-order-id",
            "symbol":"AAPL", "asset_class":"us_equity", "qty":request["qty"],
            "filled_qty":"0", "filled_avg_price":null, "side":"buy", "type":"limit",
            "time_in_force":"day", "limit_price":request["limit_price"], "stop_price":null,
            "status":"accepted", "extended_hours":false,
            "created_at":"2024-01-01T00:00:00Z", "updated_at":"2024-01-01T00:00:02Z",
            "submitted_at":"2024-01-01T00:00:02Z", "filled_at":null, "canceled_at":null,
            "expired_at":null, "failed_at":null
        }))
    }

    async fn orders_handler(
        headers: HeaderMap,
        Query(query): Query<std::collections::HashMap<String, String>>,
    ) -> Json<Value> {
        assert_auth(&headers);
        assert_eq!(query.get("status").map(String::as_str), Some("all"));
        assert_eq!(query.get("limit").map(String::as_str), Some("500"));
        Json(json!([{
            "id":"venue-order-id", "client_order_id":"client-order-id", "symbol":"AAPL",
            "asset_class":"us_equity", "qty":"1", "filled_qty":"1",
            "filled_avg_price":"185.25", "side":"buy", "type":"market",
            "time_in_force":"day", "limit_price":null, "stop_price":null, "status":"filled",
            "extended_hours":false, "created_at":"2024-01-01T00:00:00Z",
            "updated_at":"2024-01-01T00:00:01Z", "submitted_at":"2024-01-01T00:00:00Z",
            "filled_at":"2024-01-01T00:00:01Z", "canceled_at":null, "expired_at":null,
            "failed_at":null
        }]))
    }

    async fn client_order_handler(
        headers: HeaderMap,
        Query(query): Query<std::collections::HashMap<String, String>>,
    ) -> Json<Value> {
        assert_auth(&headers);
        assert_eq!(
            query.get("client_order_id").map(String::as_str),
            Some("client-order-id")
        );
        Json(json!({
            "id":"venue-order-id", "client_order_id":"client-order-id", "symbol":"AAPL",
            "asset_class":"us_equity", "qty":"1", "filled_qty":"0",
            "filled_avg_price":null, "side":"buy", "type":"limit",
            "time_in_force":"day", "limit_price":"185.25", "stop_price":null,
            "status":"accepted", "extended_hours":false,
            "created_at":"2024-01-01T00:00:00Z", "updated_at":"2024-01-01T00:00:01Z",
            "submitted_at":"2024-01-01T00:00:00Z", "filled_at":null,
            "canceled_at":null, "expired_at":null, "failed_at":null
        }))
    }

    async fn order_handler(
        headers: HeaderMap,
        axum::extract::Path(order_id): axum::extract::Path<String>,
        Query(query): Query<std::collections::HashMap<String, String>>,
    ) -> Json<Value> {
        assert_auth(&headers);
        assert_eq!(order_id, "venue-order-id");
        assert_eq!(query.get("nested").map(String::as_str), Some("true"));
        Json(json!({
            "id":"venue-order-id", "client_order_id":"client-order-id", "symbol":"AAPL",
            "asset_class":"us_equity", "qty":"1", "filled_qty":"0",
            "filled_avg_price":null, "side":"buy", "type":"market",
            "time_in_force":"day", "limit_price":null, "stop_price":null,
            "status":"accepted", "extended_hours":false,
            "created_at":"2024-01-01T00:00:00Z", "updated_at":"2024-01-01T00:00:01Z",
            "submitted_at":"2024-01-01T00:00:00Z", "filled_at":null,
            "canceled_at":null, "expired_at":null, "failed_at":null, "legs":[]
        }))
    }

    async fn positions_handler(headers: HeaderMap) -> Json<Value> {
        assert_auth(&headers);
        Json(json!([{
            "asset_id":"asset-1", "symbol":"AAPL", "asset_class":"us_equity", "qty":"1",
            "side":"long", "avg_entry_price":"185.25", "market_value":"186",
            "cost_basis":"185.25", "unrealized_pl":"0.75", "unrealized_plpc":"0.00404858",
            "current_price":"186", "lastday_price":"184", "change_today":"0.01086957"
        }]))
    }

    async fn fills_handler(
        headers: HeaderMap,
        Query(query): Query<std::collections::HashMap<String, String>>,
    ) -> Json<Value> {
        assert_auth(&headers);
        assert_eq!(query.get("direction").map(String::as_str), Some("asc"));
        assert_eq!(query.get("page_size").map(String::as_str), Some("100"));
        Json(json!([{
            "activity_type":"FILL", "id":"timestamp::fill-1", "order_id":"venue-order-id",
            "symbol":"AAPL", "side":"buy", "qty":"1", "price":"185.25",
            "cum_qty":"1", "leaves_qty":"0", "type":"fill",
            "transaction_time":"2024-01-01T00:00:01Z"
        }]))
    }

    async fn fees_handler(
        headers: HeaderMap,
        Query(query): Query<std::collections::HashMap<String, String>>,
    ) -> Json<Value> {
        assert_auth(&headers);
        assert_eq!(query.get("direction").map(String::as_str), Some("asc"));
        Json(json!([{
            "activity_type":"FEE", "activity_sub_type":"TAF", "id":"fee-1",
            "date":"2026-01-16", "net_amount":"-0.0125", "symbol":"AAPL", "qty":"25"
        }]))
    }

    async fn paginated_fees_handler(
        headers: HeaderMap,
        Query(query): Query<std::collections::HashMap<String, String>>,
    ) -> Json<Value> {
        assert_auth(&headers);
        if let Some(cursor) = query.get("page_token") {
            assert_eq!(cursor, "fee-099");
            assert_eq!(query.get("page_size").map(String::as_str), Some("1"));
            return Json(json!([{
                "activity_type":"FEE", "activity_sub_type":"REG", "id":"fee-100",
                "date":"2026-01-17", "net_amount":"-0.0001"
            }]));
        }
        assert_eq!(query.get("page_size").map(String::as_str), Some("100"));
        Json(Value::Array(
            (0..100)
                .map(|index| {
                    json!({
                        "activity_type":"FEE", "activity_sub_type":"REG",
                        "id":format!("fee-{index:03}"), "date":"2026-01-16",
                        "net_amount":"-0.0001"
                    })
                })
                .collect(),
        ))
    }

    async fn historical_trades_handler(
        headers: HeaderMap,
        Query(query): Query<std::collections::HashMap<String, String>>,
    ) -> Json<Value> {
        assert_auth(&headers);
        assert_eq!(query.get("symbols").map(String::as_str), Some("AAPL"));
        assert_eq!(query.get("feed").map(String::as_str), Some("iex"));
        assert_eq!(query.get("sort").map(String::as_str), Some("asc"));
        Json(json!({
            "trades":{"AAPL":[{"i":123,"p":"187.125","s":"0.25","t":"2024-01-01T00:00:01Z"}]},
            "next_page_token":null
        }))
    }

    async fn historical_quotes_handler(
        headers: HeaderMap,
        Query(query): Query<std::collections::HashMap<String, String>>,
    ) -> Json<Value> {
        assert_auth(&headers);
        assert_eq!(query.get("symbols").map(String::as_str), Some("AAPL"));
        Json(json!({
            "quotes":{"AAPL":[{"bp":"187.10","bs":"2","ap":"187.15","as":"3","t":"2024-01-01T00:00:01Z"}]},
            "next_page_token":null
        }))
    }

    #[rstest]
    #[tokio::test]
    async fn test_trading_account_submit_and_cancel_round_trip() {
        let state = TradingTestState::default();
        let app = Router::new()
            .route("/v2/account", get(account_handler))
            .route("/v2/orders", get(orders_handler).post(submit_handler))
            .route("/v2/orders:by_client_order_id", get(client_order_handler))
            .route(
                "/v2/orders/{order_id}",
                get(order_handler)
                    .merge(delete(cancel_handler))
                    .merge(patch(replace_handler)),
            )
            .route("/v2/positions", get(positions_handler))
            .route("/v2/account/activities/FILL", get(fills_handler))
            .route("/v2/account/activities/FEE", get(fees_handler))
            .route("/v2/stocks/trades", get(historical_trades_handler))
            .route("/v2/stocks/quotes", get(historical_quotes_handler))
            .with_state(state.clone());
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let client = AlpacaRawHttpClient::with_base_urls(
            AlpacaCredential::new("test-key", "test-secret"),
            format!("http://{address}"),
            format!("http://{address}"),
            10,
        )
        .unwrap();
        let request = AlpacaOrderRequest {
            symbol: "AAPL".to_string(),
            qty: rust_decimal::Decimal::ONE,
            side: crate::http::models::AlpacaOrderSide::Buy,
            order_type: crate::http::models::AlpacaOrderType::Market,
            time_in_force: crate::http::models::AlpacaTimeInForce::Day,
            client_order_id: "client-order-id".to_string(),
            limit_price: None,
            stop_price: None,
            trail_price: None,
            trail_percent: None,
            extended_hours: false,
            order_class: None,
            take_profit: None,
            stop_loss: None,
        };

        let account = client.get_account().await.unwrap();
        let order = client.submit_order(&request).await.unwrap();
        let replacement = client
            .replace_order(
                &order.id,
                &AlpacaReplaceOrderRequest {
                    qty: Some(Decimal::from(2)),
                    limit_price: Some(Decimal::new(18_750, 2)),
                    stop_price: None,
                },
            )
            .await
            .unwrap();
        client.cancel_order(&order.id).await.unwrap();
        let orders = client
            .get_orders(&AlpacaOrdersQuery {
                status: "all",
                limit: 500,
                direction: "desc",
                nested: false,
                after: None,
                until: None,
                symbols: None,
                before_order_id: None,
            })
            .await
            .unwrap();
        let queried = client
            .get_order_by_client_order_id("client-order-id")
            .await
            .unwrap();
        let nested = client
            .get_order_with_legs("venue-order-id", true)
            .await
            .unwrap();
        let positions = client.get_positions().await.unwrap();
        let fills = client
            .get_fill_activities(&AlpacaActivitiesQuery::default())
            .await
            .unwrap();
        let fees = client
            .get_non_trade_activities("FEE", &AlpacaActivitiesQuery::default())
            .await
            .unwrap();
        let start: jiff::Timestamp = "2024-01-01T00:00:00Z".parse().unwrap();
        let end: jiff::Timestamp = "2024-01-02T00:00:00Z".parse().unwrap();
        let ticks_query = AlpacaTicksQuery::new(
            "AAPL",
            start,
            end,
            crate::common::enums::AlpacaDataFeed::Iex,
        );
        let trades = client.get_stock_trades(&ticks_query).await.unwrap();
        let quotes = client.get_stock_quotes(&ticks_query).await.unwrap();

        assert_eq!(account.account_number, "PA123");
        assert_eq!(order.id, "venue-order-id");
        assert_eq!(replacement.id, "replacement-order-id");
        assert_eq!(orders.len(), 1);
        assert_eq!(queried.id, "venue-order-id");
        assert_eq!(nested.legs, Some(Vec::new()));
        assert_eq!(positions[0].symbol, "AAPL");
        assert_eq!(fills[0].id, "timestamp::fill-1");
        assert_eq!(fees[0].net_amount, Decimal::new(-125, 4));
        assert_eq!(trades.trades["AAPL"][0].price, Decimal::new(187_125, 3));
        assert_eq!(quotes.quotes["AAPL"][0].bid_size_lots, Decimal::from(2));
        assert_eq!(
            state.submitted.lock().unwrap().as_ref().unwrap()["client_order_id"],
            "client-order-id"
        );
        assert_eq!(
            state.canceled.lock().unwrap().as_deref(),
            Some("venue-order-id")
        );
        let replaced = state.replaced.lock().unwrap();
        let replaced = replaced.as_ref().unwrap();
        assert_eq!(replaced["qty"], "2");
        assert_eq!(replaced["limit_price"], "187.50");
        assert!(replaced.get("stop_price").is_none());
        server.abort();
    }

    #[rstest]
    #[tokio::test]
    async fn test_non_trade_activities_follow_cursor_and_honor_exact_bound() {
        let app = Router::new().route("/v2/account/activities/FEE", get(paginated_fees_handler));
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let client = AlpacaRawHttpClient::with_base_urls(
            AlpacaCredential::new("test-key", "test-secret"),
            format!("http://{address}"),
            format!("http://{address}"),
            10,
        )
        .unwrap();

        let activities = client
            .get_all_non_trade_activities("FEE", None, None, None, 101)
            .await
            .unwrap();

        assert_eq!(activities.len(), 101);
        assert_eq!(activities.first().unwrap().id, "fee-000");
        assert_eq!(activities.last().unwrap().id, "fee-100");
        server.abort();
    }
}
