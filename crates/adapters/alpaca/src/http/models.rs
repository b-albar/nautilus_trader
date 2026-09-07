// -------------------------------------------------------------------------------------------------
//  Copyright (C) 2015-2026 Nautech Systems Pty Ltd. All rights reserved.
//  https://nautechsystems.io
//
//  Licensed under the GNU Lesser General Public License Version 3.0 (the "License");
// -------------------------------------------------------------------------------------------------

use std::collections::BTreeMap;

use jiff::Timestamp;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AlpacaAssetClass {
    UsEquity,
    Crypto,
    #[serde(other)]
    Unknown,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum AlpacaAssetStatus {
    Active,
    Inactive,
    #[serde(other)]
    Unknown,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct AlpacaAsset {
    pub id: String,
    #[serde(rename = "class")]
    pub asset_class: AlpacaAssetClass,
    pub exchange: String,
    pub symbol: String,
    pub name: String,
    pub status: AlpacaAssetStatus,
    pub tradable: bool,
    pub marginable: bool,
    pub shortable: bool,
    #[serde(default)]
    pub borrow_status: Option<String>,
    #[serde(default)]
    pub easy_to_borrow: Option<bool>,
    pub fractionable: bool,
    #[serde(default)]
    pub cusip: Option<String>,
    #[serde(default)]
    pub maintenance_margin_requirement: Option<Decimal>,
    #[serde(default)]
    pub margin_requirement_long: Option<Decimal>,
    #[serde(default)]
    pub margin_requirement_short: Option<Decimal>,
    #[serde(default)]
    pub min_order_size: Option<Decimal>,
    #[serde(default)]
    pub min_trade_increment: Option<Decimal>,
    #[serde(default)]
    pub price_increment: Option<Decimal>,
    #[serde(default)]
    pub attributes: Vec<String>,
}

/// A user watchlist and its resolved Alpaca assets.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct AlpacaWatchlist {
    pub id: String,
    pub account_id: String,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
    pub name: String,
    #[serde(default)]
    pub assets: Vec<AlpacaAsset>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct AlpacaBar {
    #[serde(rename(deserialize = "t"))]
    pub timestamp: Timestamp,
    #[serde(rename(deserialize = "o"))]
    pub open: Decimal,
    #[serde(rename(deserialize = "h"))]
    pub high: Decimal,
    #[serde(rename(deserialize = "l"))]
    pub low: Decimal,
    #[serde(rename(deserialize = "c"))]
    pub close: Decimal,
    #[serde(rename(deserialize = "v"))]
    pub volume: Decimal,
    #[serde(rename(deserialize = "n"))]
    pub trade_count: u64,
    #[serde(rename(deserialize = "vw"))]
    pub volume_weighted_price: Decimal,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct AlpacaBarsResponse {
    pub bars: BTreeMap<String, Vec<AlpacaBar>>,
    pub next_page_token: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct AlpacaHistoricalTrade {
    #[serde(rename(deserialize = "i"))]
    pub trade_id: u64,
    #[serde(rename(deserialize = "p"))]
    pub price: Decimal,
    #[serde(rename(deserialize = "s"))]
    pub size: Decimal,
    #[serde(rename(deserialize = "t"))]
    pub timestamp: Timestamp,
    #[serde(rename(deserialize = "x"), default)]
    pub exchange: Option<String>,
    #[serde(rename(deserialize = "c"), default)]
    pub conditions: Vec<String>,
    #[serde(rename(deserialize = "z"), default)]
    pub tape: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct AlpacaTradesResponse {
    pub trades: BTreeMap<String, Vec<AlpacaHistoricalTrade>>,
    pub next_page_token: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct AlpacaHistoricalQuote {
    #[serde(rename(deserialize = "bx"), default)]
    pub bid_exchange: Option<String>,
    #[serde(rename(deserialize = "bp"))]
    pub bid_price: Decimal,
    #[serde(rename(deserialize = "bs"))]
    pub bid_size_lots: Decimal,
    #[serde(rename(deserialize = "ap"))]
    pub ask_price: Decimal,
    #[serde(rename(deserialize = "as"))]
    pub ask_size_lots: Decimal,
    #[serde(rename(deserialize = "ax"), default)]
    pub ask_exchange: Option<String>,
    #[serde(rename(deserialize = "t"))]
    pub timestamp: Timestamp,
    #[serde(rename(deserialize = "c"), default)]
    pub conditions: Vec<String>,
    #[serde(rename(deserialize = "z"), default)]
    pub tape: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct AlpacaQuotesResponse {
    pub quotes: BTreeMap<String, Vec<AlpacaHistoricalQuote>>,
    pub next_page_token: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct AlpacaAuctionPrint {
    #[serde(rename(deserialize = "t"))]
    pub timestamp: Timestamp,
    #[serde(rename(deserialize = "x"))]
    pub exchange: String,
    #[serde(rename(deserialize = "p"))]
    pub price: Decimal,
    #[serde(rename(deserialize = "s"), default)]
    pub size: Option<Decimal>,
    #[serde(rename(deserialize = "c"))]
    pub condition: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct AlpacaDailyAuctions {
    #[serde(rename(deserialize = "d"))]
    pub date: jiff::civil::Date,
    #[serde(rename(deserialize = "o"))]
    pub opening: Vec<AlpacaAuctionPrint>,
    #[serde(rename(deserialize = "c"))]
    pub closing: Vec<AlpacaAuctionPrint>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct AlpacaAuctionsResponse {
    pub auctions: BTreeMap<String, Vec<AlpacaDailyAuctions>>,
    pub next_page_token: Option<String>,
    #[serde(default)]
    pub currency: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct AlpacaMostActive {
    pub symbol: String,
    pub volume: u64,
    pub trade_count: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct AlpacaMostActivesResponse {
    pub most_actives: Vec<AlpacaMostActive>,
    pub last_updated: Timestamp,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct AlpacaMarketMover {
    pub symbol: String,
    pub percent_change: Decimal,
    pub change: Decimal,
    pub price: Decimal,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct AlpacaMarketMoversResponse {
    pub gainers: Vec<AlpacaMarketMover>,
    pub losers: Vec<AlpacaMarketMover>,
    pub market_type: String,
    pub last_updated: Timestamp,
}

/// Latest consolidated market state for one US equity symbol.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct AlpacaStockSnapshot {
    #[serde(rename(deserialize = "latestTrade"), default)]
    pub latest_trade: Option<AlpacaHistoricalTrade>,
    #[serde(rename(deserialize = "latestQuote"), default)]
    pub latest_quote: Option<AlpacaHistoricalQuote>,
    #[serde(rename(deserialize = "minuteBar"), default)]
    pub minute_bar: Option<AlpacaBar>,
    #[serde(rename(deserialize = "dailyBar"), default)]
    pub daily_bar: Option<AlpacaBar>,
    #[serde(rename(deserialize = "prevDailyBar"), default)]
    pub previous_daily_bar: Option<AlpacaBar>,
}

/// A deliverable received when an option is exercised or assigned.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct AlpacaOptionDeliverable {
    #[serde(rename(deserialize = "type"))]
    pub deliverable_type: String,
    pub symbol: String,
    #[serde(default)]
    pub asset_id: Option<String>,
    pub amount: Option<Decimal>,
    pub allocation_percentage: Decimal,
    pub settlement_type: String,
    pub settlement_method: String,
    pub delayed_settlement: bool,
}

/// An OCC option contract returned by Alpaca's trading reference-data API.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct AlpacaOptionContract {
    pub id: String,
    pub symbol: String,
    pub name: String,
    pub status: String,
    pub tradable: bool,
    pub expiration_date: jiff::civil::Date,
    pub underlying_symbol: String,
    pub underlying_asset_id: String,
    #[serde(rename(deserialize = "type"))]
    pub option_type: String,
    pub style: String,
    pub strike_price: Decimal,
    pub multiplier: Decimal,
    pub size: Decimal,
    #[serde(default)]
    pub root_symbol: Option<String>,
    #[serde(default)]
    pub open_interest: Option<Decimal>,
    #[serde(default)]
    pub open_interest_date: Option<jiff::civil::Date>,
    #[serde(default)]
    pub close_price: Option<Decimal>,
    #[serde(default)]
    pub close_price_date: Option<jiff::civil::Date>,
    #[serde(default)]
    pub deliverables: Vec<AlpacaOptionDeliverable>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct AlpacaOptionContractsResponse {
    pub option_contracts: Vec<AlpacaOptionContract>,
    pub next_page_token: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct AlpacaOptionGreeks {
    pub delta: Decimal,
    pub gamma: Decimal,
    pub theta: Decimal,
    pub vega: Decimal,
    pub rho: Decimal,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct AlpacaOptionQuote {
    #[serde(rename(deserialize = "t"))]
    pub timestamp: Timestamp,
    #[serde(rename(deserialize = "bx"))]
    pub bid_exchange: String,
    #[serde(rename(deserialize = "bp"))]
    pub bid_price: Decimal,
    #[serde(rename(deserialize = "bs"))]
    pub bid_size: u64,
    #[serde(rename(deserialize = "ap"))]
    pub ask_price: Decimal,
    #[serde(rename(deserialize = "as"))]
    pub ask_size: u64,
    #[serde(rename(deserialize = "ax"))]
    pub ask_exchange: String,
    #[serde(rename(deserialize = "c"))]
    pub condition: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct AlpacaOptionTrade {
    #[serde(rename(deserialize = "t"))]
    pub timestamp: Timestamp,
    #[serde(rename(deserialize = "x"))]
    pub exchange: String,
    #[serde(rename(deserialize = "p"))]
    pub price: Decimal,
    #[serde(rename(deserialize = "s"))]
    pub size: u64,
    #[serde(rename(deserialize = "c"))]
    pub condition: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct AlpacaOptionSnapshot {
    #[serde(rename(deserialize = "latestTrade"), default)]
    pub latest_trade: Option<AlpacaOptionTrade>,
    #[serde(rename(deserialize = "latestQuote"), default)]
    pub latest_quote: Option<AlpacaOptionQuote>,
    #[serde(rename(deserialize = "minuteBar"), default)]
    pub minute_bar: Option<AlpacaBar>,
    #[serde(rename(deserialize = "dailyBar"), default)]
    pub daily_bar: Option<AlpacaBar>,
    #[serde(rename(deserialize = "prevDailyBar"), default)]
    pub previous_daily_bar: Option<AlpacaBar>,
    #[serde(default)]
    pub greeks: Option<AlpacaOptionGreeks>,
    #[serde(rename(deserialize = "impliedVolatility"), default)]
    pub implied_volatility: Option<Decimal>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct AlpacaOptionSnapshotsResponse {
    pub snapshots: BTreeMap<String, AlpacaOptionSnapshot>,
    pub next_page_token: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct AlpacaOptionBarsResponse {
    pub bars: BTreeMap<String, Vec<AlpacaBar>>,
    pub next_page_token: Option<String>,
    #[serde(default)]
    pub currency: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct AlpacaOptionTradesResponse {
    pub trades: BTreeMap<String, Vec<AlpacaOptionTrade>>,
    pub next_page_token: Option<String>,
    #[serde(default)]
    pub currency: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct AlpacaNewsImage {
    pub size: String,
    pub url: String,
}

/// One historical or current Alpaca news article.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct AlpacaNewsArticle {
    pub id: u64,
    pub headline: String,
    pub author: String,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
    pub summary: String,
    #[serde(default)]
    pub content: Option<String>,
    pub url: String,
    #[serde(default)]
    pub images: Vec<AlpacaNewsImage>,
    #[serde(default)]
    pub symbols: Vec<String>,
    pub source: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct AlpacaNewsResponse {
    pub news: Vec<AlpacaNewsArticle>,
    pub next_page_token: Option<String>,
}

/// One page of heterogeneous corporate actions, grouped by Alpaca event type.
#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct AlpacaCorporateActionsResponse {
    pub next_page_token: Option<String>,
    #[serde(flatten)]
    pub actions: BTreeMap<String, Vec<serde_json::Value>>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct AlpacaSingleSymbolBarsResponse {
    pub bars: Vec<AlpacaBar>,
    pub symbol: String,
    pub next_page_token: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct AlpacaAccount {
    pub id: String,
    pub account_number: String,
    pub status: String,
    pub currency: String,
    pub cash: Decimal,
    pub buying_power: Decimal,
    pub equity: Decimal,
    pub portfolio_value: Decimal,
    pub long_market_value: Decimal,
    pub short_market_value: Decimal,
    /// Deprecated by Alpaca and removed from current account responses in July 2026.
    #[serde(default)]
    pub pattern_day_trader: Option<bool>,
    pub trading_blocked: bool,
    pub transfers_blocked: bool,
    pub account_blocked: bool,
    pub trade_suspended_by_user: bool,
    pub shorting_enabled: bool,
    pub multiplier: Decimal,
    pub created_at: Timestamp,
    #[serde(default)]
    pub regt_buying_power: Option<Decimal>,
    #[serde(default)]
    pub non_marginable_buying_power: Option<Decimal>,
    #[serde(default)]
    pub initial_margin: Option<Decimal>,
    #[serde(default)]
    pub maintenance_margin: Option<Decimal>,
    #[serde(default)]
    pub last_equity: Option<Decimal>,
    #[serde(default)]
    pub last_maintenance_margin: Option<Decimal>,
    #[serde(default)]
    pub sma: Option<Decimal>,
    #[serde(default)]
    pub accrued_fees: Option<Decimal>,
    #[serde(default)]
    pub pending_transfer_in: Option<Decimal>,
    #[serde(default)]
    pub options_buying_power: Option<Decimal>,
    #[serde(default)]
    pub options_approved_level: Option<u8>,
    #[serde(default)]
    pub options_trading_level: Option<u8>,
    #[serde(default)]
    pub crypto_status: Option<String>,
}

/// Current US equity market clock returned by Alpaca's trading API.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct AlpacaMarketClock {
    pub timestamp: Timestamp,
    pub is_open: bool,
    pub next_open: Timestamp,
    pub next_close: Timestamp,
}

/// One US equity trading session, including shortened holiday sessions.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct AlpacaCalendarDay {
    pub date: String,
    pub open: String,
    pub close: String,
    #[serde(default)]
    pub session_open: Option<String>,
    #[serde(default)]
    pub session_close: Option<String>,
    #[serde(default)]
    pub settlement_date: Option<String>,
}

/// Account equity and profit/loss series returned by Alpaca.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct AlpacaPortfolioHistory {
    pub timestamp: Vec<i64>,
    pub equity: Vec<Option<Decimal>>,
    pub profit_loss: Vec<Option<Decimal>>,
    pub profit_loss_pct: Vec<Option<Decimal>>,
    pub base_value: Decimal,
    #[serde(default)]
    pub base_value_asof: Option<String>,
    pub timeframe: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AlpacaOrderSide {
    Buy,
    Sell,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AlpacaOrderType {
    Market,
    Limit,
    Stop,
    StopLimit,
    TrailingStop,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AlpacaTimeInForce {
    Day,
    Gtc,
    Opg,
    Cls,
    Ioc,
    Fok,
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AlpacaOrderClass {
    Bracket,
    Oco,
    Oto,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct AlpacaTakeProfitRequest {
    pub limit_price: Decimal,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct AlpacaStopLossRequest {
    pub stop_price: Decimal,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit_price: Option<Decimal>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AlpacaOrderStatus {
    New,
    PartiallyFilled,
    Filled,
    DoneForDay,
    Canceled,
    Expired,
    Replaced,
    PendingCancel,
    PendingReplace,
    PendingNew,
    Accepted,
    AcceptedForBidding,
    Stopped,
    Rejected,
    Suspended,
    Calculated,
    Held,
    #[serde(other)]
    Unknown,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct AlpacaOrderRequest {
    pub symbol: String,
    pub qty: Decimal,
    pub side: AlpacaOrderSide,
    #[serde(rename = "type")]
    pub order_type: AlpacaOrderType,
    pub time_in_force: AlpacaTimeInForce,
    pub client_order_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit_price: Option<Decimal>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_price: Option<Decimal>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trail_price: Option<Decimal>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trail_percent: Option<Decimal>,
    pub extended_hours: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub order_class: Option<AlpacaOrderClass>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub take_profit: Option<AlpacaTakeProfitRequest>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_loss: Option<AlpacaStopLossRequest>,
}

/// Mutable fields accepted by Alpaca's replace-order endpoint.
#[derive(Clone, Debug, Default, Serialize, PartialEq)]
pub struct AlpacaReplaceOrderRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub qty: Option<Decimal>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit_price: Option<Decimal>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_price: Option<Decimal>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct AlpacaOrder {
    pub id: String,
    pub client_order_id: String,
    pub symbol: String,
    pub asset_class: AlpacaAssetClass,
    pub qty: Decimal,
    pub filled_qty: Decimal,
    pub filled_avg_price: Option<Decimal>,
    pub side: AlpacaOrderSide,
    #[serde(rename = "type")]
    pub order_type: AlpacaOrderType,
    pub time_in_force: AlpacaTimeInForce,
    pub limit_price: Option<Decimal>,
    pub stop_price: Option<Decimal>,
    pub status: AlpacaOrderStatus,
    pub extended_hours: bool,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
    pub submitted_at: Option<Timestamp>,
    pub filled_at: Option<Timestamp>,
    pub canceled_at: Option<Timestamp>,
    pub expired_at: Option<Timestamp>,
    pub failed_at: Option<Timestamp>,
    #[serde(default)]
    pub legs: Option<Vec<Self>>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct AlpacaPosition {
    pub asset_id: String,
    pub symbol: String,
    pub asset_class: AlpacaAssetClass,
    pub qty: Decimal,
    pub side: String,
    pub avg_entry_price: Decimal,
    pub market_value: Decimal,
    pub cost_basis: Decimal,
    pub unrealized_pl: Decimal,
    pub unrealized_plpc: Decimal,
    pub current_price: Decimal,
    pub lastday_price: Decimal,
    pub change_today: Decimal,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct AlpacaTradeActivity {
    pub activity_type: String,
    pub id: String,
    pub order_id: String,
    pub symbol: String,
    pub side: AlpacaOrderSide,
    pub qty: Decimal,
    pub price: Decimal,
    pub cum_qty: Decimal,
    pub leaves_qty: Decimal,
    #[serde(rename(deserialize = "type"))]
    pub fill_type: String,
    pub transaction_time: Timestamp,
}

/// A cash-impacting, non-trade account activity such as a regulatory fee or dividend.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct AlpacaNonTradeActivity {
    pub activity_type: String,
    #[serde(default, alias = "activity_subtype")]
    pub activity_sub_type: Option<String>,
    pub id: String,
    pub date: String,
    pub net_amount: Decimal,
    #[serde(default)]
    pub symbol: Option<String>,
    #[serde(default)]
    pub qty: Option<Decimal>,
    #[serde(default)]
    pub per_share_amount: Option<Decimal>,
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use rstest::rstest;

    use super::*;

    #[rstest]
    fn test_deserialize_official_fractionable_equity_asset() {
        let payload = r#"{
            "id": "b0b6dd9d-8b9b-48a9-ba46-b9d54906e415",
            "class": "us_equity",
            "exchange": "NASDAQ",
            "symbol": "AAPL",
            "name": "Apple Inc. Common Stock",
            "status": "active",
            "tradable": true,
            "marginable": true,
            "shortable": true,
            "borrow_status": "easy_to_borrow",
            "fractionable": true,
            "cusip": "037833100",
            "margin_requirement_long": "30.000000001",
            "margin_requirement_short": "35.000000001"
        }"#;

        let asset: AlpacaAsset = serde_json::from_str(payload).unwrap();

        assert_eq!(asset.asset_class, AlpacaAssetClass::UsEquity);
        assert_eq!(asset.status, AlpacaAssetStatus::Active);
        assert_eq!(asset.symbol, "AAPL");
        assert!(asset.fractionable);
        assert_eq!(asset.borrow_status.as_deref(), Some("easy_to_borrow"));
        assert_eq!(asset.easy_to_borrow, None);
        assert_eq!(asset.cusip.as_deref(), Some("037833100"));
        assert_eq!(
            asset.margin_requirement_long,
            Some(Decimal::from_str("30.000000001").unwrap())
        );
        assert_eq!(
            asset.margin_requirement_short,
            Some(Decimal::from_str("35.000000001").unwrap())
        );
        assert_eq!(asset.price_increment, None);
    }

    #[rstest]
    fn test_deserialize_legacy_equity_borrow_and_margin_fields() {
        let payload = r#"{
            "id": "asset-id",
            "class": "us_equity",
            "exchange": "NASDAQ",
            "symbol": "AAPL",
            "name": "Apple Inc.",
            "status": "active",
            "tradable": true,
            "marginable": true,
            "shortable": true,
            "easy_to_borrow": true,
            "fractionable": true,
            "maintenance_margin_requirement": "30"
        }"#;

        let asset: AlpacaAsset = serde_json::from_str(payload).unwrap();

        assert_eq!(asset.borrow_status, None);
        assert_eq!(asset.easy_to_borrow, Some(true));
        assert_eq!(
            asset.maintenance_margin_requirement,
            Some(Decimal::from(30))
        );
    }

    #[rstest]
    fn test_deserialize_option_contract_preserves_terms_and_deliverables() {
        let payload = r#"{
            "id": "contract-id",
            "symbol": "AAPL260116C00200000",
            "name": "AAPL Jan 16 2026 200 Call",
            "status": "active",
            "tradable": true,
            "expiration_date": "2026-01-16",
            "underlying_symbol": "AAPL",
            "underlying_asset_id": "asset-id",
            "type": "call",
            "style": "american",
            "strike_price": "200.000000001",
            "multiplier": "100",
            "size": "100",
            "deliverables": [{
                "type": "equity",
                "symbol": "AAPL",
                "asset_id": "asset-id",
                "amount": "100",
                "allocation_percentage": "100",
                "settlement_type": "T+1",
                "settlement_method": "CCC",
                "delayed_settlement": false
            }]
        }"#;

        let contract: AlpacaOptionContract = serde_json::from_str(payload).unwrap();

        assert_eq!(contract.option_type, "call");
        assert_eq!(
            contract.strike_price,
            Decimal::from_str("200.000000001").unwrap()
        );
        assert_eq!(contract.deliverables[0].deliverable_type, "equity");
        assert_eq!(contract.deliverables[0].amount, Some(Decimal::from(100)));
    }

    #[rstest]
    fn test_deserialize_option_snapshot_preserves_iv_and_greeks() {
        let payload = r#"{
            "snapshots": {
                "AAPL260116C00200000": {
                    "greeks": {
                        "delta": 0.7521304109871954,
                        "gamma": 0.06241426404871288,
                        "rho": 0.009910739032549095,
                        "theta": -0.2847623059595503,
                        "vega": 0.047540520834498785
                    },
                    "impliedVolatility": 0.3372405712050441
                }
            },
            "next_page_token": null
        }"#;

        let response: AlpacaOptionSnapshotsResponse = serde_json::from_str(payload).unwrap();
        let snapshot = &response.snapshots["AAPL260116C00200000"];

        assert_eq!(
            snapshot.implied_volatility,
            Some(Decimal::from_str("0.3372405712050441").unwrap())
        );
        assert_eq!(
            snapshot.greeks.as_ref().unwrap().delta,
            Decimal::from_str("0.7521304109871954").unwrap()
        );
    }

    #[rstest]
    fn test_deserialize_official_iex_daily_bar_without_float_rounding() {
        // Source: https://docs.alpaca.markets/docs/market-data-faq
        let payload = r#"{
            "bars": [{
                "t": "2023-09-29T04:00:00Z",
                "o": 172.015,
                "h": 173.06,
                "l": 170.36,
                "c": 171.29,
                "v": 923134,
                "n": 12630,
                "vw": 171.716432
            }],
            "symbol": "AAPL",
            "next_page_token": null
        }"#;

        let response: AlpacaSingleSymbolBarsResponse = serde_json::from_str(payload).unwrap();
        let bar = &response.bars[0];

        assert_eq!(bar.open, Decimal::from_str("172.015").unwrap());
        assert_eq!(bar.high, Decimal::from_str("173.06").unwrap());
        assert_eq!(
            bar.volume_weighted_price,
            Decimal::from_str("171.716432").unwrap()
        );
        assert_eq!(bar.volume, Decimal::from(923_134));
        assert_eq!(bar.trade_count, 12_630);
        assert_eq!(response.symbol, "AAPL");
        assert_eq!(response.next_page_token, None);
    }

    #[rstest]
    fn test_order_request_uses_alpaca_wire_names_and_decimal_strings() {
        let request = AlpacaOrderRequest {
            symbol: "AAPL".to_string(),
            qty: Decimal::from_str("2.5").unwrap(),
            side: AlpacaOrderSide::Buy,
            order_type: AlpacaOrderType::StopLimit,
            time_in_force: AlpacaTimeInForce::Day,
            client_order_id: "strategy-001".to_string(),
            limit_price: Some(Decimal::from_str("180.25").unwrap()),
            stop_price: Some(Decimal::from_str("180.00").unwrap()),
            trail_price: None,
            trail_percent: None,
            extended_hours: false,
            order_class: None,
            take_profit: None,
            stop_loss: None,
        };

        let value = serde_json::to_value(request).unwrap();
        assert_eq!(value["type"], "stop_limit");
        assert_eq!(value["side"], "buy");
        assert_eq!(value["time_in_force"], "day");
        assert_eq!(value["qty"], "2.5");
        assert_eq!(value["limit_price"], "180.25");
        assert!(value.get("trail_price").is_none());
    }

    #[rstest]
    fn test_replace_order_request_only_serializes_changed_fields() {
        let request = AlpacaReplaceOrderRequest {
            qty: Some(Decimal::from_str("2.5").unwrap()),
            limit_price: None,
            stop_price: Some(Decimal::from_str("179.50").unwrap()),
        };

        let value = serde_json::to_value(request).unwrap();
        assert_eq!(value["qty"], "2.5");
        assert_eq!(value["stop_price"], "179.50");
        assert!(value.get("limit_price").is_none());
    }

    #[rstest]
    fn test_order_status_accepts_new_held_value() {
        let status: AlpacaOrderStatus = serde_json::from_str(r#""held""#).unwrap();
        assert_eq!(status, AlpacaOrderStatus::Held);
    }

    #[rstest]
    fn test_deserialize_position_and_fill_activity_without_float_loss() {
        let position: AlpacaPosition = serde_json::from_str(
            r#"{"asset_id":"asset-1","symbol":"AAPL","asset_class":"us_equity","qty":"-2","side":"short","avg_entry_price":"187.125","market_value":"-374.25","cost_basis":"-374.25","unrealized_pl":"0","unrealized_plpc":"0","current_price":"187.125","lastday_price":"186.50","change_today":"0.003351"}"#,
        )
        .unwrap();
        let fill: AlpacaTradeActivity = serde_json::from_str(
            r#"{"activity_type":"FILL","id":"20190524113406977::fill-1","order_id":"order-1","symbol":"AAPL","side":"sell","qty":"0.25","price":"187.125","cum_qty":"0.25","leaves_qty":"1.75","type":"partial_fill","transaction_time":"2024-01-01T00:00:01Z"}"#,
        )
        .unwrap();

        assert_eq!(position.qty, Decimal::from(-2));
        assert_eq!(
            position.avg_entry_price,
            Decimal::from_str("187.125").unwrap()
        );
        assert_eq!(fill.qty, Decimal::from_str("0.25").unwrap());
        assert_eq!(fill.price, Decimal::from_str("187.125").unwrap());
    }

    #[rstest]
    fn test_deserialize_regulatory_fee_activity_without_false_fill_attribution() {
        let fee: AlpacaNonTradeActivity = serde_json::from_str(
            r#"{"activity_type":"FEE","activity_sub_type":"REG","id":"fee-1","date":"2026-01-16","net_amount":"-0.0137","symbol":"AAPL","qty":"25"}"#,
        )
        .unwrap();

        assert_eq!(fee.activity_sub_type.as_deref(), Some("REG"));
        assert_eq!(fee.net_amount, Decimal::from_str("-0.0137").unwrap());
        assert_eq!(fee.qty, Some(Decimal::from(25)));
    }
}
