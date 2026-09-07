// -------------------------------------------------------------------------------------------------
//  Copyright (C) 2015-2026 Nautech Systems Pty Ltd. All rights reserved.
//  https://nautechsystems.io
//
//  Licensed under the GNU Lesser General Public License Version 3.0 (the "License");
// -------------------------------------------------------------------------------------------------

use std::collections::HashMap;

use async_trait::async_trait;
use nautilus_common::providers::{InstrumentProvider, InstrumentStore};
use nautilus_core::UnixNanos;
use nautilus_model::{
    identifiers::InstrumentId,
    instruments::{Instrument, InstrumentAny},
};

use crate::http::{client::AlpacaRawHttpClient, parse::parse_equity, query::AlpacaAssetsQuery};

/// Loads Alpaca's asset master and replaces cached definitions atomically after a successful parse.
#[derive(Debug)]
pub struct AlpacaInstrumentProvider {
    client: AlpacaRawHttpClient,
    store: InstrumentStore,
}

impl AlpacaInstrumentProvider {
    #[must_use]
    pub fn new(client: AlpacaRawHttpClient) -> Self {
        Self {
            client,
            store: InstrumentStore::new(),
        }
    }

    fn parse_assets(
        assets: &[crate::http::models::AlpacaAsset],
        ts_init: UnixNanos,
    ) -> anyhow::Result<Vec<InstrumentAny>> {
        assets
            .iter()
            .map(|asset| {
                parse_equity(asset, ts_init)
                    .map(InstrumentAny::Equity)
                    .map_err(|error| anyhow::anyhow!("Failed to parse {}: {error}", asset.symbol))
            })
            .collect()
    }

    fn replace_store(&mut self, instruments: Vec<InstrumentAny>) {
        self.store.clear();
        self.store.add_bulk(instruments);
        self.store.set_initialized();
    }

    /// Loads only the requested Alpaca instruments and replaces the current store atomically.
    pub async fn load_ids(&mut self, instrument_ids: &[InstrumentId]) -> anyhow::Result<()> {
        anyhow::ensure!(
            !instrument_ids.is_empty(),
            "No Alpaca instrument IDs configured"
        );

        let mut instruments = Vec::with_capacity(instrument_ids.len());
        for instrument_id in instrument_ids {
            anyhow::ensure!(
                instrument_id.venue.to_string() == "ALPACA",
                "Expected ALPACA instrument, received {instrument_id}"
            );
            let asset = self.client.get_asset(instrument_id.symbol.as_str()).await?;
            let instrument = parse_equity(&asset, UnixNanos::from(jiff::Timestamp::now()))?;
            anyhow::ensure!(
                instrument.id() == *instrument_id,
                "Alpaca returned {} for requested {instrument_id}",
                instrument.id()
            );
            instruments.push(InstrumentAny::Equity(instrument));
        }

        self.replace_store(instruments);
        Ok(())
    }
}

#[async_trait(?Send)]
impl InstrumentProvider for AlpacaInstrumentProvider {
    fn store(&self) -> &InstrumentStore {
        &self.store
    }

    fn store_mut(&mut self) -> &mut InstrumentStore {
        &mut self.store
    }

    async fn load_all(&mut self, filters: Option<&HashMap<String, String>>) -> anyhow::Result<()> {
        let status = filters
            .and_then(|values| values.get("status"))
            .map(String::as_str)
            .or(Some("active"));
        let exchange = filters
            .and_then(|values| values.get("exchange"))
            .map(String::as_str);
        let attributes = filters
            .and_then(|values| values.get("attributes"))
            .map(String::as_str);
        let query = AlpacaAssetsQuery {
            status,
            asset_class: "us_equity",
            exchange,
            attributes,
        };
        let assets = self.client.get_assets(&query).await?;
        let instruments = Self::parse_assets(&assets, UnixNanos::from(jiff::Timestamp::now()))?;

        self.replace_store(instruments);
        Ok(())
    }

