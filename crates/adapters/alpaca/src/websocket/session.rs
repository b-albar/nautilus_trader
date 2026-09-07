// -------------------------------------------------------------------------------------------------
//  Copyright (C) 2015-2026 Nautech Systems Pty Ltd. All rights reserved.
//  https://nautechsystems.io
//
//  Licensed under the GNU Lesser General Public License Version 3.0 (the "License");
// -------------------------------------------------------------------------------------------------

use std::collections::HashSet;

use nautilus_network::{
    RECONNECTED,
    websocket::{AuthTracker, SubscriptionState, auth::AuthResultReceiver},
};

use super::messages::{AlpacaMarketDataMessage, AlpacaSubscriptionState, decode_market_data_frame};

const TOPIC_DELIMITER: char = '|';

#[derive(Clone, Debug, PartialEq)]
pub enum AlpacaSessionEvent {
    Authenticated,
    SubscriptionUpdated(AlpacaSubscriptionState),
    MarketData(Vec<AlpacaMarketDataMessage>),
    VenueError { code: u32, message: String },
    ProtocolWarning(String),
    Reconnected { topics_to_replay: Vec<String> },
}

/// Protocol state shared by the socket handler and reconnect controller.
#[derive(Clone, Debug)]
pub struct AlpacaSessionState {
    auth: AuthTracker,
    subscriptions: SubscriptionState,
}

impl Default for AlpacaSessionState {
    fn default() -> Self {
        Self::new()
    }
}

impl AlpacaSessionState {
    #[must_use]
    pub fn new() -> Self {
        Self {
            auth: AuthTracker::new(),
            subscriptions: SubscriptionState::new(TOPIC_DELIMITER),
        }
    }

    pub fn begin_authentication(&self) -> AuthResultReceiver {
        self.auth.begin()
    }

    #[must_use]
    pub fn is_authenticated(&self) -> bool {
        self.auth.is_authenticated()
    }

    #[must_use]
    pub fn auth_tracker(&self) -> AuthTracker {
        self.auth.clone()
    }

    #[must_use]
    pub fn subscriptions(&self) -> &SubscriptionState {
        &self.subscriptions
    }

    pub fn mark_subscribe(&self, channel: &str, symbols: &[String]) {
        for symbol in symbols {
            self.subscriptions
                .mark_subscribe(&format!("{channel}{TOPIC_DELIMITER}{symbol}"));
        }
    }

    pub fn mark_unsubscribe(&self, channel: &str, symbols: &[String]) {
        for symbol in symbols {
            self.subscriptions
                .mark_unsubscribe(&format!("{channel}{TOPIC_DELIMITER}{symbol}"));
        }
    }

    /// Applies a complete Alpaca frame to authentication and subscription state.
    ///
    /// # Errors
    ///
    /// Returns a decoding error if the frame is not a valid Alpaca JSON message array.
    pub fn on_frame(&self, payload: &[u8]) -> Result<Vec<AlpacaSessionEvent>, serde_json::Error> {
        if payload == RECONNECTED.as_bytes() {
            self.auth.invalidate();
            let topics_to_replay = self.subscriptions.reset_after_reconnect();
            return Ok(vec![AlpacaSessionEvent::Reconnected { topics_to_replay }]);
        }

        let messages = decode_market_data_frame(payload)?;
        let mut events = Vec::new();
        let mut market_data = Vec::new();

        for message in messages {
            match message {
                AlpacaMarketDataMessage::Success { ref msg } if msg == "authenticated" => {
                    flush_market_data(&mut events, &mut market_data);
                    self.auth.succeed();
                    events.push(AlpacaSessionEvent::Authenticated);
                }
                AlpacaMarketDataMessage::Subscription(state) => {
                    flush_market_data(&mut events, &mut market_data);
                    self.reconcile_subscriptions(&state);
                    events.push(AlpacaSessionEvent::SubscriptionUpdated(state));
                }
                AlpacaMarketDataMessage::Error { code, msg } => {
                    flush_market_data(&mut events, &mut market_data);
                    if !self.auth.is_authenticated() {
                        self.auth.fail(format!("Alpaca error {code}: {msg}"));
                    }
                    events.push(AlpacaSessionEvent::VenueError { code, message: msg });
                }
                AlpacaMarketDataMessage::Trade(_)
                | AlpacaMarketDataMessage::Quote(_)
                | AlpacaMarketDataMessage::MinuteBar(_)
                | AlpacaMarketDataMessage::DailyBar(_)
                | AlpacaMarketDataMessage::UpdatedBar(_)
                | AlpacaMarketDataMessage::TradeCorrection(_)
                | AlpacaMarketDataMessage::TradeCancelError(_)
                | AlpacaMarketDataMessage::TradingStatus(_)
                | AlpacaMarketDataMessage::Luld(_) => market_data.push(message),
                AlpacaMarketDataMessage::Success { .. } => {
                    flush_market_data(&mut events, &mut market_data);
                }
                AlpacaMarketDataMessage::Unknown => {
                    flush_market_data(&mut events, &mut market_data);
                    events.push(AlpacaSessionEvent::ProtocolWarning(
                        "Alpaca sent an unsupported market-data message type".to_string(),
                    ));
                }
            }
        }

        flush_market_data(&mut events, &mut market_data);
        Ok(events)
    }

