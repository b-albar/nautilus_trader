// -------------------------------------------------------------------------------------------------
//  Copyright (C) 2015-2026 Nautech Systems Pty Ltd. All rights reserved.
//  https://nautechsystems.io
//
//  Licensed under the GNU Lesser General Public License Version 3.0 (the "License");
// -------------------------------------------------------------------------------------------------

use jiff::Timestamp;
use serde::Serialize;

use crate::common::enums::{AlpacaBarAdjustment, AlpacaDataFeed};

#[derive(Clone, Copy, Debug, Default, Serialize)]
pub struct AlpacaEmptyQuery {}

#[derive(Clone, Debug, Default, Serialize)]
pub struct AlpacaAssetsQuery<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<&'a str>,
    #[serde(rename = "asset_class")]
    pub asset_class: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exchange: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attributes: Option<&'a str>,
}

impl AlpacaAssetsQuery<'_> {
    #[must_use]
    pub const fn active_us_equities() -> Self {
        Self {
            status: Some("active"),
            asset_class: "us_equity",
            exchange: None,
            attributes: None,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct AlpacaBarsQuery<'a> {
    pub symbols: &'a str,
    pub timeframe: &'a str,
    pub start: Timestamp,
    pub end: Timestamp,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
    pub adjustment: AlpacaBarAdjustment,
    pub feed: AlpacaDataFeed,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub asof: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page_token: Option<&'a str>,
}

impl<'a> AlpacaBarsQuery<'a> {
    #[must_use]
    pub fn new(
        symbols: &'a str,
        timeframe: &'a str,
        start: Timestamp,
        end: Timestamp,
        feed: AlpacaDataFeed,
        adjustment: AlpacaBarAdjustment,
    ) -> Self {
        Self {
            symbols,
            timeframe,
            start,
            end,
            limit: None,
            adjustment,
            feed,
            asof: None,
            page_token: None,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct AlpacaTicksQuery<'a> {
    pub symbols: &'a str,
    pub start: Timestamp,
    pub end: Timestamp,
    pub limit: u32,
    pub feed: AlpacaDataFeed,
    pub sort: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub asof: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page_token: Option<&'a str>,
}

#[derive(Clone, Copy, Debug, Serialize)]
pub struct AlpacaFeedQuery {
    pub feed: AlpacaDataFeed,
}

#[derive(Clone, Copy, Debug, Serialize)]
pub struct AlpacaSnapshotsQuery<'a> {
    pub symbols: &'a str,
    pub feed: AlpacaDataFeed,
}

#[derive(Clone, Debug, Serialize)]
pub struct AlpacaOptionContractsQuery<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub underlying_symbols: Option<&'a str>,
    pub show_deliverables: bool,
    pub status: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expiration_date: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expiration_date_gte: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expiration_date_lte: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub root_symbol: Option<&'a str>,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub option_type: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub style: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strike_price_gte: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strike_price_lte: Option<&'a str>,
    pub limit: u16,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page_token: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ppind: Option<bool>,
}

#[derive(Clone, Debug, Serialize)]
pub struct AlpacaOptionChainQuery<'a> {
    pub feed: &'a str,
    pub limit: u16,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_since: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page_token: Option<&'a str>,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub option_type: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strike_price_gte: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strike_price_lte: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expiration_date: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expiration_date_gte: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expiration_date_lte: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub root_symbol: Option<&'a str>,
}

#[derive(Clone, Debug, Serialize)]
pub struct AlpacaOptionSnapshotsQuery<'a> {
    pub symbols: &'a str,
    pub feed: &'a str,
    pub limit: u16,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_since: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page_token: Option<&'a str>,
}

#[derive(Clone, Debug, Serialize)]
pub struct AlpacaOptionBarsQuery<'a> {
    pub symbols: &'a str,
    pub timeframe: &'a str,
    pub start: Timestamp,
    pub end: Timestamp,
    pub limit: u16,
    pub sort: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page_token: Option<&'a str>,
}

