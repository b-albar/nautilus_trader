// -------------------------------------------------------------------------------------------------
//  Copyright (C) 2015-2026 Nautech Systems Pty Ltd. All rights reserved.
//  https://nautechsystems.io
//
//  Licensed under the GNU Lesser General Public License Version 3.0 (the "License");
// -------------------------------------------------------------------------------------------------

use jiff::Timestamp;
use nautilus_core::string::secret::SecretString;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use zeroize::Zeroize;

#[derive(Clone, Serialize, Zeroize)]
pub struct AlpacaWsAuth {
    #[zeroize(skip)]
    pub action: &'static str,
    pub key: SecretString,
    pub secret: SecretString,
}

impl std::fmt::Debug for AlpacaWsAuth {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct(stringify!(AlpacaWsAuth))
            .field("action", &self.action)
            .field("key", &"***")
            .field("secret", &"***")
            .finish()
    }
}

impl AlpacaWsAuth {
    #[must_use]
    pub fn new(key: impl Into<SecretString>, secret: impl Into<SecretString>) -> Self {
        Self {
            action: "auth",
            key: key.into(),
            secret: secret.into(),
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, PartialEq, Eq)]
pub struct AlpacaWsSubscription {
    pub action: AlpacaWsAction,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub trades: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub quotes: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub bars: Vec<String>,
    #[serde(rename = "dailyBars", skip_serializing_if = "Vec::is_empty")]
    pub daily_bars: Vec<String>,
    #[serde(rename = "updatedBars", skip_serializing_if = "Vec::is_empty")]
    pub updated_bars: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub statuses: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub lulds: Vec<String>,
}

#[derive(Clone, Copy, Debug, Default, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum AlpacaWsAction {
    #[default]
    Subscribe,
    Unsubscribe,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(tag = "T")]
pub enum AlpacaMarketDataMessage {
    #[serde(rename = "success")]
    Success { msg: String },
    #[serde(rename = "error")]
    Error { code: u32, msg: String },
    #[serde(rename = "subscription")]
    Subscription(AlpacaSubscriptionState),
    #[serde(rename = "t")]
    Trade(AlpacaTrade),
    #[serde(rename = "q")]
    Quote(AlpacaQuote),
    #[serde(rename = "b")]
    MinuteBar(AlpacaStreamBar),
    #[serde(rename = "d")]
    DailyBar(AlpacaStreamBar),
    #[serde(rename = "u")]
    UpdatedBar(AlpacaStreamBar),
    /// A correction to a trade previously published on the stream.
    #[serde(rename = "c")]
    TradeCorrection(AlpacaTradeCorrection),
    /// A trade cancellation or trade error for a previously published trade.
    #[serde(rename = "x")]
    TradeCancelError(AlpacaTradeCancelError),
    /// A change to a security's trading status, including halts and resumptions.
    #[serde(rename = "s")]
    TradingStatus(AlpacaTradingStatus),
    /// Limit Up-Limit Down price bands for a security.
    #[serde(rename = "l")]
    Luld(AlpacaLuld),
    #[serde(other)]
    Unknown,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
pub struct AlpacaSubscriptionState {
    #[serde(default)]
    pub trades: Vec<String>,
    #[serde(default)]
    pub quotes: Vec<String>,
    #[serde(default)]
    pub bars: Vec<String>,
    #[serde(rename = "dailyBars", default)]
    pub daily_bars: Vec<String>,
    #[serde(rename = "updatedBars", default)]
    pub updated_bars: Vec<String>,
    #[serde(default)]
    pub statuses: Vec<String>,
    #[serde(default)]
    pub lulds: Vec<String>,
    #[serde(default)]
    pub corrections: Vec<String>,
    #[serde(rename = "cancelErrors", default)]
    pub cancel_errors: Vec<String>,
}

pub fn decode_market_data_frame(
    payload: &[u8],
) -> Result<Vec<AlpacaMarketDataMessage>, serde_json::Error> {
    serde_json::from_slice(payload)
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct AlpacaTrade {
    #[serde(rename = "S")]
    pub symbol: String,
    #[serde(rename = "i")]
    pub trade_id: u64,
    #[serde(rename = "x")]
    pub exchange: String,
    #[serde(rename = "p")]
    pub price: Decimal,
    #[serde(rename = "s")]
    pub size: Decimal,
    #[serde(rename = "c", default)]
    pub conditions: Vec<String>,
    #[serde(rename = "t")]
    pub timestamp: Timestamp,
    #[serde(rename = "z")]
    pub tape: String,
}

/// Alpaca's wire representation of a corrected trade.
///
/// Both the original and corrected values are retained because a downstream consumer must retract
/// the original event before applying the replacement. Converting this record directly into a
/// [`TradeTick`](nautilus_model::data::TradeTick) would lose that semantic.
#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct AlpacaTradeCorrection {
    #[serde(rename = "S")]
    pub symbol: String,
    #[serde(rename = "x")]
    pub exchange: String,
    #[serde(rename = "oi")]
    pub original_trade_id: u64,
    #[serde(rename = "op")]
    pub original_price: Decimal,
    #[serde(rename = "os")]
    pub original_size: Decimal,
    #[serde(rename = "oc", default)]
    pub original_conditions: Vec<String>,
    #[serde(rename = "ci")]
    pub corrected_trade_id: u64,
    #[serde(rename = "cp")]
    pub corrected_price: Decimal,
    #[serde(rename = "cs")]
    pub corrected_size: Decimal,
    #[serde(rename = "cc", default)]
    pub corrected_conditions: Vec<String>,
    #[serde(rename = "t")]
    pub timestamp: Timestamp,
    #[serde(rename = "z")]
    pub tape: String,
}

/// Alpaca's wire representation of a trade cancellation or error.
#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct AlpacaTradeCancelError {
    #[serde(rename = "S")]
    pub symbol: String,
    #[serde(rename = "i")]
    pub trade_id: u64,
    #[serde(rename = "x")]
    pub exchange: String,
    #[serde(rename = "p")]
    pub price: Decimal,
    #[serde(rename = "s")]
    pub size: Decimal,
    /// `C` denotes a cancellation and `E` denotes an error.
    #[serde(rename = "a")]
    pub action: String,
    #[serde(rename = "t")]
    pub timestamp: Timestamp,
    #[serde(rename = "z")]
    pub tape: String,
}

