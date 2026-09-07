// -------------------------------------------------------------------------------------------------
//  Copyright (C) 2015-2026 Nautech Systems Pty Ltd. All rights reserved.
//  https://nautechsystems.io
//
//  Licensed under the GNU Lesser General Public License Version 3.0 (the "License");
// -------------------------------------------------------------------------------------------------

use std::collections::{BTreeMap, HashSet};

use nautilus_core::python::{params::value_to_pyobject, to_pyvalue_err};
use pyo3::prelude::*;

use crate::{
    common::{
        credential::AlpacaCredential,
        enums::{AlpacaBarAdjustment, AlpacaDataFeed, AlpacaEnvironment},
        urls::{data_http_url, trading_http_url},
    },
    http::{
        client::AlpacaRawHttpClient,
        query::{
            AlpacaActivitiesQuery, AlpacaAssetsQuery, AlpacaBarsQuery, AlpacaCalendarQuery,
            AlpacaConditionsQuery, AlpacaCorporateActionsQuery, AlpacaMarketMoversQuery,
            AlpacaMostActivesQuery, AlpacaNewsQuery, AlpacaOptionBarsQuery, AlpacaOptionChainQuery,
            AlpacaOptionContractsQuery, AlpacaOptionSnapshotsQuery, AlpacaOptionTradesQuery,
            AlpacaOrdersQuery, AlpacaPortfolioHistoryQuery, AlpacaSnapshotsQuery, AlpacaTicksQuery,
        },
    },
};

const ALPACA_ASSET_ATTRIBUTES: &[&str] = &[
    "ptp_no_exception",
    "ptp_with_exception",
    "ipo",
    "has_options",
    "options_late_close",
    "fractional_eh_enabled",
    "overnight_tradable",
    "overnight_halted",
];

fn build_read_client(
    api_key: Option<String>,
    api_secret: Option<String>,
    environment: AlpacaEnvironment,
    base_url: Option<String>,
    timeout_secs: u64,
    max_retries: u32,
    proxy_url: Option<String>,
) -> PyResult<AlpacaRawHttpClient> {
    if timeout_secs == 0 {
        return Err(to_pyvalue_err("Alpaca HTTP timeout must be positive"));
    }
    let credential = AlpacaCredential::resolve(api_key, api_secret).ok_or_else(|| {
        to_pyvalue_err(
            "Alpaca credentials unavailable; pass both values or set ALPACA_API_KEY and ALPACA_API_SECRET",
        )
    })?;
    AlpacaRawHttpClient::with_base_urls_and_proxy(
        credential,
        data_http_url().to_string(),
        base_url.unwrap_or_else(|| trading_http_url(environment).to_string()),
        timeout_secs,
        proxy_url,
    )
    .map(|client| client.with_max_retries(max_retries))
    .map_err(to_pyvalue_err)
}

fn build_historical_client(
    api_key: Option<String>,
    api_secret: Option<String>,
    base_url: Option<String>,
    timeout_secs: u64,
    max_retries: u32,
    proxy_url: Option<String>,
) -> PyResult<AlpacaRawHttpClient> {
    if timeout_secs == 0 {
        return Err(to_pyvalue_err("Alpaca HTTP timeout must be positive"));
    }
    let credential = AlpacaCredential::resolve(api_key, api_secret).ok_or_else(|| {
        to_pyvalue_err(
            "Alpaca credentials unavailable; pass both values or set ALPACA_API_KEY and ALPACA_API_SECRET",
        )
    })?;
    AlpacaRawHttpClient::with_base_urls_and_proxy(
        credential,
        base_url.unwrap_or_else(|| data_http_url().to_string()),
        trading_http_url(AlpacaEnvironment::Paper).to_string(),
        timeout_secs,
        proxy_url,
    )
    .map(|client| client.with_max_retries(max_retries))
    .map_err(to_pyvalue_err)
}

fn historical_range(start: &str, end: &str) -> PyResult<(jiff::Timestamp, jiff::Timestamp)> {
    let start = start
        .parse::<jiff::Timestamp>()
        .map_err(|error| to_pyvalue_err(format!("Invalid Alpaca historical start: {error}")))?;
    let end = end
        .parse::<jiff::Timestamp>()
        .map_err(|error| to_pyvalue_err(format!("Invalid Alpaca historical end: {error}")))?;
    if start >= end {
        return Err(to_pyvalue_err(
            "Alpaca historical start must be earlier than end",
        ));
    }
    Ok((start, end))
}

fn validate_historical_request(
    symbol: &str,
    feed: AlpacaDataFeed,
    max_items: usize,
) -> PyResult<()> {
    validate_equity_symbol(symbol)?;
    if !feed.supports_historical_ranges() {
        return Err(to_pyvalue_err(format!(
            "Alpaca feed '{feed}' does not support historical ranges"
        )));
    }
    if max_items == 0 || max_items > 1_000_000 {
        return Err(to_pyvalue_err(
            "Alpaca historical max_items must be between 1 and 1000000",
        ));
    }
    Ok(())
}

fn validate_equity_symbol(symbol: &str) -> PyResult<()> {
    if symbol.trim().is_empty()
        || symbol.trim() != symbol
        || symbol.contains(',')
        || symbol.contains('/')
    {
        return Err(to_pyvalue_err(
            "Alpaca symbol must be one non-empty equity symbol without whitespace, ',' or '/'",
        ));
    }
    Ok(())
}

fn validate_csv(value: Option<&str>, label: &str) -> PyResult<()> {
    if value.is_some_and(|value| {
        value.is_empty()
            || value
                .split(',')
                .any(|item| item.is_empty() || item.trim() != item)
    }) {
        return Err(to_pyvalue_err(format!(
            "Alpaca {label} must be a comma-separated list without empty values or whitespace"
        )));
    }
    Ok(())
}

fn join_unique_symbols(symbols: Vec<String>, label: &str) -> PyResult<String> {
    if symbols.is_empty() {
        return Err(to_pyvalue_err(format!(
            "Alpaca {label} require at least one equity symbol"
        )));
    }
    for symbol in &symbols {
        validate_equity_symbol(symbol)?;
    }
    let mut unique = HashSet::with_capacity(symbols.len());
    if symbols.iter().any(|symbol| !unique.insert(symbol)) {
        return Err(to_pyvalue_err(format!(
            "Alpaca {label} symbols must not contain duplicates"
        )));
    }
    Ok(symbols
        .into_iter()
        .reduce(|mut joined, symbol| {
            joined.push(',');
            joined.push_str(&symbol);
            joined
        })
        .expect("non-empty symbols validated"))
}

fn join_option_symbols(symbols: Vec<String>, label: &str) -> PyResult<String> {
    if symbols.is_empty() || symbols.len() > 100 {
        return Err(to_pyvalue_err(format!(
            "Alpaca {label} require between 1 and 100 contract symbols"
        )));
    }
    if symbols.iter().any(|symbol| {
        symbol.is_empty()
            || symbol.len() > 32
            || !symbol
                .bytes()
                .all(|value| value.is_ascii_uppercase() || value.is_ascii_digit())
    }) {
        return Err(to_pyvalue_err(
            "Alpaca option symbols must contain only uppercase ASCII letters and digits",
        ));
    }
    let mut unique = HashSet::with_capacity(symbols.len());
    if symbols.iter().any(|symbol| !unique.insert(symbol)) {
        return Err(to_pyvalue_err(format!(
            "Alpaca {label} symbols must not contain duplicates"
        )));
    }
    Ok(symbols
        .into_iter()
        .reduce(|mut joined, symbol| {
            joined.push(',');
            joined.push_str(&symbol);
            joined
        })
        .expect("non-empty option symbols validated"))
}

fn truncate_grouped<T>(values: &mut BTreeMap<String, Vec<T>>, max_items: usize) {
    let mut remaining = max_items;
    for items in values.values_mut() {
        items.truncate(remaining);
        remaining -= items.len();
    }
}

fn validate_asof(asof: Option<&str>) -> PyResult<()> {
    if let Some(value) = asof
        && value != "-"
    {
        value.parse::<jiff::civil::Date>().map_err(|e| {
            to_pyvalue_err(format!(
                "Invalid Alpaca historical asof date; use YYYY-MM-DD or '-': {e}"
            ))
        })?;
    }
    Ok(())
}

fn validate_historical_timeframe(value: &str) -> PyResult<()> {
    let valid = [
        ("Month", &[1_u8, 2, 3, 4, 6, 12][..]),
        ("Hour", &[1_u8][..]),
        ("Week", &[1_u8][..]),
        ("Day", &[1_u8][..]),
        ("Min", &[1_u8][..]),
        ("M", &[1_u8, 2, 3, 4, 6, 12][..]),
        ("H", &[1_u8][..]),
        ("W", &[1_u8][..]),
        ("D", &[1_u8][..]),
        ("T", &[1_u8][..]),
    ]
    .into_iter()
    .any(|(suffix, discrete)| {
        let Some(prefix) = value.strip_suffix(suffix) else {
            return false;
        };
        let Ok(step) = prefix.parse::<u8>() else {
            return false;
        };
        match suffix {
            "Min" | "T" => (1..=59).contains(&step),
            "Hour" | "H" => (1..=23).contains(&step),
            _ => discrete.contains(&step),
        }
    });
    if !valid {
        return Err(to_pyvalue_err(
            "Invalid Alpaca timeframe; use 1-59Min, 1-23Hour, 1Day, 1Week, or 1/2/3/4/6/12Month (T/H/D/W/M aliases are accepted)",
        ));
    }
    Ok(())
}

fn validate_option_type(value: Option<&str>) -> PyResult<()> {
    if value.is_some_and(|value| !matches!(value, "call" | "put")) {
        return Err(to_pyvalue_err("Alpaca option type must be 'call' or 'put'"));
    }
    Ok(())
}

fn validate_option_date(value: Option<&str>, label: &str) -> PyResult<()> {
    if let Some(value) = value {
        value.parse::<jiff::civil::Date>().map_err(|e| {
            to_pyvalue_err(format!(
                "Invalid Alpaca option {label}; use YYYY-MM-DD: {e}"
            ))
        })?;
    }
    Ok(())
}

