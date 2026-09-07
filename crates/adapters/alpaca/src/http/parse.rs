// -------------------------------------------------------------------------------------------------
//  Copyright (C) 2015-2026 Nautech Systems Pty Ltd. All rights reserved.
//  https://nautechsystems.io
//
//  Licensed under the GNU Lesser General Public License Version 3.0 (the "License");
// -------------------------------------------------------------------------------------------------

use nautilus_core::{Params, UnixNanos};
use nautilus_model::{
    identifiers::{InstrumentId, Symbol},
    instruments::Equity,
    types::{Currency, Price, Quantity},
};
use rust_decimal::Decimal;
use serde_json::json;

use super::models::{AlpacaAsset, AlpacaAssetClass};

/// Converts an Alpaca US equity definition into a Nautilus instrument.
///
/// # Errors
///
/// Returns an error for non-equity assets, invalid venue-provided increments, unsupported
/// precision, or a domain validation failure.
pub fn parse_equity(asset: &AlpacaAsset, ts_init: UnixNanos) -> anyhow::Result<Equity> {
    anyhow::ensure!(
        asset.asset_class == AlpacaAssetClass::UsEquity,
        "Unsupported Alpaca asset class for {}: {:?}",
        asset.symbol,
        asset.asset_class
    );

    // Alpaca documents increment fields as crypto-only. Equity orders accept four decimals below
    // $1 and two at or above $1, so 0.0001 is the instrument-wide resolution; execution applies
    // the price-dependent restriction before submission.
    let price_increment = asset.price_increment.unwrap_or_else(|| Decimal::new(1, 4));
    anyhow::ensure!(
        price_increment > Decimal::ZERO,
        "Alpaca asset {} has non-positive price_increment {price_increment}",
        asset.symbol
    );

    let normalized_price_increment = price_increment.normalize();
    let price_precision = u8::try_from(normalized_price_increment.scale())?;
    let size_increment = if asset.fractionable {
        asset
            .min_trade_increment
            .unwrap_or_else(|| Decimal::new(1, 9))
    } else {
        Decimal::ONE
    };
    anyhow::ensure!(
        size_increment > Decimal::ZERO,
        "Alpaca asset {} has non-positive min_trade_increment {size_increment}",
        asset.symbol
    );
    let size_increment = size_increment.normalize();
    let size_precision = u8::try_from(size_increment.scale())?;
    let min_quantity = if asset.fractionable {
        asset.min_order_size.unwrap_or(size_increment)
    } else {
        Decimal::ONE
    };
    anyhow::ensure!(
        min_quantity > Decimal::ZERO,
        "Alpaca asset {} has non-positive min_order_size {min_quantity}",
        asset.symbol
    );

    let mut info = Params::new();
    info.insert("alpaca_asset_id".to_string(), json!(asset.id));
    info.insert("name".to_string(), json!(asset.name));
    info.insert("exchange".to_string(), json!(asset.exchange));
    info.insert("status".to_string(), json!(format!("{:?}", asset.status)));
    info.insert("tradable".to_string(), json!(asset.tradable));
    info.insert("marginable".to_string(), json!(asset.marginable));
    info.insert("shortable".to_string(), json!(asset.shortable));
    let easy_to_borrow = asset
        .borrow_status
        .as_deref()
        .map(|status| status == "easy_to_borrow")
        .or(asset.easy_to_borrow);
    info.insert("borrow_status".to_string(), json!(asset.borrow_status));
    info.insert("easy_to_borrow".to_string(), json!(easy_to_borrow));
    info.insert("fractionable".to_string(), json!(asset.fractionable));
    info.insert("cusip".to_string(), json!(asset.cusip));
    info.insert(
        "margin_requirement_long".to_string(),
        json!(asset.margin_requirement_long.map(|value| value.to_string())),
    );
    info.insert(
        "margin_requirement_short".to_string(),
        json!(
            asset
                .margin_requirement_short
                .map(|value| value.to_string())
        ),
    );
    info.insert(
        "maintenance_margin_requirement".to_string(),
        json!(
            asset
                .maintenance_margin_requirement
                .map(|value| value.to_string())
        ),
    );
    info.insert(
        "min_order_size".to_string(),
        json!(asset.min_order_size.map(|value| value.to_string())),
    );
    info.insert(
        "min_trade_increment".to_string(),
        json!(asset.min_trade_increment.map(|value| value.to_string())),
    );
    info.insert(
        "increment_source".to_string(),
        json!(
            if asset.price_increment.is_some() || asset.min_trade_increment.is_some() {
                "asset_payload"
            } else {
                "alpaca_equity_protocol"
            }
        ),
    );
    info.insert(
        "quantity_scope".to_string(),
        json!(if asset.fractionable {
            "fractional_shares"
        } else {
            "whole_shares_only"
        }),
    );
    info.insert("attributes".to_string(), json!(asset.attributes));

    Equity::builder()
        .instrument_id(InstrumentId::from(format!("{}.ALPACA", asset.symbol)))
        .raw_symbol(Symbol::from(asset.symbol.as_str()))
        .currency(Currency::USD())
        .price_precision(price_precision)
        .price_increment(Price::from_decimal(normalized_price_increment)?)
        .size_precision(size_precision)
        .size_increment(Quantity::from_decimal(size_increment)?)
        .lot_size(Quantity::from(100))
        .min_quantity(Quantity::from_decimal(min_quantity)?)
        .info(info)
        .ts_event(ts_init)
        .ts_init(ts_init)
        .build()
        .map_err(anyhow::Error::from)
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use nautilus_model::instruments::Instrument;
    use rstest::rstest;

    use super::*;
    use crate::http::models::AlpacaAssetStatus;

    fn asset() -> AlpacaAsset {
        AlpacaAsset {
            id: "b0b6dd9d-8b9b-48a9-ba46-b9d54906e415".to_string(),
            asset_class: AlpacaAssetClass::UsEquity,
            exchange: "NASDAQ".to_string(),
            symbol: "AAPL".to_string(),
            name: "Apple Inc. Common Stock".to_string(),
            status: AlpacaAssetStatus::Active,
            tradable: true,
            marginable: true,
            shortable: true,
            borrow_status: Some("easy_to_borrow".to_string()),
            easy_to_borrow: None,
            fractionable: true,
            cusip: Some("037833100".to_string()),
            maintenance_margin_requirement: Some(Decimal::from(30)),
            margin_requirement_long: Some(Decimal::from(30)),
            margin_requirement_short: Some(Decimal::from(35)),
            min_order_size: Some(Decimal::from_str("0.000001").unwrap()),
            min_trade_increment: Some(Decimal::from_str("0.000001").unwrap()),
            price_increment: Some(Decimal::from_str("0.01").unwrap()),
            attributes: vec!["fractional_eh_enabled".to_string()],
        }
    }

    #[rstest]
    fn test_parse_fractionable_equity_preserves_price_and_quantity_increments() {
        let equity = parse_equity(&asset(), UnixNanos::from(42)).unwrap();

        assert_eq!(equity.id.to_string(), "AAPL.ALPACA");
        assert_eq!(equity.price_increment, Price::from("0.01"));
        assert_eq!(equity.price_precision, 2);
        assert_eq!(equity.min_quantity, Some(Quantity::from("0.000001")));
        assert_eq!(equity.size_precision(), 6);
        assert_eq!(equity.size_increment(), Quantity::from("0.000001"));
        assert_eq!(equity.lot_size, Some(Quantity::from(100)));
        assert_eq!(equity.ts_event, UnixNanos::from(42));
        assert_eq!(
            equity.info.as_ref().unwrap()["quantity_scope"],
            json!("fractional_shares")
        );
        assert_eq!(
            equity.info.as_ref().unwrap()["borrow_status"],
            json!("easy_to_borrow")
        );
        assert_eq!(equity.info.as_ref().unwrap()["easy_to_borrow"], json!(true));
        assert_eq!(equity.info.as_ref().unwrap()["cusip"], json!("037833100"));
        assert_eq!(
            equity.info.as_ref().unwrap()["margin_requirement_long"],
            json!("30")
        );
        assert_eq!(
            equity.info.as_ref().unwrap()["margin_requirement_short"],
            json!("35")
        );
    }

    #[rstest]
    fn test_parse_non_fractionable_equity_keeps_whole_share_increment() {
        let mut asset = asset();
        asset.fractionable = false;

        let equity = parse_equity(&asset, UnixNanos::from(42)).unwrap();

        assert_eq!(equity.min_quantity, Some(Quantity::from(1)));
        assert_eq!(equity.size_precision(), 0);
        assert_eq!(equity.size_increment(), Quantity::from(1));
        assert_eq!(
            equity.info.as_ref().unwrap()["quantity_scope"],
            json!("whole_shares_only")
        );
    }

    #[rstest]
    fn test_fractional_increment_is_preserved_as_metadata() {
        let mut asset = asset();
        asset.min_trade_increment = Some(Decimal::from_str("0.000001").unwrap());

        let equity = parse_equity(&asset, UnixNanos::from(1)).unwrap();

        assert_eq!(
            equity.info.as_ref().unwrap()["min_trade_increment"],
            json!("0.000001")
        );
    }

    #[rstest]
    fn test_legacy_easy_to_borrow_is_used_when_borrow_status_is_absent() {
        let mut asset = asset();
        asset.borrow_status = None;
        asset.easy_to_borrow = Some(true);

        let equity = parse_equity(&asset, UnixNanos::from(1)).unwrap();

        assert_eq!(equity.info.as_ref().unwrap()["borrow_status"], json!(null));
        assert_eq!(equity.info.as_ref().unwrap()["easy_to_borrow"], json!(true));
    }

    #[rstest]
    fn test_official_equity_payload_without_crypto_only_increments_uses_protocol_resolution() {
        let mut asset = asset();
        asset.price_increment = None;
        asset.min_order_size = None;
        asset.min_trade_increment = None;

        let equity = parse_equity(&asset, UnixNanos::from(1)).unwrap();

        assert_eq!(equity.price_increment, Price::from("0.0001"));
        assert_eq!(equity.price_precision, 4);
        assert_eq!(equity.size_increment(), Quantity::from("0.000000001"));
        assert_eq!(equity.min_quantity, Some(Quantity::from("0.000000001")));
        assert_eq!(
            equity.info.as_ref().unwrap()["increment_source"],
            json!("alpaca_equity_protocol")
        );
    }

    #[rstest]
    fn test_invalid_venue_provided_increment_is_rejected() {
        let mut asset = asset();
        asset.price_increment = Some(Decimal::ZERO);

        let error = parse_equity(&asset, UnixNanos::from(1)).unwrap_err();

        assert!(error.to_string().contains("non-positive price_increment"));
    }
}