    fn reconcile_subscriptions(&self, state: &AlpacaSubscriptionState) {
        let acknowledged = subscription_topics(state);

        for topic in self.subscriptions.pending_subscribe_topics() {
            if acknowledged.contains(&topic) {
                self.subscriptions.confirm_subscribe(&topic);
            }
        }
        for topic in self.subscriptions.pending_unsubscribe_topics() {
            if !acknowledged.contains(&topic) {
                self.subscriptions.confirm_unsubscribe(&topic);
            }
        }
    }
}

fn flush_market_data(
    events: &mut Vec<AlpacaSessionEvent>,
    market_data: &mut Vec<AlpacaMarketDataMessage>,
) {
    if !market_data.is_empty() {
        events.push(AlpacaSessionEvent::MarketData(std::mem::take(market_data)));
    }
}

fn subscription_topics(state: &AlpacaSubscriptionState) -> HashSet<String> {
    let mut topics = HashSet::new();
    for (channel, symbols) in [
        ("trades", &state.trades),
        ("quotes", &state.quotes),
        ("bars", &state.bars),
        ("dailyBars", &state.daily_bars),
        ("updatedBars", &state.updated_bars),
        ("statuses", &state.statuses),
        ("lulds", &state.lulds),
    ] {
        for symbol in symbols {
            topics.insert(format!("{channel}{TOPIC_DELIMITER}{symbol}"));
        }
    }
    topics
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    #[rstest]
    fn test_authentication_acknowledgement_resolves_attempt() {
        let state = AlpacaSessionState::new();
        let mut result = state.begin_authentication();

        let events = state
            .on_frame(br#"[{"T":"success","msg":"authenticated"}]"#)
            .unwrap();

        assert_eq!(events, vec![AlpacaSessionEvent::Authenticated]);
        assert!(state.is_authenticated());
        assert_eq!(result.try_recv().unwrap(), Ok(()));
    }

    #[rstest]
    fn test_complete_subscription_ack_reconciles_pending_state() {
        let state = AlpacaSessionState::new();
        state.mark_subscribe("trades", &["AAPL".to_string()]);
        state.mark_subscribe("quotes", &["AMD".to_string()]);
        state.mark_subscribe("statuses", &["MSFT".to_string()]);

        state
            .on_frame(
                br#"[{"T":"subscription","trades":["AAPL"],"quotes":["AMD"],"bars":[],"updatedBars":[],"dailyBars":[],"statuses":["MSFT"],"lulds":[]}]"#,
            )
            .unwrap();

        assert_eq!(state.subscriptions().len(), 3);
        assert!(state.subscriptions().pending_subscribe_topics().is_empty());
    }

    #[rstest]
    fn test_reconnect_invalidates_auth_and_returns_subscription_intent() {
        let state = AlpacaSessionState::new();
        let _receiver = state.begin_authentication();
        state
            .on_frame(br#"[{"T":"success","msg":"authenticated"}]"#)
            .unwrap();
        state.mark_subscribe("trades", &["AAPL".to_string()]);
        state
            .on_frame(br#"[{"T":"subscription","trades":["AAPL"]}]"#)
            .unwrap();

        let events = state.on_frame(RECONNECTED.as_bytes()).unwrap();

        assert!(!state.is_authenticated());
        assert_eq!(
            events,
            vec![AlpacaSessionEvent::Reconnected {
                topics_to_replay: vec!["trades|AAPL".to_string()]
            }]
        );
        assert_eq!(
            state.subscriptions().pending_subscribe_topics(),
            vec!["trades|AAPL".to_string()]
        );
    }

    #[rstest]
    fn test_authentication_error_resolves_attempt_with_failure() {
        let state = AlpacaSessionState::new();
        let mut result = state.begin_authentication();

        let events = state
            .on_frame(br#"[{"T":"error","code":402,"msg":"auth failed"}]"#)
            .unwrap();

        assert!(matches!(
            &events[0],
            AlpacaSessionEvent::VenueError { code: 402, message } if message == "auth failed"
        ));
        assert_eq!(
            result.try_recv().unwrap(),
            Err("Alpaca error 402: auth failed".to_string())
        );
    }

    #[rstest]
    fn test_mixed_frame_preserves_protocol_event_order() {
        let state = AlpacaSessionState::new();
        let events = state
            .on_frame(br#"[{"T":"t","i":1,"S":"AAPL","x":"D","p":126.55,"s":1,"t":"2021-02-22T15:51:44.208Z","c":[],"z":"C"},{"T":"error","code":405,"msg":"symbol limit exceeded"},{"T":"q","S":"AMD","bx":"U","bp":87.66,"bs":1,"ax":"Q","ap":87.68,"as":4,"t":"2021-02-22T15:51:45Z","c":[],"z":"C"}]"#)
            .unwrap();

        assert!(
            matches!(&events[0], AlpacaSessionEvent::MarketData(messages) if matches!(messages[0], AlpacaMarketDataMessage::Trade(_)))
        );
        assert!(matches!(
            &events[1],
            AlpacaSessionEvent::VenueError { code: 405, .. }
        ));
        assert!(
            matches!(&events[2], AlpacaSessionEvent::MarketData(messages) if matches!(messages[0], AlpacaMarketDataMessage::Quote(_)))
        );
    }

    #[rstest]
    fn test_unknown_market_data_message_is_not_silently_discarded() {
        let state = AlpacaSessionState::new();

        let events = state
            .on_frame(br#"[{"T":"future_type","value":1}]"#)
            .unwrap();

        assert_eq!(
            events,
            vec![AlpacaSessionEvent::ProtocolWarning(
                "Alpaca sent an unsupported market-data message type".to_string()
            )]
        );
    }
}