fn validate_option_strikes(gte: Option<&str>, lte: Option<&str>) -> PyResult<()> {
    let parse = |value: Option<&str>, label: &str| -> PyResult<Option<rust_decimal::Decimal>> {
        value
            .map(|value| {
                value
                    .parse::<rust_decimal::Decimal>()
                    .map_err(|e| to_pyvalue_err(format!("Invalid Alpaca option {label}: {e}")))
            })
            .transpose()
    };
    let gte = parse(gte, "minimum strike")?;
    let lte = parse(lte, "maximum strike")?;
    if gte.is_some_and(|value| value.is_sign_negative())
        || lte.is_some_and(|value| value.is_sign_negative())
    {
        return Err(to_pyvalue_err("Alpaca option strikes must be non-negative"));
    }
    if let (Some(gte), Some(lte)) = (gte, lte)
        && gte > lte
    {
        return Err(to_pyvalue_err(
            "Alpaca option minimum strike must not exceed maximum strike",
        ));
    }
    Ok(())
}

fn validate_option_history_limit(max_items: usize) -> PyResult<()> {
    if max_items == 0 || max_items > 1_000_000 {
        return Err(to_pyvalue_err(
            "Alpaca historical option max_items must be between 1 and 1000000",
        ));
    }
    Ok(())
}

fn validate_option_updated_since(value: Option<&str>) -> PyResult<()> {
    if let Some(value) = value
        && value.parse::<jiff::Timestamp>().is_err()
        && value.parse::<jiff::civil::Date>().is_err()
    {
        return Err(to_pyvalue_err(
            "Invalid Alpaca option updated_since; use RFC 3339 or YYYY-MM-DD",
        ));
    }
    Ok(())
}

/// Async read-only Python client for bounded Alpaca historical market data.
#[derive(Clone, Debug)]
#[pyclass(module = "nautilus_trader.adapters.alpaca", skip_from_py_object)]
#[pyo3_stub_gen::derive::gen_stub_pyclass(module = "nautilus_trader.adapters.alpaca")]
pub struct AlpacaHistoricalDataClient {
    inner: AlpacaRawHttpClient,
}

#[pymethods]
#[pyo3_stub_gen::derive::gen_stub_pymethods]
impl AlpacaHistoricalDataClient {
    #[new]
    #[pyo3(signature = (api_key=None, api_secret=None, base_url=None, timeout_secs=10, max_retries=3, proxy_url=None))]
    fn py_new(
        api_key: Option<String>,
        api_secret: Option<String>,
        base_url: Option<String>,
        timeout_secs: u64,
        max_retries: u32,
        proxy_url: Option<String>,
    ) -> PyResult<Self> {
        Ok(Self {
            inner: build_historical_client(
                api_key,
                api_secret,
                base_url,
                timeout_secs,
                max_retries,
                proxy_url,
            )?,
        })
    }

