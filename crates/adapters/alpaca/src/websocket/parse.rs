// -------------------------------------------------------------------------------------------------
//  Copyright (C) 2015-2026 Nautech Systems Pty Ltd. All rights reserved.
//  https://nautechsystems.io
//
//  Licensed under the GNU Lesser General Public License Version 3.0 (the "License");
// -------------------------------------------------------------------------------------------------

use nautilus_core::UnixNanos;
use nautilus_model::{
    data::{Bar, BarType, QuoteTick, TradeTick},
    enums::AggressorSide,
    identifiers::{InstrumentId, TradeId},
    types::{Price, Quantity},
};
use rust_decimal::Decimal;

use super::messages::{AlpacaQuote, AlpacaStreamBar, AlpacaTrade};

const US_EQUITY_ROUND_LOT_SIZE: u32 = 100;

pub fn parse_trade(
    message: &AlpacaTrade,
    instrument_id: InstrumentId,
    price_precision: u8,
    _size_precision: u8,
    ts_init: UnixNanos,
) -> anyhow::Result<TradeTick> {
    TradeTick::new_checked(
        instrument_id,
        Price::from_decimal_dp(message.price, price_precision)?,
        Quantity::from_decimal(message.size)?,
        AggressorSide::NoAggressor,
        TradeId::new(message.trade_id.to_string()),
        UnixNanos::from(message.timestamp),
        ts_init,
    )
}

pub fn parse_quote(
    message: &AlpacaQuote,
    instrument_id: InstrumentId,
    price_precision: u8,
    _size_precision: u8,
    ts_init: UnixNanos,
) -> anyhow::Result<QuoteTick> {
    let lot_size = Decimal::from(US_EQUITY_ROUND_LOT_SIZE);

    QuoteTick::new_checked(
        instrument_id,
        Price::from_decimal_dp(message.bid_price, price_precision)?,
        Price::from_decimal_dp(message.ask_price, price_precision)?,
        Quantity::from_decimal(message.bid_size_lots * lot_size)?,
        Quantity::from_decimal(message.ask_size_lots * lot_size)?,
        UnixNanos::from(message.timestamp),
        ts_init,
    )
}

pub fn parse_bar(
    message: &AlpacaStreamBar,
    bar_type: BarType,
    price_precision: u8,
    _size_precision: u8,
    ts_init: UnixNanos,
) -> anyhow::Result<Bar> {
    Bar::new_checked(
        bar_type,
        Price::from_decimal_dp(message.open, price_precision)?,
        Price::from_decimal_dp(message.high, price_precision)?,
        Price::from_decimal_dp(message.low, price_precision)?,
        Price::from_decimal_dp(message.close, price_precision)?,
        Quantity::from_decimal(message.volume)?,
        UnixNanos::from(message.timestamp),
        ts_init,
    )
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use nautilus_model::identifiers::InstrumentId;
    use rstest::rstest;

    use super::*;
    use crate::websocket::messages::AlpacaMarketDataMessage;

    #[rstest]
    fn test_parse_official_trade_message() {
        let payload = r#"{"T":"t","i":96921,"S":"AAPL","x":"D","p":126.55,"s":1,"t":"2021-02-22T15:51:44.208Z","c":["@","I"],"z":"C"}"#;
        let message: AlpacaMarketDataMessage = serde_json::from_str(payload).unwrap();
        let AlpacaMarketDataMessage::Trade(message) = message else {
            panic!("expected trade")
        };
        let tick = parse_trade(
            &message,
            InstrumentId::from_str("AAPL.ALPACA").unwrap(),
            2,
            0,
            UnixNanos::from(1),
        )
        .unwrap();

        assert_eq!(tick.price, Price::from("126.55"));
        assert_eq!(tick.size, Quantity::from(1));
        assert_eq!(tick.trade_id.to_string(), "96921");
        assert_eq!(tick.aggressor_side, AggressorSide::NoAggressor);
        assert_eq!(tick.ts_event, UnixNanos::from(message.timestamp));
    }

    #[rstest]
    fn test_parse_official_quote_converts_round_lots_to_shares() {
        let payload = r#"{"T":"q","S":"AMD","bx":"U","bp":87.66,"bs":1,"ax":"Q","ap":87.68,"as":4,"t":"2021-02-22T15:51:45.335689322Z","c":["R"],"z":"C"}"#;
        let message: AlpacaMarketDataMessage = serde_json::from_str(payload).unwrap();
        let AlpacaMarketDataMessage::Quote(message) = message else {
            panic!("expected quote")
        };
        let tick = parse_quote(
            &message,
            InstrumentId::from_str("AMD.ALPACA").unwrap(),
            2,
            0,
            UnixNanos::from(1),
        )
        .unwrap();

        assert_eq!(tick.bid_price, Price::from("87.66"));
        assert_eq!(tick.ask_price, Price::from("87.68"));
        assert_eq!(tick.bid_size, Quantity::from(100));
        assert_eq!(tick.ask_size, Quantity::from(400));
        assert_eq!(tick.ts_event, UnixNanos::from(message.timestamp));
    }
}
