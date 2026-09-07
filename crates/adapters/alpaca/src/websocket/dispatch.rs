// -------------------------------------------------------------------------------------------------
//  Copyright (C) 2015-2026 Nautech Systems Pty Ltd. All rights reserved.
//  https://nautechsystems.io
//
//  Licensed under the GNU Lesser General Public License Version 3.0 (the "License");
// -------------------------------------------------------------------------------------------------

use std::collections::HashMap;

use super::{
    messages::{
        AlpacaLuld, AlpacaMarketDataMessage, AlpacaTradeCancelError, AlpacaTradeCorrection,
    },
    parse::{parse_bar, parse_quote, parse_trade},
};
use nautilus_core::UnixNanos;
use nautilus_model::{
    data::{Bar, BarType, InstrumentStatus, QuoteTick, TradeTick},
    enums::MarketStatusAction,
    identifiers::InstrumentId,
};

#[derive(Clone, Debug, PartialEq)]
pub enum AlpacaDataEvent {
    Trade(TradeTick),
    Quote(QuoteTick),
    Bar(Bar),
    /// A correction which requires retracting an earlier trade before applying its replacement.
    TradeCorrection(AlpacaTradeCorrection),
    /// A venue cancellation or error which invalidates a previously published trade.
    TradeCancelError(AlpacaTradeCancelError),
    InstrumentStatus(InstrumentStatus),
    /// LULD has no exact Nautilus core data type, so its full venue semantics are retained here.
    Luld(AlpacaLuld),
}

#[derive(Clone, Copy, Debug)]
struct InstrumentParsingContext {
    instrument_id: InstrumentId,
    price_precision: u8,
    size_precision: u8,
    minute_bar_type: Option<BarType>,
    daily_bar_type: Option<BarType>,
}

/// Converts Alpaca wire messages using explicitly registered instrument precision.
#[derive(Clone, Debug, Default)]
pub struct AlpacaDataDispatcher {
    instruments: HashMap<String, InstrumentParsingContext>,
}

impl AlpacaDataDispatcher {
    pub fn register_instrument(
        &mut self,
        symbol: impl Into<String>,
        instrument_id: InstrumentId,
        price_precision: u8,
        size_precision: u8,
    ) {
        self.instruments.insert(
            symbol.into(),
            InstrumentParsingContext {
                instrument_id,
                price_precision,
                size_precision,
                minute_bar_type: None,
                daily_bar_type: None,
            },
        );
    }

    /// Registers external bar identities requested by the engine.
    ///
    /// # Errors
    ///
    /// Returns an error when the Alpaca symbol has no instrument parsing context.
    pub fn register_bar_types(
        &mut self,
        symbol: &str,
        minute: Option<BarType>,
        daily: Option<BarType>,
    ) -> anyhow::Result<()> {
        let context = self
            .instruments
            .get_mut(symbol)
            .ok_or_else(|| anyhow::anyhow!("No Alpaca instrument registered for {symbol}"))?;
        context.minute_bar_type = minute;
        context.daily_bar_type = daily;
        Ok(())
    }