    /// Retrieve the latest market snapshot for one equity symbol.
    #[pyo3(name = "get_snapshot", signature = (symbol, feed=AlpacaDataFeed::Iex))]
    #[gen_stub(override_return_type(
        type_repr = "typing.Awaitable[dict[str, object]]",
        imports = ("typing",)
    ))]
    fn py_get_snapshot<'py>(
        &self,
        py: Python<'py>,
        symbol: String,
        feed: AlpacaDataFeed,
    ) -> PyResult<Bound<'py, PyAny>> {
        validate_equity_symbol(&symbol)?;
        let client = self.inner.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let snapshot = client
                .get_stock_snapshot(&symbol, feed)
                .await
                .map_err(to_pyvalue_err)?;
            Python::attach(|py| {
                value_to_pyobject(py, &serde_json::to_value(snapshot).map_err(to_pyvalue_err)?)
            })
        })
    }

    /// Retrieve latest consolidated market state for multiple symbols in one request.
    #[pyo3(name = "get_snapshots", signature = (symbols, feed=AlpacaDataFeed::Iex))]
    #[gen_stub(override_return_type(
        type_repr = "typing.Awaitable[dict[str, dict[str, object]]]",
        imports = ("typing",)
    ))]
    fn py_get_snapshots<'py>(
        &self,
        py: Python<'py>,
        symbols: Vec<String>,
        feed: AlpacaDataFeed,
    ) -> PyResult<Bound<'py, PyAny>> {
        let symbols = join_unique_symbols(symbols, "snapshots")?;
        let client = self.inner.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let snapshots = client
                .get_stock_snapshots(&AlpacaSnapshotsQuery {
                    symbols: &symbols,
                    feed,
                })
                .await
                .map_err(to_pyvalue_err)?;
            Python::attach(|py| {
                value_to_pyobject(
                    py,
                    &serde_json::to_value(snapshots).map_err(to_pyvalue_err)?,
                )
            })
        })
    }

    /// Retrieve a bounded, automatically paginated option chain with exact Greeks and prices.
    #[pyo3(name = "get_option_chain", signature = (underlying_symbol, feed="indicative".to_string(), option_type=None, strike_price_gte=None, strike_price_lte=None, expiration_date=None, expiration_date_gte=None, expiration_date_lte=None, root_symbol=None, updated_since=None, max_items=1_000))]
    #[gen_stub(override_return_type(
        type_repr = "typing.Awaitable[dict[str, dict[str, object]]]",
        imports = ("typing",)
    ))]
    #[expect(clippy::too_many_arguments)]
    fn py_get_option_chain<'py>(
        &self,
        py: Python<'py>,
        underlying_symbol: String,
        feed: String,
        option_type: Option<String>,
        strike_price_gte: Option<String>,
        strike_price_lte: Option<String>,
        expiration_date: Option<String>,
        expiration_date_gte: Option<String>,
        expiration_date_lte: Option<String>,
        root_symbol: Option<String>,
        updated_since: Option<String>,
        max_items: usize,
    ) -> PyResult<Bound<'py, PyAny>> {
        validate_equity_symbol(&underlying_symbol)?;
        if !matches!(feed.as_str(), "indicative" | "opra") {
            return Err(to_pyvalue_err(
                "Alpaca option feed must be 'indicative' or 'opra'",
            ));
        }
        validate_option_type(option_type.as_deref())?;
        validate_option_strikes(strike_price_gte.as_deref(), strike_price_lte.as_deref())?;
        validate_option_date(expiration_date.as_deref(), "expiration_date")?;
        validate_option_date(expiration_date_gte.as_deref(), "expiration_date_gte")?;
        validate_option_date(expiration_date_lte.as_deref(), "expiration_date_lte")?;
        if max_items == 0 || max_items > 100_000 {
            return Err(to_pyvalue_err(
                "Alpaca option-chain max_items must be between 1 and 100000",
            ));
        }
        validate_option_updated_since(updated_since.as_deref())?;
        if let Some(value) = root_symbol.as_deref() {
            validate_equity_symbol(value)?;
        }
        let client = self.inner.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let mut values = BTreeMap::new();
            let mut cursor = None::<String>;
            while values.len() < max_items {
                let query = AlpacaOptionChainQuery {
                    feed: &feed,
                    limit: u16::try_from((max_items - values.len()).min(1_000))
                        .expect("bounded page size"),
                    updated_since: updated_since.as_deref(),
                    page_token: cursor.as_deref(),
                    option_type: option_type.as_deref(),
                    strike_price_gte: strike_price_gte.as_deref(),
                    strike_price_lte: strike_price_lte.as_deref(),
                    expiration_date: expiration_date.as_deref(),
                    expiration_date_gte: expiration_date_gte.as_deref(),
                    expiration_date_lte: expiration_date_lte.as_deref(),
                    root_symbol: root_symbol.as_deref(),
                };
                let response = client
                    .get_option_chain(&underlying_symbol, &query)
                    .await
                    .map_err(to_pyvalue_err)?;
                values.extend(response.snapshots);
                if response.next_page_token == cursor && cursor.is_some() {
                    return Err(to_pyvalue_err(
                        "Alpaca option-chain pagination did not advance",
                    ));
                }
                cursor = response.next_page_token;
                if cursor.is_none() {
                    break;
                }
            }
            let values = values
                .into_iter()
                .take(max_items)
                .collect::<BTreeMap<_, _>>();
            Python::attach(|py| {
                value_to_pyobject(py, &serde_json::to_value(values).map_err(to_pyvalue_err)?)
            })
        })
    }

    /// Retrieve latest snapshots for an explicit option-contract selection.
    #[pyo3(name = "get_option_snapshots", signature = (symbols, feed="indicative".to_string(), updated_since=None, max_items=100))]
    #[gen_stub(override_return_type(
        type_repr = "typing.Awaitable[dict[str, dict[str, object]]]",
        imports = ("typing",)
    ))]
    fn py_get_option_snapshots<'py>(
        &self,
        py: Python<'py>,
        symbols: Vec<String>,
        feed: String,
        updated_since: Option<String>,
        max_items: usize,
    ) -> PyResult<Bound<'py, PyAny>> {
        let symbols = join_option_symbols(symbols, "option snapshots")?;
        if !matches!(feed.as_str(), "indicative" | "opra") {
            return Err(to_pyvalue_err(
                "Alpaca option feed must be 'indicative' or 'opra'",
            ));
        }
        validate_option_updated_since(updated_since.as_deref())?;
        if max_items == 0 || max_items > 100 {
            return Err(to_pyvalue_err(
                "Alpaca option-snapshot max_items must be between 1 and 100",
            ));
        }
        let client = self.inner.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let mut values = BTreeMap::new();
            let mut cursor = None::<String>;
            while values.len() < max_items {
                let query = AlpacaOptionSnapshotsQuery {
                    symbols: &symbols,
                    feed: &feed,
                    limit: u16::try_from((max_items - values.len()).min(1_000))
                        .expect("bounded page size"),
                    updated_since: updated_since.as_deref(),
                    page_token: cursor.as_deref(),
                };
                let response = client
                    .get_option_snapshots(&query)
                    .await
                    .map_err(to_pyvalue_err)?;
                values.extend(response.snapshots);
                if response.next_page_token == cursor && cursor.is_some() {
                    return Err(to_pyvalue_err(
                        "Alpaca option-snapshot pagination did not advance",
                    ));
                }
                cursor = response.next_page_token;
                if cursor.is_none() {
                    break;
                }
            }
            let values = values
                .into_iter()
                .take(max_items)
                .collect::<BTreeMap<_, _>>();
            Python::attach(|py| {
                value_to_pyobject(py, &serde_json::to_value(values).map_err(to_pyvalue_err)?)
            })
        })
    }

    /// Retrieve grouped historical bars for up to 100 option contract symbols.
    #[pyo3(name = "get_option_bars", signature = (symbols, timeframe, start, end, max_items=100_000))]
    #[gen_stub(override_return_type(
        type_repr = "typing.Awaitable[dict[str, list[dict[str, object]]]]",
        imports = ("typing",)
    ))]
    fn py_get_option_bars<'py>(
        &self,
        py: Python<'py>,
        symbols: Vec<String>,
        timeframe: String,
        start: &str,
        end: &str,
        max_items: usize,
    ) -> PyResult<Bound<'py, PyAny>> {
        let symbols = join_option_symbols(symbols, "historical option bars")?;
        validate_historical_timeframe(&timeframe)?;
        validate_option_history_limit(max_items)?;
        let (start, end) = historical_range(start, end)?;
        let client = self.inner.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let mut values = symbols
                .split(',')
                .map(|symbol| (symbol.to_string(), Vec::new()))
                .collect::<BTreeMap<_, _>>();
            let mut count = 0_usize;
            let mut cursor = None::<String>;
            while count < max_items {
                let query = AlpacaOptionBarsQuery {
                    symbols: &symbols,
                    timeframe: &timeframe,
                    start,
                    end,
                    limit: u16::try_from((max_items - count).min(1_000))
                        .expect("bounded page size"),
                    sort: "asc",
                    page_token: cursor.as_deref(),
                };
                let response = client
                    .get_option_bars(&query)
                    .await
                    .map_err(to_pyvalue_err)?;
                for (symbol, bars) in response.bars {
                    count += bars.len();
                    values.entry(symbol).or_default().extend(bars);
                }
                if response.next_page_token == cursor && cursor.is_some() {
                    return Err(to_pyvalue_err(
                        "Alpaca historical option-bar pagination did not advance",
                    ));
                }
                cursor = response.next_page_token;
                if cursor.is_none() {
                    break;
                }
            }
            truncate_grouped(&mut values, max_items);
            Python::attach(|py| {
                value_to_pyobject(py, &serde_json::to_value(values).map_err(to_pyvalue_err)?)
            })
        })
    }

    /// Retrieve grouped historical trades for up to 100 option contract symbols.
    #[pyo3(name = "get_option_trades", signature = (symbols, start, end, max_items=100_000))]
    #[gen_stub(override_return_type(
        type_repr = "typing.Awaitable[dict[str, list[dict[str, object]]]]",
        imports = ("typing",)
    ))]
    fn py_get_option_trades<'py>(
        &self,
        py: Python<'py>,
        symbols: Vec<String>,
        start: &str,
        end: &str,
        max_items: usize,
    ) -> PyResult<Bound<'py, PyAny>> {
        let symbols = join_option_symbols(symbols, "historical option trades")?;
        validate_option_history_limit(max_items)?;
        let (start, end) = historical_range(start, end)?;
        let client = self.inner.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let mut values = symbols
                .split(',')
                .map(|symbol| (symbol.to_string(), Vec::new()))
                .collect::<BTreeMap<_, _>>();
            let mut count = 0_usize;
            let mut cursor = None::<String>;
            while count < max_items {
                let query = AlpacaOptionTradesQuery {
                    symbols: &symbols,
                    start,
                    end,
                    limit: u16::try_from((max_items - count).min(1_000))
                        .expect("bounded page size"),
                    sort: "asc",
                    page_token: cursor.as_deref(),
                };
                let response = client
                    .get_option_trades(&query)
                    .await
                    .map_err(to_pyvalue_err)?;
                for (symbol, trades) in response.trades {
                    count += trades.len();
                    values.entry(symbol).or_default().extend(trades);
                }
                if response.next_page_token == cursor && cursor.is_some() {
                    return Err(to_pyvalue_err(
                        "Alpaca historical option-trade pagination did not advance",
                    ));
                }
                cursor = response.next_page_token;
                if cursor.is_none() {
                    break;
                }
            }
            truncate_grouped(&mut values, max_items);
            Python::attach(|py| {
                value_to_pyobject(py, &serde_json::to_value(values).map_err(to_pyvalue_err)?)
            })
        })
    }

    /// Retrieve the current most-active US equities by volume or trade count.
    #[pyo3(name = "get_most_actives", signature = (by="volume", top=10))]
    #[gen_stub(override_return_type(
        type_repr = "typing.Awaitable[dict[str, object]]",
        imports = ("typing",)
    ))]
    fn py_get_most_actives<'py>(
        &self,
        py: Python<'py>,
        by: &str,
        top: u8,
    ) -> PyResult<Bound<'py, PyAny>> {
        if !matches!(by, "volume" | "trades") {
            return Err(to_pyvalue_err(
                "Alpaca most-actives ranking must be 'volume' or 'trades'",
            ));
        }
        if !(1..=100).contains(&top) {
            return Err(to_pyvalue_err(
                "Alpaca most-actives top must be between 1 and 100",
            ));
        }
        let client = self.inner.clone();
        let by = by.to_string();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let response = client
                .get_most_actives(&AlpacaMostActivesQuery { by: &by, top })
                .await
                .map_err(to_pyvalue_err)?;
            Python::attach(|py| {
                value_to_pyobject(py, &serde_json::to_value(response).map_err(to_pyvalue_err)?)
            })
        })
    }

    /// Retrieve the current top US equity gainers and losers.
    #[pyo3(name = "get_market_movers", signature = (top=10))]
    #[gen_stub(override_return_type(
        type_repr = "typing.Awaitable[dict[str, object]]",
        imports = ("typing",)
    ))]
    fn py_get_market_movers<'py>(&self, py: Python<'py>, top: u8) -> PyResult<Bound<'py, PyAny>> {
        if !(1..=50).contains(&top) {
            return Err(to_pyvalue_err(
                "Alpaca market-movers top must be between 1 and 50",
            ));
        }
        let client = self.inner.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let response = client
                .get_stock_movers(&AlpacaMarketMoversQuery { top })
                .await
                .map_err(to_pyvalue_err)?;
            Python::attach(|py| {
                value_to_pyobject(py, &serde_json::to_value(response).map_err(to_pyvalue_err)?)
            })
        })
    }

    /// Retrieve stock exchange code descriptions.
    #[pyo3(name = "get_exchange_codes")]
    #[gen_stub(override_return_type(
        type_repr = "typing.Awaitable[dict[str, str]]",
        imports = ("typing",)
    ))]
    fn py_get_exchange_codes<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let client = self.inner.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let exchanges = client.get_stock_exchanges().await.map_err(to_pyvalue_err)?;
            Python::attach(|py| {
                value_to_pyobject(
                    py,
                    &serde_json::to_value(exchanges).map_err(to_pyvalue_err)?,
                )
            })
        })
    }

    /// Retrieve trade or quote condition descriptions for one SIP tape.
    #[pyo3(name = "get_condition_codes")]
    #[gen_stub(override_return_type(
        type_repr = "typing.Awaitable[dict[str, str]]",
        imports = ("typing",)
    ))]
    fn py_get_condition_codes<'py>(
        &self,
        py: Python<'py>,
        tick_type: &str,
        tape: &str,
    ) -> PyResult<Bound<'py, PyAny>> {
        if !matches!(tick_type, "trade" | "quote") {
            return Err(to_pyvalue_err(
                "Alpaca condition tick_type must be 'trade' or 'quote'",
            ));
        }
        if !matches!(tape, "A" | "B" | "C") {
            return Err(to_pyvalue_err(
                "Alpaca condition tape must be 'A', 'B', or 'C'",
            ));
        }
        let client = self.inner.clone();
        let tick_type = tick_type.to_string();
        let tape = tape.to_string();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let conditions = client
                .get_stock_conditions(&tick_type, &AlpacaConditionsQuery { tape: &tape })
                .await
                .map_err(to_pyvalue_err)?;
            Python::attach(|py| {
                value_to_pyobject(
                    py,
                    &serde_json::to_value(conditions).map_err(to_pyvalue_err)?,
                )
            })
        })
    }

    /// Retrieve bars with automatic cursor pagination and exact decimal strings.
    #[pyo3(name = "get_bars", signature = (symbol, timeframe, start, end, feed=AlpacaDataFeed::Iex, adjustment=AlpacaBarAdjustment::Raw, max_items=10_000, asof=None))]
    #[gen_stub(override_return_type(
        type_repr = "typing.Awaitable[list[dict[str, object]]]",
        imports = ("typing",)
    ))]
    #[expect(clippy::too_many_arguments)]
    fn py_get_bars<'py>(
        &self,
        py: Python<'py>,
        symbol: String,
        timeframe: String,
        start: &str,
        end: &str,
        feed: AlpacaDataFeed,
        adjustment: AlpacaBarAdjustment,
        max_items: usize,
        asof: Option<String>,
    ) -> PyResult<Bound<'py, PyAny>> {
        validate_historical_request(&symbol, feed, max_items)?;
        validate_historical_timeframe(&timeframe)?;
        validate_asof(asof.as_deref())?;
        let (start, end) = historical_range(start, end)?;
        let client = self.inner.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let mut values = Vec::with_capacity(max_items.min(10_000));
            let mut cursor = None::<String>;
            while values.len() < max_items {
                let mut query =
                    AlpacaBarsQuery::new(&symbol, &timeframe, start, end, feed, adjustment);
                query.limit = Some(
                    u32::try_from((max_items - values.len()).min(10_000))
                        .expect("bounded page size"),
                );
                query.asof = asof.as_deref();
                query.page_token = cursor.as_deref();
                let response = client
                    .get_stock_bars(&query)
                    .await
                    .map_err(to_pyvalue_err)?;
                values.extend(response.bars.get(&symbol).into_iter().flatten().cloned());
                if response.next_page_token == cursor && cursor.is_some() {
                    return Err(to_pyvalue_err(
                        "Alpaca historical bar pagination did not advance",
                    ));
                }
                cursor = response.next_page_token;
                if cursor.is_none() {
                    break;
                }
            }
            values.truncate(max_items);
            Python::attach(|py| {
                value_to_pyobject(py, &serde_json::to_value(values).map_err(to_pyvalue_err)?)
            })
        })
    }

    /// Retrieve trades with automatic cursor pagination and exact decimal strings.
    #[pyo3(name = "get_trades", signature = (symbol, start, end, feed=AlpacaDataFeed::Iex, max_items=10_000, asof=None))]
    #[gen_stub(override_return_type(
        type_repr = "typing.Awaitable[list[dict[str, object]]]",
        imports = ("typing",)
    ))]
    #[expect(clippy::too_many_arguments)]
    fn py_get_trades<'py>(
        &self,
        py: Python<'py>,
        symbol: String,
        start: &str,
        end: &str,
        feed: AlpacaDataFeed,
        max_items: usize,
        asof: Option<String>,
    ) -> PyResult<Bound<'py, PyAny>> {
        validate_historical_request(&symbol, feed, max_items)?;
        validate_asof(asof.as_deref())?;
        let (start, end) = historical_range(start, end)?;
        let client = self.inner.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let mut values = Vec::with_capacity(max_items.min(10_000));
            let mut cursor = None::<String>;
            while values.len() < max_items {
                let mut query = AlpacaTicksQuery::new(&symbol, start, end, feed);
                query.limit = u32::try_from((max_items - values.len()).min(10_000))
                    .expect("bounded page size");
                query.asof = asof.as_deref();
                query.page_token = cursor.as_deref();
                let response = client
                    .get_stock_trades(&query)
                    .await
                    .map_err(to_pyvalue_err)?;
                values.extend(response.trades.get(&symbol).into_iter().flatten().cloned());
                if response.next_page_token == cursor && cursor.is_some() {
                    return Err(to_pyvalue_err(
                        "Alpaca historical trade pagination did not advance",
                    ));
                }
                cursor = response.next_page_token;
                if cursor.is_none() {
                    break;
                }
            }
            values.truncate(max_items);
            Python::attach(|py| {
                value_to_pyobject(py, &serde_json::to_value(values).map_err(to_pyvalue_err)?)
            })
        })
    }

    /// Retrieve quotes with automatic cursor pagination and exact decimal strings.
    #[pyo3(name = "get_quotes", signature = (symbol, start, end, feed=AlpacaDataFeed::Iex, max_items=10_000, asof=None))]
    #[gen_stub(override_return_type(
        type_repr = "typing.Awaitable[list[dict[str, object]]]",
        imports = ("typing",)
    ))]
    #[expect(clippy::too_many_arguments)]
    fn py_get_quotes<'py>(
        &self,
        py: Python<'py>,
        symbol: String,
        start: &str,
        end: &str,
        feed: AlpacaDataFeed,
        max_items: usize,
        asof: Option<String>,
    ) -> PyResult<Bound<'py, PyAny>> {
        validate_historical_request(&symbol, feed, max_items)?;
        validate_asof(asof.as_deref())?;
        let (start, end) = historical_range(start, end)?;
        let client = self.inner.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let mut values = Vec::with_capacity(max_items.min(10_000));
            let mut cursor = None::<String>;
            while values.len() < max_items {
                let mut query = AlpacaTicksQuery::new(&symbol, start, end, feed);
                query.limit = u32::try_from((max_items - values.len()).min(10_000))
                    .expect("bounded page size");
                query.asof = asof.as_deref();
                query.page_token = cursor.as_deref();
                let response = client
                    .get_stock_quotes(&query)
                    .await
                    .map_err(to_pyvalue_err)?;
                values.extend(response.quotes.get(&symbol).into_iter().flatten().cloned());
                if response.next_page_token == cursor && cursor.is_some() {
                    return Err(to_pyvalue_err(
                        "Alpaca historical quote pagination did not advance",
                    ));
                }
                cursor = response.next_page_token;
                if cursor.is_none() {
                    break;
                }
            }
            values.truncate(max_items);
            Python::attach(|py| {
                value_to_pyobject(py, &serde_json::to_value(values).map_err(to_pyvalue_err)?)
            })
        })
    }

    /// Retrieve grouped bars for multiple symbols with automatic cursor pagination.
    #[pyo3(name = "get_bars_multi", signature = (symbols, timeframe, start, end, feed=AlpacaDataFeed::Iex, adjustment=AlpacaBarAdjustment::Raw, max_items=100_000, asof=None))]
    #[gen_stub(override_return_type(
        type_repr = "typing.Awaitable[dict[str, list[dict[str, object]]]]",
        imports = ("typing",)
    ))]
    #[expect(clippy::too_many_arguments)]
    fn py_get_bars_multi<'py>(
        &self,
        py: Python<'py>,
        symbols: Vec<String>,
        timeframe: String,
        start: &str,
        end: &str,
        feed: AlpacaDataFeed,
        adjustment: AlpacaBarAdjustment,
        max_items: usize,
        asof: Option<String>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let symbols = join_unique_symbols(symbols, "historical bars")?;
        validate_historical_request(symbols.split(',').next().unwrap(), feed, max_items)?;
        validate_historical_timeframe(&timeframe)?;
        validate_asof(asof.as_deref())?;
        let (start, end) = historical_range(start, end)?;
        let client = self.inner.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let mut values = symbols
                .split(',')
                .map(|symbol| (symbol.to_string(), Vec::new()))
                .collect::<BTreeMap<_, _>>();
            let mut count = 0_usize;
            let mut cursor = None::<String>;
            while count < max_items {
                let mut query =
                    AlpacaBarsQuery::new(&symbols, &timeframe, start, end, feed, adjustment);
                query.limit = Some(
                    u32::try_from((max_items - count).min(10_000)).expect("bounded page size"),
                );
                query.asof = asof.as_deref();
                query.page_token = cursor.as_deref();
                let response = client
                    .get_stock_bars(&query)
                    .await
                    .map_err(to_pyvalue_err)?;
                for (symbol, bars) in response.bars {
                    count += bars.len();
                    values.entry(symbol).or_default().extend(bars);
                }
                if response.next_page_token == cursor && cursor.is_some() {
                    return Err(to_pyvalue_err(
                        "Alpaca historical bar pagination did not advance",
                    ));
                }
                cursor = response.next_page_token;
                if cursor.is_none() {
                    break;
                }
            }
            truncate_grouped(&mut values, max_items);
            Python::attach(|py| {
                value_to_pyobject(py, &serde_json::to_value(values).map_err(to_pyvalue_err)?)
            })
        })
    }

    /// Retrieve grouped trades for multiple symbols with automatic cursor pagination.
    #[pyo3(name = "get_trades_multi", signature = (symbols, start, end, feed=AlpacaDataFeed::Iex, max_items=100_000, asof=None))]
    #[gen_stub(override_return_type(
        type_repr = "typing.Awaitable[dict[str, list[dict[str, object]]]]",
        imports = ("typing",)
    ))]
    #[expect(clippy::too_many_arguments)]
    fn py_get_trades_multi<'py>(
        &self,
        py: Python<'py>,
        symbols: Vec<String>,
        start: &str,
        end: &str,
        feed: AlpacaDataFeed,
        max_items: usize,
        asof: Option<String>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let symbols = join_unique_symbols(symbols, "historical trades")?;
        validate_historical_request(symbols.split(',').next().unwrap(), feed, max_items)?;
        validate_asof(asof.as_deref())?;
        let (start, end) = historical_range(start, end)?;
        let client = self.inner.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let mut values = symbols
                .split(',')
                .map(|symbol| (symbol.to_string(), Vec::new()))
                .collect::<BTreeMap<_, _>>();
            let mut count = 0_usize;
            let mut cursor = None::<String>;
            while count < max_items {
                let mut query = AlpacaTicksQuery::new(&symbols, start, end, feed);
                query.limit =
                    u32::try_from((max_items - count).min(10_000)).expect("bounded page size");
                query.asof = asof.as_deref();
                query.page_token = cursor.as_deref();
                let response = client
                    .get_stock_trades(&query)
                    .await
                    .map_err(to_pyvalue_err)?;
                for (symbol, trades) in response.trades {
                    count += trades.len();
                    values.entry(symbol).or_default().extend(trades);
                }
                if response.next_page_token == cursor && cursor.is_some() {
                    return Err(to_pyvalue_err(
                        "Alpaca historical trade pagination did not advance",
                    ));
                }
                cursor = response.next_page_token;
                if cursor.is_none() {
                    break;
                }
            }
            truncate_grouped(&mut values, max_items);
            Python::attach(|py| {
                value_to_pyobject(py, &serde_json::to_value(values).map_err(to_pyvalue_err)?)
            })
        })
    }

    /// Retrieve grouped quotes for multiple symbols with automatic cursor pagination.
    #[pyo3(name = "get_quotes_multi", signature = (symbols, start, end, feed=AlpacaDataFeed::Iex, max_items=100_000, asof=None))]
    #[gen_stub(override_return_type(
        type_repr = "typing.Awaitable[dict[str, list[dict[str, object]]]]",
        imports = ("typing",)
    ))]
    #[expect(clippy::too_many_arguments)]
    fn py_get_quotes_multi<'py>(
        &self,
        py: Python<'py>,
        symbols: Vec<String>,
        start: &str,
        end: &str,
        feed: AlpacaDataFeed,
        max_items: usize,
        asof: Option<String>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let symbols = join_unique_symbols(symbols, "historical quotes")?;
        validate_historical_request(symbols.split(',').next().unwrap(), feed, max_items)?;
        validate_asof(asof.as_deref())?;
        let (start, end) = historical_range(start, end)?;
        let client = self.inner.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let mut values = symbols
                .split(',')
                .map(|symbol| (symbol.to_string(), Vec::new()))
                .collect::<BTreeMap<_, _>>();
            let mut count = 0_usize;
            let mut cursor = None::<String>;
            while count < max_items {
                let mut query = AlpacaTicksQuery::new(&symbols, start, end, feed);
                query.limit =
                    u32::try_from((max_items - count).min(10_000)).expect("bounded page size");
                query.asof = asof.as_deref();
                query.page_token = cursor.as_deref();
                let response = client
                    .get_stock_quotes(&query)
                    .await
                    .map_err(to_pyvalue_err)?;
                for (symbol, quotes) in response.quotes {
                    count += quotes.len();
                    values.entry(symbol).or_default().extend(quotes);
                }
                if response.next_page_token == cursor && cursor.is_some() {
                    return Err(to_pyvalue_err(
                        "Alpaca historical quote pagination did not advance",
                    ));
                }
                cursor = response.next_page_token;
                if cursor.is_none() {
                    break;
                }
            }
            truncate_grouped(&mut values, max_items);
            Python::attach(|py| {
                value_to_pyobject(py, &serde_json::to_value(values).map_err(to_pyvalue_err)?)
            })
        })
    }

    /// Retrieve grouped opening and closing auction prints for multiple symbols.
    #[pyo3(name = "get_auctions", signature = (symbols, start, end, max_items=100_000, asof=None))]
    #[gen_stub(override_return_type(
        type_repr = "typing.Awaitable[dict[str, list[dict[str, object]]]]",
        imports = ("typing",)
    ))]
    fn py_get_auctions<'py>(
        &self,
        py: Python<'py>,
        symbols: Vec<String>,
        start: &str,
        end: &str,
        max_items: usize,
        asof: Option<String>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let symbols = join_unique_symbols(symbols, "historical auctions")?;
        validate_historical_request(
            symbols.split(',').next().unwrap(),
            AlpacaDataFeed::Sip,
            max_items,
        )?;
        validate_asof(asof.as_deref())?;
        let (start, end) = historical_range(start, end)?;
        let client = self.inner.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let mut values = symbols
                .split(',')
                .map(|symbol| (symbol.to_string(), Vec::new()))
                .collect::<BTreeMap<_, _>>();
            let mut count = 0_usize;
            let mut cursor = None::<String>;
            while count < max_items {
                let mut query = AlpacaTicksQuery::new(&symbols, start, end, AlpacaDataFeed::Sip);
                query.limit =
                    u32::try_from((max_items - count).min(10_000)).expect("bounded page size");
                query.asof = asof.as_deref();
                query.page_token = cursor.as_deref();
                let response = client
                    .get_stock_auctions(&query)
                    .await
                    .map_err(to_pyvalue_err)?;
                for (symbol, auctions) in response.auctions {
                    count += auctions.len();
                    values.entry(symbol).or_default().extend(auctions);
                }
                if response.next_page_token == cursor && cursor.is_some() {
                    return Err(to_pyvalue_err(
                        "Alpaca historical auction pagination did not advance",
                    ));
                }
                cursor = response.next_page_token;
                if cursor.is_none() {
                    break;
                }
            }
            truncate_grouped(&mut values, max_items);
            Python::attach(|py| {
                value_to_pyobject(py, &serde_json::to_value(values).map_err(to_pyvalue_err)?)
            })
        })
    }

    /// Retrieve a bounded historical news range with automatic cursor pagination.
    #[pyo3(name = "get_news", signature = (symbols=None, start=None, end=None, sort="desc", include_content=false, exclude_contentless=false, max_items=1_000))]
    #[gen_stub(override_return_type(
        type_repr = "typing.Awaitable[list[dict[str, object]]]",
        imports = ("typing",)
    ))]
    #[expect(clippy::too_many_arguments)]
    fn py_get_news<'py>(
        &self,
        py: Python<'py>,
        symbols: Option<String>,
        start: Option<String>,
        end: Option<String>,
        sort: &str,
        include_content: bool,
        exclude_contentless: bool,
        max_items: usize,
    ) -> PyResult<Bound<'py, PyAny>> {
        if !matches!(sort, "asc" | "desc") {
            return Err(to_pyvalue_err("Alpaca news sort must be 'asc' or 'desc'"));
        }
        if max_items == 0 || max_items > 100_000 {
            return Err(to_pyvalue_err(
                "Alpaca news max_items must be between 1 and 100000",
            ));
        }
        if symbols.as_deref().is_some_and(|value| {
            value.is_empty()
                || value
                    .split(',')
                    .any(|symbol| symbol.is_empty() || symbol.trim() != symbol)
        }) {
            return Err(to_pyvalue_err(
                "Alpaca news symbols must be a comma-separated list without empty values or whitespace",
            ));
        }
        let start = start
            .map(|value| value.parse::<jiff::Timestamp>())
            .transpose()
            .map_err(|error| to_pyvalue_err(format!("Invalid Alpaca news start: {error}")))?;
        let end = end
            .map(|value| value.parse::<jiff::Timestamp>())
            .transpose()
            .map_err(|error| to_pyvalue_err(format!("Invalid Alpaca news end: {error}")))?;
        if start.zip(end).is_some_and(|(start, end)| start >= end) {
            return Err(to_pyvalue_err("Alpaca news start must be earlier than end"));
        }
        let client = self.inner.clone();
        let sort = sort.to_string();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let mut articles = Vec::with_capacity(max_items.min(50));
            let mut cursor = None::<String>;
            while articles.len() < max_items {
                let query = AlpacaNewsQuery {
                    start,
                    end,
                    sort: &sort,
                    symbols: symbols.as_deref(),
                    limit: u8::try_from((max_items - articles.len()).min(50))
                        .expect("bounded page size"),
                    include_content,
                    exclude_contentless,
                    page_token: cursor.as_deref(),
                };
                let response = client.get_news(&query).await.map_err(to_pyvalue_err)?;
                if response.next_page_token == cursor && cursor.is_some() {
                    return Err(to_pyvalue_err("Alpaca news pagination did not advance"));
                }
                articles.extend(response.news);
                cursor = response.next_page_token;
                if cursor.is_none() {
                    break;
                }
            }
            articles.truncate(max_items);
            Python::attach(|py| {
                value_to_pyobject(py, &serde_json::to_value(articles).map_err(to_pyvalue_err)?)
            })
        })
    }

    /// Retrieve normalized corporate actions with automatic cursor pagination.
    #[pyo3(name = "get_corporate_actions", signature = (symbols=None, cusips=None, action_types=None, start=None, end=None, ids=None, region=None, data_quality=None, sort="asc", max_items=10_000))]
    #[gen_stub(override_return_type(
        type_repr = "typing.Awaitable[list[dict[str, object]]]",
        imports = ("typing",)
    ))]
    #[expect(clippy::too_many_arguments)]
    fn py_get_corporate_actions<'py>(
        &self,
        py: Python<'py>,
        symbols: Option<String>,
        cusips: Option<String>,
        action_types: Option<String>,
        start: Option<String>,
        end: Option<String>,
        ids: Option<String>,
        region: Option<String>,
        data_quality: Option<String>,
        sort: &str,
        max_items: usize,
    ) -> PyResult<Bound<'py, PyAny>> {
        const ACTION_TYPES: &[&str] = &[
            "reverse_split",
            "forward_split",
            "unit_split",
            "cash_dividend",
            "stock_dividend",
            "spin_off",
            "cash_merger",
            "stock_merger",
            "stock_and_cash_merger",
            "redemption",
            "name_change",
            "worthless_removal",
            "rights_distribution",
            "partial_call",
            "reorganization",
            "capital_gains_distribution",
        ];
        validate_csv(symbols.as_deref(), "corporate-action symbols")?;
        validate_csv(cusips.as_deref(), "corporate-action CUSIPs")?;
        validate_csv(action_types.as_deref(), "corporate-action types")?;
        validate_csv(ids.as_deref(), "corporate-action IDs")?;
        if action_types.as_deref().is_some_and(|types| {
            types
                .split(',')
                .any(|action_type| !ACTION_TYPES.contains(&action_type))
        }) {
            return Err(to_pyvalue_err("Unsupported Alpaca corporate-action type"));
        }
        if !matches!(sort, "asc" | "desc") {
            return Err(to_pyvalue_err(
                "Alpaca corporate-action sort must be 'asc' or 'desc'",
            ));
        }
        if region
            .as_deref()
            .is_some_and(|value| !matches!(value, "us" | "non_us" | "all"))
        {
            return Err(to_pyvalue_err(
                "Alpaca corporate-action region must be 'us', 'non_us', or 'all'",
            ));
        }
        if data_quality
            .as_deref()
            .is_some_and(|value| !matches!(value, "complete" | "all"))
        {
            return Err(to_pyvalue_err(
                "Alpaca corporate-action data_quality must be 'complete' or 'all'",
            ));
        }
        if max_items == 0 || max_items > 1_000_000 {
            return Err(to_pyvalue_err(
                "Alpaca corporate-action max_items must be between 1 and 1000000",
            ));
        }
        let start_date = start
            .as_deref()
            .map(str::parse::<jiff::civil::Date>)
            .transpose()
            .map_err(|error| {
                to_pyvalue_err(format!("Invalid Alpaca corporate-action start: {error}"))
            })?;
        let end_date = end
            .as_deref()
            .map(str::parse::<jiff::civil::Date>)
            .transpose()
            .map_err(|error| {
                to_pyvalue_err(format!("Invalid Alpaca corporate-action end: {error}"))
            })?;
        if start_date
            .zip(end_date)
            .is_some_and(|(start, end)| start > end)
        {
            return Err(to_pyvalue_err(
                "Alpaca corporate-action start must not be later than end",
            ));
        }
        if ids.is_some()
            && (symbols.is_some()
                || cusips.is_some()
                || action_types.is_some()
                || start.is_some()
                || end.is_some()
                || region.is_some()
                || data_quality.is_some())
        {
            return Err(to_pyvalue_err(
                "Alpaca corporate-action IDs cannot be combined with other filters",
            ));
        }

        let client = self.inner.clone();
        let sort = sort.to_string();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let mut actions = Vec::with_capacity(max_items.min(1_000));
            let mut cursor = None::<String>;
            while actions.len() < max_items {
                let query = AlpacaCorporateActionsQuery {
                    symbols: symbols.as_deref(),
                    cusips: cusips.as_deref(),
                    action_types: action_types.as_deref(),
                    start: start.as_deref(),
                    end: end.as_deref(),
                    ids: ids.as_deref(),
                    region: region.as_deref(),
                    data_quality: data_quality.as_deref(),
                    limit: u16::try_from((max_items - actions.len()).min(1_000))
                        .expect("bounded page size"),
                    sort: &sort,
                    page_token: cursor.as_deref(),
                };
                let response = client
                    .get_corporate_actions(&query)
                    .await
                    .map_err(to_pyvalue_err)?;
                if response.next_page_token == cursor && cursor.is_some() {
                    return Err(to_pyvalue_err(
                        "Alpaca corporate-action pagination did not advance",
                    ));
                }
                for (group, values) in response.actions {
                    let action_type = group.strip_suffix('s').unwrap_or(&group);
                    for mut value in values {
                        let object = value.as_object_mut().ok_or_else(|| {
                            to_pyvalue_err("Alpaca corporate action was not an object")
                        })?;
                        object.insert(
                            "type".to_string(),
                            serde_json::Value::String(action_type.to_string()),
                        );
                        actions.push(value);
                    }
                }
                cursor = response.next_page_token;
                if cursor.is_none() {
                    break;
                }
            }
            actions.sort_by(|left, right| {
                let left_key = left
                    .get("process_date")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default();
                let right_key = right
                    .get("process_date")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default();
                left_key.cmp(right_key)
            });
            if sort == "desc" {
                actions.reverse();
            }
            actions.truncate(max_items);
            Python::attach(|py| value_to_pyobject(py, &serde_json::Value::Array(actions)))
        })
    }
}