    async fn load(
        &mut self,
        instrument_id: &InstrumentId,
        _filters: Option<&HashMap<String, String>>,
    ) -> anyhow::Result<()> {
        anyhow::ensure!(
            instrument_id.venue.to_string() == "ALPACA",
            "Expected ALPACA instrument, received {instrument_id}"
        );
        let asset = self.client.get_asset(instrument_id.symbol.as_str()).await?;
        let instrument = parse_equity(&asset, UnixNanos::from(jiff::Timestamp::now()))?;
        anyhow::ensure!(
            instrument.id() == *instrument_id,
            "Alpaca returned {} for requested {instrument_id}",
            instrument.id()
        );
        self.store.add(InstrumentAny::Equity(instrument));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    use axum::{
        Json, Router,
        extract::{Path, Query, State},
        http::HeaderMap,
        routing::get,
    };
    use nautilus_common::providers::InstrumentProvider;
    use nautilus_model::types::{Price, Quantity};
    use rstest::rstest;
    use serde_json::{Value, json};
    use tokio::net::TcpListener;

    use super::*;
    use crate::common::{
        consts::{ALPACA_API_KEY_HEADER, ALPACA_API_SECRET_HEADER},
        credential::AlpacaCredential,
    };

    async fn assets_handler(
        State(request_count): State<Arc<AtomicUsize>>,
        Query(query): Query<HashMap<String, String>>,
        headers: HeaderMap,
    ) -> Json<Value> {
        assert_eq!(query.get("status").map(String::as_str), Some("active"));
        assert_eq!(
            query.get("asset_class").map(String::as_str),
            Some("us_equity")
        );
        assert_eq!(headers[ALPACA_API_KEY_HEADER], "test-key");
        assert_eq!(headers[ALPACA_API_SECRET_HEADER], "test-secret");

        let symbol = if request_count.fetch_add(1, Ordering::SeqCst) == 0 {
            "AAPL"
        } else {
            "MSFT"
        };
        Json(json!([{
            "id": format!("asset-{symbol}"),
            "class": "us_equity",
            "exchange": "NASDAQ",
            "symbol": symbol,
            "name": format!("{symbol} Inc."),
            "status": "active",
            "tradable": true,
            "marginable": true,
            "shortable": true,
            "easy_to_borrow": true,
            "fractionable": true,
            "attributes": []
        }]))
    }

    async fn asset_handler(
        State(request_count): State<Arc<AtomicUsize>>,
        Path(symbol): Path<String>,
        headers: HeaderMap,
    ) -> Json<Value> {
        assert_eq!(headers[ALPACA_API_KEY_HEADER], "test-key");
        assert_eq!(headers[ALPACA_API_SECRET_HEADER], "test-secret");
        request_count.fetch_add(1, Ordering::SeqCst);
        Json(json!({
            "id": format!("asset-{symbol}"),
            "class": "us_equity",
            "exchange": "NASDAQ",
            "symbol": symbol,
            "name": "Selected Inc.",
            "status": "active",
            "tradable": true,
            "marginable": true,
            "shortable": true,
            "easy_to_borrow": true,
            "fractionable": true,
            "attributes": []
        }))
    }

    #[rstest]
    #[tokio::test]
    async fn test_load_all_fetches_fresh_asset_master_and_replaces_stale_store() {
        let request_count = Arc::new(AtomicUsize::new(0));
        let app = Router::new()
            .route("/v2/assets", get(assets_handler))
            .with_state(Arc::clone(&request_count));
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let base_url = format!("http://{address}");
        let client = AlpacaRawHttpClient::with_base_urls(
            AlpacaCredential::new("test-key", "test-secret"),
            base_url.clone(),
            base_url,
            10,
        )
        .unwrap();
        let mut provider = AlpacaInstrumentProvider::new(client);

        provider.load_all(None).await.unwrap();
        assert!(
            provider
                .store()
                .contains(&InstrumentId::from("AAPL.ALPACA"))
        );
        assert_eq!(provider.store().count(), 1);
        let instrument = provider
            .store()
            .find(&InstrumentId::from("AAPL.ALPACA"))
            .unwrap();
        assert_eq!(instrument.price_increment(), Price::from("0.0001"));
        assert_eq!(instrument.size_increment(), Quantity::from("0.000000001"));

        provider.load_all(None).await.unwrap();
        assert!(
            !provider
                .store()
                .contains(&InstrumentId::from("AAPL.ALPACA"))
        );
        assert!(
            provider
                .store()
                .contains(&InstrumentId::from("MSFT.ALPACA"))
        );
        assert_eq!(provider.store().count(), 1);
        assert_eq!(request_count.load(Ordering::SeqCst), 2);

        server.abort();
    }

    #[rstest]
    #[tokio::test]
    async fn test_load_ids_fetches_only_selected_assets() {
        let request_count = Arc::new(AtomicUsize::new(0));
        let app = Router::new()
            .route("/v2/assets/{symbol}", get(asset_handler))
            .with_state(Arc::clone(&request_count));
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let base_url = format!("http://{address}");
        let client = AlpacaRawHttpClient::with_base_urls(
            AlpacaCredential::new("test-key", "test-secret"),
            base_url.clone(),
            base_url,
            10,
        )
        .unwrap();
        let mut provider = AlpacaInstrumentProvider::new(client);
        let selected = [
            InstrumentId::from("AAPL.ALPACA"),
            InstrumentId::from("MSFT.ALPACA"),
        ];

        provider.load_ids(&selected).await.unwrap();

        assert_eq!(provider.store().count(), 2);
        assert!(provider.store().contains(&selected[0]));
        assert!(provider.store().contains(&selected[1]));
        assert_eq!(request_count.load(Ordering::SeqCst), 2);

        server.abort();
    }
}