    /// Converts a single market-data message to a Nautilus domain event.
    ///
    /// # Errors
    ///
    /// Returns an error for missing instrument context, missing bar registration, control
    /// messages, or invalid domain values.
    pub fn dispatch(
        &self,
        message: &AlpacaMarketDataMessage,
        ts_init: UnixNanos,
    ) -> anyhow::Result<AlpacaDataEvent> {
        match message {
            AlpacaMarketDataMessage::Trade(trade) => {
                let context = self.context(&trade.symbol)?;
                Ok(AlpacaDataEvent::Trade(parse_trade(
                    trade,
                    context.instrument_id,
                    context.price_precision,
                    context.size_precision,
                    ts_init,
                )?))
            }
            AlpacaMarketDataMessage::Quote(quote) => {
                let context = self.context(&quote.symbol)?;
                Ok(AlpacaDataEvent::Quote(parse_quote(
                    quote,
                    context.instrument_id,
                    context.price_precision,
                    context.size_precision,
                    ts_init,
                )?))
            }
            AlpacaMarketDataMessage::MinuteBar(bar) | AlpacaMarketDataMessage::UpdatedBar(bar) => {
                let context = self.context(&bar.symbol)?;
                let bar_type = context.minute_bar_type.ok_or_else(|| {
                    anyhow::anyhow!("No Alpaca minute bar type registered for {}", bar.symbol)
                })?;
                Ok(AlpacaDataEvent::Bar(parse_bar(
                    bar,
                    bar_type,
                    context.price_precision,
                    context.size_precision,
                    ts_init,
                )?))
            }
            AlpacaMarketDataMessage::DailyBar(bar) => {
                let context = self.context(&bar.symbol)?;
                let bar_type = context.daily_bar_type.ok_or_else(|| {
                    anyhow::anyhow!("No Alpaca daily bar type registered for {}", bar.symbol)
                })?;
                Ok(AlpacaDataEvent::Bar(parse_bar(
                    bar,
                    bar_type,
                    context.price_precision,
                    context.size_precision,
                    ts_init,
                )?))
            }
            AlpacaMarketDataMessage::TradeCorrection(correction) => {
                self.context(&correction.symbol)?;
                Ok(AlpacaDataEvent::TradeCorrection(correction.clone()))
            }
            AlpacaMarketDataMessage::TradeCancelError(cancel) => {
                self.context(&cancel.symbol)?;
                Ok(AlpacaDataEvent::TradeCancelError(cancel.clone()))
            }
            AlpacaMarketDataMessage::TradingStatus(status) => {
                let context = self.context(&status.symbol)?;
                let (action, is_trading, is_quoting, is_short_sell_restricted) =
                    map_status_code(&status.status_code);
                Ok(AlpacaDataEvent::InstrumentStatus(InstrumentStatus::new(
                    context.instrument_id,
                    action,
                    UnixNanos::from(status.timestamp),
                    ts_init,
                    (!status.reason_message.is_empty())
                        .then(|| status.reason_message.as_str().into()),
                    (!status.status_message.is_empty())
                        .then(|| status.status_message.as_str().into()),
                    is_trading,
                    is_quoting,
                    is_short_sell_restricted,
                )))
            }
            AlpacaMarketDataMessage::Luld(luld) => {
                self.context(&luld.symbol)?;
                Ok(AlpacaDataEvent::Luld(luld.clone()))
            }
            _ => anyhow::bail!("Alpaca message is not market data"),
        }
    }

    fn context(&self, symbol: &str) -> anyhow::Result<&InstrumentParsingContext> {
        self.instruments
            .get(symbol)
            .ok_or_else(|| anyhow::anyhow!("No Alpaca instrument registered for {symbol}"))
    }
}

fn map_status_code(code: &str) -> (MarketStatusAction, Option<bool>, Option<bool>, Option<bool>) {
    match code {
        "2" | "H" => (MarketStatusAction::Halt, Some(false), Some(false), None),
        "P" | "F" => (MarketStatusAction::Pause, Some(false), None, None),
        "3" | "T" => (MarketStatusAction::Trading, Some(true), Some(true), None),
        "Q" => (MarketStatusAction::Quoting, Some(false), Some(true), None),
        "5" | "6" => (MarketStatusAction::NewPriceIndication, None, None, None),
        "E" => (
            MarketStatusAction::ShortSellRestrictionChange,
            None,
            None,
            Some(true),
        ),
        _ => (MarketStatusAction::None, None, None, None),
    }
}

#[cfg(test)]
mod tests {
    use nautilus_model::{
        data::BarType,
        identifiers::InstrumentId,
        types::{Price, Quantity},
    };
    use rstest::rstest;

    use super::*;

    #[rstest]
    fn test_dispatches_official_stream_batch_to_domain_events() {
        let messages: Vec<AlpacaMarketDataMessage> = serde_json::from_str(
            r#"[{"T":"t","i":96921,"S":"AAPL","x":"D","p":126.55,"s":1,"t":"2021-02-22T15:51:44.208Z","c":["@","I"],"z":"C"},{"T":"q","S":"AMD","bx":"U","bp":87.66,"bs":1,"ax":"Q","ap":87.68,"as":4,"t":"2021-02-22T15:51:45.335689322Z","c":["R"],"z":"C"}]"#,
        )
        .unwrap();
        let mut dispatcher = AlpacaDataDispatcher::default();
        dispatcher.register_instrument("AAPL", InstrumentId::from("AAPL.ALPACA"), 2, 0);
        dispatcher.register_instrument("AMD", InstrumentId::from("AMD.ALPACA"), 2, 0);

        let trade = dispatcher
            .dispatch(&messages[0], UnixNanos::from(1))
            .unwrap();
        let quote = dispatcher
            .dispatch(&messages[1], UnixNanos::from(1))
            .unwrap();

        assert!(
            matches!(trade, AlpacaDataEvent::Trade(tick) if tick.price == Price::from("126.55") && tick.size == Quantity::from(1))
        );
        assert!(
            matches!(quote, AlpacaDataEvent::Quote(tick) if tick.bid_size == Quantity::from(100) && tick.ask_size == Quantity::from(400))
        );
    }