/// Async read-only Python client for Alpaca instrument reference data.
#[derive(Clone, Debug)]
#[pyclass(module = "nautilus_trader.adapters.alpaca", skip_from_py_object)]
#[pyo3_stub_gen::derive::gen_stub_pyclass(module = "nautilus_trader.adapters.alpaca")]
pub struct AlpacaReferenceDataClient {
    inner: AlpacaRawHttpClient,
}

#[pymethods]
#[pyo3_stub_gen::derive::gen_stub_pymethods]
impl AlpacaReferenceDataClient {
    #[new]
    #[pyo3(signature = (api_key=None, api_secret=None, environment=AlpacaEnvironment::Paper, base_url=None, timeout_secs=10, max_retries=3, proxy_url=None))]
    fn py_new(
        api_key: Option<String>,
        api_secret: Option<String>,
        environment: AlpacaEnvironment,
        base_url: Option<String>,
        timeout_secs: u64,
        max_retries: u32,
        proxy_url: Option<String>,
    ) -> PyResult<Self> {
        Ok(Self {
            inner: build_read_client(
                api_key,
                api_secret,
                environment,
                base_url,
                timeout_secs,
                max_retries,
                proxy_url,
            )?,
        })
    }

    /// Retrieve one asset by symbol or Alpaca asset ID.
    #[pyo3(name = "get_asset")]
    #[gen_stub(override_return_type(
        type_repr = "typing.Awaitable[dict[str, object]]",
        imports = ("typing",)
    ))]
    fn py_get_asset<'py>(
        &self,
        py: Python<'py>,
        symbol_or_asset_id: String,
    ) -> PyResult<Bound<'py, PyAny>> {
        if symbol_or_asset_id.trim().is_empty()
            || symbol_or_asset_id.trim() != symbol_or_asset_id
            || symbol_or_asset_id.contains('/')
        {
            return Err(to_pyvalue_err(
                "Alpaca symbol_or_asset_id must be non-empty, contain no surrounding whitespace, and not contain '/'",
            ));
        }
        let client = self.inner.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let asset = client
                .get_asset(&symbol_or_asset_id)
                .await
                .map_err(to_pyvalue_err)?;
            Python::attach(|py| {
                let value = serde_json::to_value(asset).map_err(to_pyvalue_err)?;
                value_to_pyobject(py, &value)
            })
        })
    }

    /// Retrieve US equity reference data with optional status, exchange, and attribute filters.
    #[pyo3(name = "get_assets", signature = (status="active".to_string(), exchange=None, attributes=None))]
    #[gen_stub(override_return_type(
        type_repr = "typing.Awaitable[list[dict[str, object]]]",
        imports = ("typing",)
    ))]
    fn py_get_assets<'py>(
        &self,
        py: Python<'py>,
        status: String,
        exchange: Option<String>,
        attributes: Option<Vec<String>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        if !matches!(status.as_str(), "active" | "inactive" | "all") {
            return Err(to_pyvalue_err(
                "Alpaca asset status must be 'active', 'inactive', or 'all'",
            ));
        }
        if exchange
            .as_deref()
            .is_some_and(|value| value.trim().is_empty() || value.trim() != value)
        {
            return Err(to_pyvalue_err(
                "Alpaca exchange must be non-empty and contain no surrounding whitespace",
            ));
        }
        if attributes.as_ref().is_some_and(Vec::is_empty) {
            return Err(to_pyvalue_err(
                "Alpaca asset attributes must contain at least one value when provided",
            ));
        }
        if attributes.as_ref().is_some_and(|values| {
            values
                .iter()
                .any(|value| !ALPACA_ASSET_ATTRIBUTES.contains(&value.as_str()))
        }) {
            return Err(to_pyvalue_err("Unsupported Alpaca asset attribute"));
        }
        if attributes.as_ref().is_some_and(|values| {
            let mut unique = HashSet::with_capacity(values.len());
            values.iter().any(|value| !unique.insert(value))
        }) {
            return Err(to_pyvalue_err(
                "Alpaca asset attributes must not contain duplicates",
            ));
        }
        let attributes = attributes.map(|values| values.join(","));
        let client = self.inner.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let query = AlpacaAssetsQuery {
                status: (status != "all").then_some(status.as_str()),
                asset_class: "us_equity",
                exchange: exchange.as_deref(),
                attributes: attributes.as_deref(),
            };
            let assets = client.get_assets(&query).await.map_err(to_pyvalue_err)?;
            Python::attach(|py| {
                let value = serde_json::to_value(assets).map_err(to_pyvalue_err)?;
                value_to_pyobject(py, &value)
            })
        })
    }

    /// Retrieve one option contract by OCC symbol or Alpaca contract ID.
    #[pyo3(name = "get_option_contract")]
    #[gen_stub(override_return_type(
        type_repr = "typing.Awaitable[dict[str, object]]",
        imports = ("typing",)
    ))]
    fn py_get_option_contract<'py>(
        &self,
        py: Python<'py>,
        symbol_or_contract_id: String,
    ) -> PyResult<Bound<'py, PyAny>> {
        validate_watchlist_key("option contract key", &symbol_or_contract_id, true)?;
        let client = self.inner.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let contract = client
                .get_option_contract(&symbol_or_contract_id)
                .await
                .map_err(to_pyvalue_err)?;
            Python::attach(|py| {
                value_to_pyobject(py, &serde_json::to_value(contract).map_err(to_pyvalue_err)?)
            })
        })
    }

    /// Retrieve bounded option contracts with automatic cursor pagination.
    #[pyo3(name = "get_option_contracts", signature = (underlying_symbols=None, status="active".to_string(), option_type=None, style=None, expiration_date=None, expiration_date_gte=None, expiration_date_lte=None, root_symbol=None, strike_price_gte=None, strike_price_lte=None, show_deliverables=false, ppind=None, max_items=10_000))]
    #[gen_stub(override_return_type(
        type_repr = "typing.Awaitable[list[dict[str, object]]]",
        imports = ("typing",)
    ))]
    #[expect(clippy::too_many_arguments)]
    fn py_get_option_contracts<'py>(
        &self,
        py: Python<'py>,
        underlying_symbols: Option<Vec<String>>,
        status: String,
        option_type: Option<String>,
        style: Option<String>,
        expiration_date: Option<String>,
        expiration_date_gte: Option<String>,
        expiration_date_lte: Option<String>,
        root_symbol: Option<String>,
        strike_price_gte: Option<String>,
        strike_price_lte: Option<String>,
        show_deliverables: bool,
        ppind: Option<bool>,
        max_items: usize,
    ) -> PyResult<Bound<'py, PyAny>> {
        if !matches!(status.as_str(), "active" | "inactive") {
            return Err(to_pyvalue_err(
                "Alpaca option contract status must be 'active' or 'inactive'",
            ));
        }
        validate_option_type(option_type.as_deref())?;
        if style
            .as_deref()
            .is_some_and(|value| !matches!(value, "american" | "european"))
        {
            return Err(to_pyvalue_err(
                "Alpaca option style must be 'american' or 'european'",
            ));
        }
        validate_option_date(expiration_date.as_deref(), "expiration_date")?;
        validate_option_date(expiration_date_gte.as_deref(), "expiration_date_gte")?;
        validate_option_date(expiration_date_lte.as_deref(), "expiration_date_lte")?;
        validate_option_strikes(strike_price_gte.as_deref(), strike_price_lte.as_deref())?;
        if max_items == 0 || max_items > 1_000_000 {
            return Err(to_pyvalue_err(
                "Alpaca option-contract max_items must be between 1 and 1000000",
            ));
        }
        let underlying_symbols = underlying_symbols
            .map(|symbols| join_unique_symbols(symbols, "option contracts"))
            .transpose()?;
        if let Some(value) = root_symbol.as_deref() {
            validate_equity_symbol(value)?;
        }
        let client = self.inner.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let mut values = Vec::with_capacity(max_items.min(10_000));
            let mut cursor = None::<String>;
            while values.len() < max_items {
                let query = AlpacaOptionContractsQuery {
                    underlying_symbols: underlying_symbols.as_deref(),
                    show_deliverables,
                    status: &status,
                    expiration_date: expiration_date.as_deref(),
                    expiration_date_gte: expiration_date_gte.as_deref(),
                    expiration_date_lte: expiration_date_lte.as_deref(),
                    root_symbol: root_symbol.as_deref(),
                    option_type: option_type.as_deref(),
                    style: style.as_deref(),
                    strike_price_gte: strike_price_gte.as_deref(),
                    strike_price_lte: strike_price_lte.as_deref(),
                    limit: u16::try_from((max_items - values.len()).min(10_000))
                        .expect("bounded page size"),
                    page_token: cursor.as_deref(),
                    ppind,
                };
                let response = client
                    .get_option_contracts(&query)
                    .await
                    .map_err(to_pyvalue_err)?;
                values.extend(response.option_contracts);
                if response.next_page_token == cursor && cursor.is_some() {
                    return Err(to_pyvalue_err(
                        "Alpaca option-contract pagination did not advance",
                    ));
                }
                cursor = response.next_page_token;
                if cursor.is_none() {
                    break;
                }
            }
            values.truncate(max_items);
            Python::attach(|py| {
                value_to_pyobject(py, &serde_json::to_value(values).map_err(to_pyvalue_err)?)
            })
        })
    }

    /// Retrieve all account watchlists and their resolved assets.
    #[pyo3(name = "get_watchlists")]
    #[gen_stub(override_return_type(
        type_repr = "typing.Awaitable[list[dict[str, object]]]",
        imports = ("typing",)
    ))]
    fn py_get_watchlists<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let client = self.inner.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let watchlists = client.get_watchlists().await.map_err(to_pyvalue_err)?;
            Python::attach(|py| {
                value_to_pyobject(
                    py,
                    &serde_json::to_value(watchlists).map_err(to_pyvalue_err)?,
                )
            })
        })
    }

    /// Retrieve one watchlist by Alpaca ID.
    #[pyo3(name = "get_watchlist")]
    #[gen_stub(override_return_type(
        type_repr = "typing.Awaitable[dict[str, object]]",
        imports = ("typing",)
    ))]
    fn py_get_watchlist<'py>(
        &self,
        py: Python<'py>,
        watchlist_id: String,
    ) -> PyResult<Bound<'py, PyAny>> {
        validate_watchlist_key("watchlist_id", &watchlist_id, true)?;
        let client = self.inner.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let watchlist = client
                .get_watchlist(&watchlist_id)
                .await
                .map_err(to_pyvalue_err)?;
            Python::attach(|py| {
                value_to_pyobject(
                    py,
                    &serde_json::to_value(watchlist).map_err(to_pyvalue_err)?,
                )
            })
        })
    }

    /// Retrieve one watchlist by its user-visible name.
    #[pyo3(name = "get_watchlist_by_name")]
    #[gen_stub(override_return_type(
        type_repr = "typing.Awaitable[dict[str, object]]",
        imports = ("typing",)
    ))]
    fn py_get_watchlist_by_name<'py>(
        &self,
        py: Python<'py>,
        name: String,
    ) -> PyResult<Bound<'py, PyAny>> {
        validate_watchlist_key("watchlist name", &name, false)?;
        let client = self.inner.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let watchlist = client
                .get_watchlist_by_name(&name)
                .await
                .map_err(to_pyvalue_err)?;
            Python::attach(|py| {
                value_to_pyobject(
                    py,
                    &serde_json::to_value(watchlist).map_err(to_pyvalue_err)?,
                )
            })
        })
    }
}