#[derive(Clone, Debug, Serialize)]
pub struct AlpacaOptionTradesQuery<'a> {
    pub symbols: &'a str,
    pub start: Timestamp,
    pub end: Timestamp,
    pub limit: u16,
    pub sort: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page_token: Option<&'a str>,
}

#[derive(Clone, Copy, Debug, Serialize)]
pub struct AlpacaMostActivesQuery<'a> {
    pub by: &'a str,
    pub top: u8,
}

#[derive(Clone, Copy, Debug, Serialize)]
pub struct AlpacaMarketMoversQuery {
    pub top: u8,
}

#[derive(Clone, Copy, Debug, Serialize)]
pub struct AlpacaConditionsQuery<'a> {
    pub tape: &'a str,
}

#[derive(Clone, Debug, Serialize)]
pub struct AlpacaNewsQuery<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start: Option<Timestamp>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end: Option<Timestamp>,
    pub sort: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbols: Option<&'a str>,
    pub limit: u8,
    pub include_content: bool,
    pub exclude_contentless: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page_token: Option<&'a str>,
}

#[derive(Clone, Debug, Serialize)]
pub struct AlpacaCorporateActionsQuery<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbols: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cusips: Option<&'a str>,
    #[serde(rename = "types", skip_serializing_if = "Option::is_none")]
    pub action_types: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ids: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub region: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data_quality: Option<&'a str>,
    pub limit: u16,
    pub sort: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page_token: Option<&'a str>,
}

impl<'a> AlpacaTicksQuery<'a> {
    #[must_use]
    pub const fn new(
        symbols: &'a str,
        start: Timestamp,
        end: Timestamp,
        feed: AlpacaDataFeed,
    ) -> Self {
        Self {
            symbols,
            start,
            end,
            limit: 10_000,
            feed,
            sort: "asc",
            asof: None,
            page_token: None,
        }
    }
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct AlpacaOrdersQuery<'a> {
    pub status: &'a str,
    pub limit: u16,
    pub direction: &'a str,
    pub nested: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub after: Option<Timestamp>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub until: Option<Timestamp>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbols: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub before_order_id: Option<&'a str>,
}

#[derive(Clone, Debug, Serialize)]
pub struct AlpacaClientOrderQuery<'a> {
    pub client_order_id: &'a str,
}

#[derive(Clone, Debug, Serialize)]
pub struct AlpacaWatchlistNameQuery<'a> {
    pub name: &'a str,
}

#[derive(Clone, Copy, Debug, Serialize)]
pub struct AlpacaOrderQuery {
    pub nested: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct AlpacaActivitiesQuery<'a> {
    pub direction: &'a str,
    pub page_size: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub after: Option<Timestamp>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub until: Option<Timestamp>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub order_id: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page_token: Option<&'a str>,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct AlpacaCalendarQuery<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end: Option<&'a str>,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct AlpacaPortfolioHistoryQuery<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub period: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeframe: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub intraday_reporting: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start: Option<Timestamp>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end: Option<Timestamp>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cashflow_types: Option<&'a str>,
}

impl Default for AlpacaActivitiesQuery<'_> {
    fn default() -> Self {
        Self {
            direction: "asc",
            page_size: 100,
            after: None,
            until: None,
            order_id: None,
            page_token: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    #[rstest]
    fn test_bar_query_serializes_explicit_corporate_action_adjustment() {
        let query = AlpacaBarsQuery::new(
            "AAPL",
            "1Day",
            "2024-01-01T00:00:00Z".parse().unwrap(),
            "2024-02-01T00:00:00Z".parse().unwrap(),
            AlpacaDataFeed::Sip,
            AlpacaBarAdjustment::All,
        );

        let encoded = serde_json::to_value(query).unwrap();

        assert_eq!(encoded["adjustment"], "all");
        assert_eq!(encoded["feed"], "sip");
    }
}