    #[rstest]
    fn test_updated_bar_uses_registered_minute_bar_identity() {
        let message: AlpacaMarketDataMessage = serde_json::from_str(
            r#"{"T":"u","S":"SPY","o":388.985,"h":389.13,"l":388.975,"c":389.12,"v":49378,"n":461,"vw":389.062639,"t":"2021-02-22T19:15:00Z"}"#,
        )
        .unwrap();
        let instrument_id = InstrumentId::from("SPY.ALPACA");
        let bar_type = BarType::from("SPY.ALPACA-1-MINUTE-LAST-EXTERNAL");
        let mut dispatcher = AlpacaDataDispatcher::default();
        dispatcher.register_instrument("SPY", instrument_id, 3, 0);
        dispatcher
            .register_bar_types("SPY", Some(bar_type), None)
            .unwrap();

        let event = dispatcher.dispatch(&message, UnixNanos::from(1)).unwrap();

        assert!(
            matches!(event, AlpacaDataEvent::Bar(bar) if bar.bar_type == bar_type && bar.close == Price::from("389.120"))
        );
    }

    #[rstest]
    #[case("H", MarketStatusAction::Halt, Some(false), Some(false))]
    #[case("P", MarketStatusAction::Pause, Some(false), None)]
    #[case("Q", MarketStatusAction::Quoting, Some(false), Some(true))]
    #[case("T", MarketStatusAction::Trading, Some(true), Some(true))]
    fn test_dispatches_trading_status_without_losing_venue_reason(
        #[case] status_code: &str,
        #[case] expected_action: MarketStatusAction,
        #[case] expected_trading: Option<bool>,
        #[case] expected_quoting: Option<bool>,
    ) {
        let payload = format!(
            r#"{{"T":"s","S":"AAPL","sc":"{status_code}","sm":"Trading status","rc":"T12","rm":"Venue reason","t":"2021-02-22T19:15:00Z","z":"C"}}"#,
        );
        let message: AlpacaMarketDataMessage = serde_json::from_str(&payload).unwrap();
        let mut dispatcher = AlpacaDataDispatcher::default();
        dispatcher.register_instrument("AAPL", InstrumentId::from("AAPL.ALPACA"), 4, 9);

        let event = dispatcher.dispatch(&message, UnixNanos::from(7)).unwrap();
        let AlpacaDataEvent::InstrumentStatus(status) = event else {
            panic!("expected instrument status")
        };

        assert_eq!(status.action, expected_action);
        assert_eq!(status.is_trading, expected_trading);
        assert_eq!(status.is_quoting, expected_quoting);
        assert_eq!(status.reason.unwrap().as_str(), "Venue reason");
        assert_eq!(status.trading_event.unwrap().as_str(), "Trading status");
        assert_eq!(status.ts_init, UnixNanos::from(7));
    }

    #[rstest]
    fn test_missing_instrument_is_explicit_error() {
        let message: AlpacaMarketDataMessage = serde_json::from_str(
            r#"{"T":"t","i":1,"S":"UNKNOWN","x":"D","p":1,"s":1,"t":"2021-02-22T15:51:44Z","c":[],"z":"C"}"#,
        )
        .unwrap();

        let error = AlpacaDataDispatcher::default()
            .dispatch(&message, UnixNanos::from(1))
            .unwrap_err();

        assert_eq!(
            error.to_string(),
            "No Alpaca instrument registered for UNKNOWN"
        );
    }

    #[rstest]
    fn test_dispatch_preserves_trade_correction_semantics() {
        let message: AlpacaMarketDataMessage = serde_json::from_str(
            r#"{"T":"c","S":"AAPL","x":"Q","oi":10,"op":190.01,"os":2,"oc":[],"ci":11,"cp":190.02,"cs":3,"cc":[],"t":"2024-01-02T15:04:05Z","z":"C"}"#,
        )
        .unwrap();
        let mut dispatcher = AlpacaDataDispatcher::default();
        dispatcher.register_instrument("AAPL", InstrumentId::from("AAPL.ALPACA"), 2, 0);

        let event = dispatcher.dispatch(&message, UnixNanos::from(1)).unwrap();

        assert!(matches!(
            event,
            AlpacaDataEvent::TradeCorrection(correction)
                if correction.original_trade_id == 10 && correction.corrected_trade_id == 11
        ));
    }
}