fn validate_watchlist_key(label: &str, value: &str, reject_slash: bool) -> PyResult<()> {
    if value.trim().is_empty() || value.trim() != value || (reject_slash && value.contains('/')) {
        return Err(to_pyvalue_err(format!(
            "Alpaca {label} must be non-empty and contain no surrounding whitespace{}",
            if reject_slash { " or '/'" } else { "" }
        )));
    }
    Ok(())
}

/// Async read-only Python client for account and portfolio snapshots.
#[derive(Clone, Debug)]
#[pyclass(module = "nautilus_trader.adapters.alpaca", skip_from_py_object)]
#[pyo3_stub_gen::derive::gen_stub_pyclass(module = "nautilus_trader.adapters.alpaca")]
pub struct AlpacaPortfolioClient {
    inner: AlpacaRawHttpClient,
}

#[pymethods]
#[pyo3_stub_gen::derive::gen_stub_pymethods]
impl AlpacaPortfolioClient {
    #[new]
    #[pyo3(signature = (api_key=None, api_secret=None, environment=AlpacaEnvironment::Paper, base_url=None, timeout_secs=10, max_retries=3, proxy_url=None))]
    fn py_new(
        api_key: Option<String>,
        api_secret: Option<String>,
        environment: AlpacaEnvironment,
        base_url: Option<String>,
        timeout_secs: u64,
        max_retries: u32,
        proxy_url: Option<String>,
    ) -> PyResult<Self> {
        Ok(Self {
            inner: build_read_client(
                api_key,
                api_secret,
                environment,
                base_url,
                timeout_secs,
                max_retries,
                proxy_url,
            )?,
        })
    }