/// Alpaca's wire representation of a security trading-status update.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct AlpacaTradingStatus {
    #[serde(rename = "S")]
    pub symbol: String,
    #[serde(rename = "sc")]
    pub status_code: String,
    #[serde(rename = "sm")]
    pub status_message: String,
    #[serde(rename = "rc")]
    pub reason_code: String,
    #[serde(rename = "rm")]
    pub reason_message: String,
    #[serde(rename = "t")]
    pub timestamp: Timestamp,
    #[serde(rename = "z")]
    pub tape: String,
}

/// Alpaca's wire representation of Limit Up-Limit Down price bands.
#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct AlpacaLuld {
    #[serde(rename = "S")]
    pub symbol: String,
    #[serde(rename = "u")]
    pub limit_up_price: Decimal,
    #[serde(rename = "d")]
    pub limit_down_price: Decimal,
    #[serde(rename = "i")]
    pub indicator: String,
    #[serde(rename = "t")]
    pub timestamp: Timestamp,
    #[serde(rename = "z")]
    pub tape: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct AlpacaQuote {
    #[serde(rename = "S")]
    pub symbol: String,
    #[serde(rename = "bx")]
    pub bid_exchange: String,
    #[serde(rename = "bp")]
    pub bid_price: Decimal,
    #[serde(rename = "bs")]
    pub bid_size_lots: Decimal,
    #[serde(rename = "ax")]
    pub ask_exchange: String,
    #[serde(rename = "ap")]
    pub ask_price: Decimal,
    #[serde(rename = "as")]
    pub ask_size_lots: Decimal,
    #[serde(rename = "c", default)]
    pub conditions: Vec<String>,
    #[serde(rename = "t")]
    pub timestamp: Timestamp,
    #[serde(rename = "z")]
    pub tape: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct AlpacaStreamBar {
    #[serde(rename = "S")]
    pub symbol: String,
    #[serde(rename = "o")]
    pub open: Decimal,
    #[serde(rename = "h")]
    pub high: Decimal,
    #[serde(rename = "l")]
    pub low: Decimal,
    #[serde(rename = "c")]
    pub close: Decimal,
    #[serde(rename = "v")]
    pub volume: Decimal,
    #[serde(rename = "n")]
    pub trade_count: u64,
    #[serde(rename = "vw")]
    pub volume_weighted_price: Decimal,
    #[serde(rename = "t")]
    pub timestamp: Timestamp,
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    #[rstest]
    fn test_auth_serializes_exact_wire_shape_and_debug_redacts() {
        let auth = AlpacaWsAuth::new("key-value", "secret-value");

        assert_eq!(
            serde_json::to_string(&auth).unwrap(),
            r#"{"action":"auth","key":"key-value","secret":"secret-value"}"#
        );
        let debug = format!("{auth:?}");
        assert!(!debug.contains("key-value"));
        assert!(!debug.contains("secret-value"));
    }

    #[rstest]
    fn test_subscription_serializes_only_requested_channels() {
        let subscription = AlpacaWsSubscription {
            trades: vec!["AAPL".to_string()],
            quotes: vec!["AMD".to_string(), "CLDR".to_string()],
            bars: vec!["*".to_string()],
            ..Default::default()
        };

        assert_eq!(
            serde_json::to_string(&subscription).unwrap(),
            r#"{"action":"subscribe","trades":["AAPL"],"quotes":["AMD","CLDR"],"bars":["*"]}"#
        );
    }

    #[rstest]
    fn test_decode_batched_control_and_data_messages() {
        let payload = br#"[{"T":"success","msg":"authenticated"},{"T":"t","i":96921,"S":"AAPL","x":"D","p":126.55,"s":1,"t":"2021-02-22T15:51:44.208Z","c":["@","I"],"z":"C"}]"#;
        let messages = decode_market_data_frame(payload).unwrap();

        assert_eq!(messages.len(), 2);
        assert!(
            matches!(&messages[0], AlpacaMarketDataMessage::Success { msg } if msg == "authenticated")
        );
        assert!(
            matches!(&messages[1], AlpacaMarketDataMessage::Trade(trade) if trade.symbol == "AAPL")
        );
    }

    #[rstest]
    fn test_decode_trade_correction_and_cancel_error() {
        let payload = br#"[{"T":"c","S":"AAPL","x":"Q","oi":10,"op":190.01,"os":2,"oc":["@"],"ci":11,"cp":190.02,"cs":3,"cc":["I"],"t":"2024-01-02T15:04:05.123456789Z","z":"C"},{"T":"x","S":"AAPL","i":11,"x":"Q","p":190.02,"s":3,"a":"C","t":"2024-01-02T15:04:06Z","z":"C"}]"#;

        let messages = decode_market_data_frame(payload).unwrap();

        assert!(matches!(
            &messages[0],
            AlpacaMarketDataMessage::TradeCorrection(correction)
                if correction.original_trade_id == 10
                    && correction.corrected_trade_id == 11
                    && correction.corrected_price == Decimal::new(19_002, 2)
        ));
        assert!(matches!(
            &messages[1],
            AlpacaMarketDataMessage::TradeCancelError(cancel)
                if cancel.trade_id == 11 && cancel.action == "C"
        ));
    }

    #[rstest]
    fn test_decode_official_trading_status_and_luld_messages() {
        let messages = decode_market_data_frame(
            br#"[{"T":"s","S":"AAPL","sc":"H","sm":"Trading Halt","rc":"T12","rm":"Trading Halted; For information requested by NASDAQ","t":"2021-02-22T19:15:00Z","z":"C"},{"T":"l","S":"IONM","u":3.24,"d":2.65,"i":"B","t":"2023-04-06T13:34:45.565004401Z","z":"C"}]"#,
        )
        .unwrap();

        assert!(matches!(
            &messages[0],
            AlpacaMarketDataMessage::TradingStatus(status)
                if status.symbol == "AAPL" && status.status_code == "H" && status.reason_code == "T12"
        ));
        assert!(matches!(
            &messages[1],
            AlpacaMarketDataMessage::Luld(luld)
                if luld.symbol == "IONM"
                    && luld.limit_up_price == Decimal::new(324, 2)
                    && luld.limit_down_price == Decimal::new(265, 2)
        ));
    }
}