    /// Retrieve the account snapshot with monetary values encoded as exact strings.
    #[pyo3(name = "get_account")]
    #[gen_stub(override_return_type(
        type_repr = "typing.Awaitable[dict[str, object]]",
        imports = ("typing",)
    ))]
    fn py_get_account<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let client = self.inner.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let account = client.get_account().await.map_err(to_pyvalue_err)?;
            Python::attach(|py| {
                let value = serde_json::to_value(account).map_err(to_pyvalue_err)?;
                value_to_pyobject(py, &value)
            })
        })
    }

    /// Retrieve current positions with monetary values encoded as exact strings.
    #[pyo3(name = "get_positions")]
    #[gen_stub(override_return_type(
        type_repr = "typing.Awaitable[list[dict[str, object]]]",
        imports = ("typing",)
    ))]
    fn py_get_positions<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let client = self.inner.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let positions = client.get_positions().await.map_err(to_pyvalue_err)?;
            Python::attach(|py| {
                let value = serde_json::to_value(positions).map_err(to_pyvalue_err)?;
                value_to_pyobject(py, &value)
            })
        })
    }

    /// Retrieve account equity and P&L history for charting and performance analysis.
    #[pyo3(name = "get_portfolio_history", signature = (period=None, timeframe=None, start=None, end=None, intraday_reporting="market_hours".to_string(), cashflow_types=None))]
    #[gen_stub(override_return_type(
        type_repr = "typing.Awaitable[dict[str, object]]",
        imports = ("typing",)
    ))]
    #[expect(clippy::too_many_arguments)]
    fn py_get_portfolio_history<'py>(
        &self,
        py: Python<'py>,
        period: Option<String>,
        timeframe: Option<String>,
        start: Option<String>,
        end: Option<String>,
        intraday_reporting: String,
        cashflow_types: Option<String>,
    ) -> PyResult<Bound<'py, PyAny>> {
        if period.as_deref().is_some_and(|value| {
            let Some((unit_index, unit)) = value.char_indices().last() else {
                return true;
            };
            !matches!(unit, 'D' | 'W' | 'M' | 'A')
                || value[..unit_index]
                    .parse::<u32>()
                    .map_or(true, |number| number == 0)
        }) {
            return Err(to_pyvalue_err(
                "Alpaca portfolio period must be a positive number followed by D, W, M, or A",
            ));
        }
        if timeframe
            .as_deref()
            .is_some_and(|value| !matches!(value, "1Min" | "5Min" | "15Min" | "1H" | "1D"))
        {
            return Err(to_pyvalue_err(
                "Alpaca portfolio timeframe must be 1Min, 5Min, 15Min, 1H, or 1D",
            ));
        }
        if !matches!(
            intraday_reporting.as_str(),
            "market_hours" | "extended_hours" | "continuous"
        ) {
            return Err(to_pyvalue_err(
                "Alpaca intraday_reporting must be 'market_hours', 'extended_hours', or 'continuous'",
            ));
        }
        if cashflow_types.as_deref().is_some_and(|value| {
            value.is_empty()
                || value.split(',').any(|item| {
                    item.is_empty()
                        || !item
                            .bytes()
                            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit())
                })
        }) {
            return Err(to_pyvalue_err(
                "Alpaca cashflow_types must be ALL, NONE, or comma-separated uppercase activity types",
            ));
        }
        let start = start
            .map(|value| value.parse::<jiff::Timestamp>())
            .transpose()
            .map_err(|error| to_pyvalue_err(format!("Invalid Alpaca portfolio start: {error}")))?;
        let end = end
            .map(|value| value.parse::<jiff::Timestamp>())
            .transpose()
            .map_err(|error| to_pyvalue_err(format!("Invalid Alpaca portfolio end: {error}")))?;
        if start.zip(end).is_some_and(|(start, end)| start >= end) {
            return Err(to_pyvalue_err(
                "Alpaca portfolio start must be earlier than end",
            ));
        }
        if usize::from(period.is_some()) + usize::from(start.is_some()) + usize::from(end.is_some())
            > 2
        {
            return Err(to_pyvalue_err(
                "Alpaca portfolio history accepts only two of period, start, and end",
            ));
        }
        let client = self.inner.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let query = AlpacaPortfolioHistoryQuery {
                period: period.as_deref(),
                timeframe: timeframe.as_deref(),
                intraday_reporting: Some(&intraday_reporting),
                start,
                end,
                cashflow_types: cashflow_types.as_deref(),
            };
            let history = client
                .get_portfolio_history(&query)
                .await
                .map_err(to_pyvalue_err)?;
            Python::attach(|py| {
                value_to_pyobject(py, &serde_json::to_value(history).map_err(to_pyvalue_err)?)
            })
        })
    }

    /// Retrieve the current US equity market clock.
    #[pyo3(name = "get_market_clock")]
    #[gen_stub(override_return_type(
        type_repr = "typing.Awaitable[dict[str, object]]",
        imports = ("typing",)
    ))]
    fn py_get_market_clock<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let client = self.inner.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let clock = client.get_market_clock().await.map_err(to_pyvalue_err)?;
            Python::attach(|py| {
                let value = serde_json::to_value(clock).map_err(to_pyvalue_err)?;
                value_to_pyobject(py, &value)
            })
        })
    }

    /// Retrieve market sessions in an optional inclusive ISO-date range.
    #[pyo3(name = "get_calendar", signature = (start=None, end=None))]
    #[gen_stub(override_return_type(
        type_repr = "typing.Awaitable[list[dict[str, object]]]",
        imports = ("typing",)
    ))]
    fn py_get_calendar<'py>(
        &self,
        py: Python<'py>,
        start: Option<String>,
        end: Option<String>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let start_date = start
            .as_deref()
            .map(str::parse::<jiff::civil::Date>)
            .transpose()
            .map_err(|error| {
                to_pyvalue_err(format!("Invalid Alpaca calendar start date: {error}"))
            })?;
        let end_date = end
            .as_deref()
            .map(str::parse::<jiff::civil::Date>)
            .transpose()
            .map_err(|error| {
                to_pyvalue_err(format!("Invalid Alpaca calendar end date: {error}"))
            })?;
        if start_date
            .zip(end_date)
            .is_some_and(|(start, end)| start > end)
        {
            return Err(to_pyvalue_err(
                "Alpaca calendar start must not be later than end",
            ));
        }
        let client = self.inner.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let query = AlpacaCalendarQuery {
                start: start.as_deref(),
                end: end.as_deref(),
            };
            let days = client.get_calendar(&query).await.map_err(to_pyvalue_err)?;
            Python::attach(|py| {
                let value = serde_json::to_value(days).map_err(to_pyvalue_err)?;
                value_to_pyobject(py, &value)
            })
        })
    }

    /// Retrieve a bounded order history, following Alpaca pagination automatically.
    #[pyo3(name = "get_orders", signature = (status="open", symbols=None, after=None, until=None, nested=true, max_items=10_000))]
    #[gen_stub(override_return_type(
        type_repr = "typing.Awaitable[list[dict[str, object]]]",
        imports = ("typing",)
    ))]
    #[expect(clippy::too_many_arguments)]
    fn py_get_orders<'py>(
        &self,
        py: Python<'py>,
        status: &str,
        symbols: Option<String>,
        after: Option<String>,
        until: Option<String>,
        nested: bool,
        max_items: usize,
    ) -> PyResult<Bound<'py, PyAny>> {
        if !matches!(status, "open" | "closed" | "all") {
            return Err(to_pyvalue_err(
                "Alpaca order status must be 'open', 'closed', or 'all'",
            ));
        }
        if max_items == 0 || max_items > 100_000 {
            return Err(to_pyvalue_err(
                "Alpaca order max_items must be between 1 and 100000",
            ));
        }
        if symbols.as_deref().is_some_and(|value| {
            value.is_empty()
                || value
                    .split(',')
                    .any(|symbol| symbol.is_empty() || symbol.trim() != symbol)
        }) {
            return Err(to_pyvalue_err(
                "Alpaca order symbols must be a comma-separated list without empty values or whitespace",
            ));
        }
        let after = after
            .map(|value| value.parse::<jiff::Timestamp>())
            .transpose()
            .map_err(|error| {
                to_pyvalue_err(format!("Invalid Alpaca order after timestamp: {error}"))
            })?;
        let until = until
            .map(|value| value.parse::<jiff::Timestamp>())
            .transpose()
            .map_err(|error| {
                to_pyvalue_err(format!("Invalid Alpaca order until timestamp: {error}"))
            })?;
        if after
            .zip(until)
            .is_some_and(|(after, until)| after >= until)
        {
            return Err(to_pyvalue_err(
                "Alpaca order after timestamp must be earlier than until",
            ));
        }
        let client = self.inner.clone();
        let status = status.to_string();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let mut orders = Vec::with_capacity(max_items.min(500));
            let mut cursor = None::<String>;
            while orders.len() < max_items {
                let first_page = cursor.is_none();
                let remaining = max_items - orders.len();
                let limit = u16::try_from(remaining.min(500)).expect("page limit is bounded");
                let query = AlpacaOrdersQuery {
                    status: &status,
                    limit,
                    direction: "desc",
                    nested,
                    after: first_page.then_some(after).flatten(),
                    until: first_page.then_some(until).flatten(),
                    symbols: symbols.as_deref(),
                    before_order_id: cursor.as_deref(),
                };
                let page = client.get_orders(&query).await.map_err(to_pyvalue_err)?;
                let page_len = page.len();
                let next_cursor = page.last().map(|order| order.id.clone());
                if page_len == usize::from(limit) && next_cursor == cursor {
                    return Err(to_pyvalue_err("Alpaca order pagination did not advance"));
                }
                orders.extend(page);
                if page_len < usize::from(limit) {
                    break;
                }
                cursor = next_cursor;
            }
            Python::attach(|py| {
                let value = serde_json::to_value(orders).map_err(to_pyvalue_err)?;
                value_to_pyobject(py, &value)
            })
        })
    }

    /// Retrieve one order by Alpaca venue ID, optionally including contingent legs.
    #[pyo3(name = "get_order", signature = (order_id, nested=true))]
    #[gen_stub(override_return_type(
        type_repr = "typing.Awaitable[dict[str, object]]",
        imports = ("typing",)
    ))]
    fn py_get_order<'py>(
        &self,
        py: Python<'py>,
        order_id: String,
        nested: bool,
    ) -> PyResult<Bound<'py, PyAny>> {
        if order_id.trim().is_empty() || order_id.trim() != order_id {
            return Err(to_pyvalue_err(
                "Alpaca order_id must be non-empty and contain no surrounding whitespace",
            ));
        }
        let client = self.inner.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let order = client
                .get_order_with_legs(&order_id, nested)
                .await
                .map_err(to_pyvalue_err)?;
            Python::attach(|py| {
                let value = serde_json::to_value(order).map_err(to_pyvalue_err)?;
                value_to_pyobject(py, &value)
            })
        })
    }

    /// Retrieve one order by the strategy-supplied client order ID.
    #[pyo3(name = "get_order_by_client_order_id")]
    #[gen_stub(override_return_type(
        type_repr = "typing.Awaitable[dict[str, object]]",
        imports = ("typing",)
    ))]
    fn py_get_order_by_client_order_id<'py>(
        &self,
        py: Python<'py>,
        client_order_id: String,
    ) -> PyResult<Bound<'py, PyAny>> {
        if client_order_id.trim().is_empty() || client_order_id.trim() != client_order_id {
            return Err(to_pyvalue_err(
                "Alpaca client_order_id must be non-empty and contain no surrounding whitespace",
            ));
        }
        let client = self.inner.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let order = client
                .get_order_by_client_order_id(&client_order_id)
                .await
                .map_err(to_pyvalue_err)?;
            Python::attach(|py| {
                let value = serde_json::to_value(order).map_err(to_pyvalue_err)?;
                value_to_pyobject(py, &value)
            })
        })
    }
}

/// Async Python client for Alpaca's exact cash-impacting account activity ledger.
#[derive(Clone, Debug)]
#[pyclass(module = "nautilus_trader.adapters.alpaca", skip_from_py_object)]
#[pyo3_stub_gen::derive::gen_stub_pyclass(module = "nautilus_trader.adapters.alpaca")]
pub struct AlpacaAccountActivityClient {
    inner: AlpacaRawHttpClient,
}

#[pymethods]
#[pyo3_stub_gen::derive::gen_stub_pymethods]
impl AlpacaAccountActivityClient {
    #[new]
    #[pyo3(signature = (api_key=None, api_secret=None, environment=AlpacaEnvironment::Paper, base_url=None, timeout_secs=10, max_retries=3, proxy_url=None))]
    fn py_new(
        api_key: Option<String>,
        api_secret: Option<String>,
        environment: AlpacaEnvironment,
        base_url: Option<String>,
        timeout_secs: u64,
        max_retries: u32,
        proxy_url: Option<String>,
    ) -> PyResult<Self> {
        let inner = build_read_client(
            api_key,
            api_secret,
            environment,
            base_url,
            timeout_secs,
            max_retries,
            proxy_url,
        )?;
        Ok(Self { inner })
    }

    /// Retrieve an activity ledger page as dictionaries without floating-point conversion.
    #[pyo3(name = "get_activities", signature = (activity_type, start=None, end=None, order_id=None, page_token=None, page_size=100))]
    #[gen_stub(override_return_type(
        type_repr = "typing.Awaitable[list[dict[str, object]]]",
        imports = ("typing",)
    ))]
    #[expect(clippy::too_many_arguments)]
    fn py_get_activities<'py>(
        &self,
        py: Python<'py>,
        activity_type: String,
        start: Option<String>,
        end: Option<String>,
        order_id: Option<String>,
        page_token: Option<String>,
        page_size: u8,
    ) -> PyResult<Bound<'py, PyAny>> {
        if !(1..=100).contains(&page_size) {
            return Err(to_pyvalue_err(
                "Alpaca activity page_size must be between 1 and 100",
            ));
        }
        let start = start
            .map(|value| value.parse::<jiff::Timestamp>())
            .transpose()
            .map_err(to_pyvalue_err)?;
        let end = end
            .map(|value| value.parse::<jiff::Timestamp>())
            .transpose()
            .map_err(to_pyvalue_err)?;
        if start.zip(end).is_some_and(|(start, end)| start >= end) {
            return Err(to_pyvalue_err(
                "Alpaca activity start must be earlier than end",
            ));
        }
        let client = self.inner.clone();

        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let query = AlpacaActivitiesQuery {
                direction: "asc",
                page_size,
                after: start,
                until: end,
                order_id: order_id.as_deref(),
                page_token: page_token.as_deref(),
            };
            let activities = client
                .get_non_trade_activities(&activity_type, &query)
                .await
                .map_err(to_pyvalue_err)?;
            Python::attach(|py| {
                let value = serde_json::to_value(activities).map_err(to_pyvalue_err)?;
                value_to_pyobject(py, &value)
            })
        })
    }

    /// Retrieve a complete bounded activity range, following Alpaca cursors automatically.
    #[pyo3(name = "get_all_activities", signature = (activity_type, start=None, end=None, order_id=None, max_items=10_000))]
    #[gen_stub(override_return_type(
        type_repr = "typing.Awaitable[list[dict[str, object]]]",
        imports = ("typing",)
    ))]
    fn py_get_all_activities<'py>(
        &self,
        py: Python<'py>,
        activity_type: String,
        start: Option<String>,
        end: Option<String>,
        order_id: Option<String>,
        max_items: usize,
    ) -> PyResult<Bound<'py, PyAny>> {
        if max_items == 0 || max_items > 100_000 {
            return Err(to_pyvalue_err(
                "Alpaca activity max_items must be between 1 and 100000",
            ));
        }
        let start = start
            .map(|value| value.parse::<jiff::Timestamp>())
            .transpose()
            .map_err(to_pyvalue_err)?;
        let end = end
            .map(|value| value.parse::<jiff::Timestamp>())
            .transpose()
            .map_err(to_pyvalue_err)?;
        if start.zip(end).is_some_and(|(start, end)| start >= end) {
            return Err(to_pyvalue_err(
                "Alpaca activity start must be earlier than end",
            ));
        }
        let client = self.inner.clone();

        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let activities = client
                .get_all_non_trade_activities(
                    &activity_type,
                    start,
                    end,
                    order_id.as_deref(),
                    max_items,
                )
                .await
                .map_err(to_pyvalue_err)?;
            Python::attach(|py| {
                let value = serde_json::to_value(activities).map_err(to_pyvalue_err)?;
                value_to_pyobject(py, &value)
            })
        })
    }

    /// Retrieve a bounded, cursor-paginated ledger of individual trade fills.
    #[pyo3(name = "get_fills", signature = (start=None, end=None, order_id=None, max_items=10_000))]
    #[gen_stub(override_return_type(
        type_repr = "typing.Awaitable[list[dict[str, object]]]",
        imports = ("typing",)
    ))]
    fn py_get_fills<'py>(
        &self,
        py: Python<'py>,
        start: Option<String>,
        end: Option<String>,
        order_id: Option<String>,
        max_items: usize,
    ) -> PyResult<Bound<'py, PyAny>> {
        if max_items == 0 || max_items > 100_000 {
            return Err(to_pyvalue_err(
                "Alpaca fill max_items must be between 1 and 100000",
            ));
        }
        if order_id
            .as_deref()
            .is_some_and(|value| value.trim().is_empty() || value.trim() != value)
        {
            return Err(to_pyvalue_err(
                "Alpaca fill order_id must be non-empty and contain no surrounding whitespace",
            ));
        }
        let start = start
            .map(|value| value.parse::<jiff::Timestamp>())
            .transpose()
            .map_err(|error| to_pyvalue_err(format!("Invalid Alpaca fill start: {error}")))?;
        let end = end
            .map(|value| value.parse::<jiff::Timestamp>())
            .transpose()
            .map_err(|error| to_pyvalue_err(format!("Invalid Alpaca fill end: {error}")))?;
        if start.zip(end).is_some_and(|(start, end)| start >= end) {
            return Err(to_pyvalue_err("Alpaca fill start must be earlier than end"));
        }
        let client = self.inner.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let mut fills = Vec::with_capacity(max_items.min(100));
            let mut cursor = None::<String>;
            while fills.len() < max_items {
                let page_size =
                    u8::try_from((max_items - fills.len()).min(100)).expect("bounded page size");
                let query = AlpacaActivitiesQuery {
                    direction: "asc",
                    page_size,
                    after: start,
                    until: end,
                    order_id: order_id.as_deref(),
                    page_token: cursor.as_deref(),
                };
                let page = client
                    .get_fill_activities(&query)
                    .await
                    .map_err(to_pyvalue_err)?;
                let page_len = page.len();
                let next_cursor = page.last().map(|fill| fill.id.clone());
                if page_len == usize::from(page_size) && next_cursor == cursor {
                    return Err(to_pyvalue_err("Alpaca fill pagination did not advance"));
                }
                fills.extend(page);
                if page_len < usize::from(page_size) {
                    break;
                }
                cursor = next_cursor;
            }
            Python::attach(|py| {
                value_to_pyobject(py, &serde_json::to_value(fills).map_err(to_pyvalue_err)?)
            })
        })
    }
}
