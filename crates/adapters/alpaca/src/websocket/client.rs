// -------------------------------------------------------------------------------------------------
//  Copyright (C) 2015-2026 Nautech Systems Pty Ltd. All rights reserved.
//  https://nautechsystems.io
//
//  Licensed under the GNU Lesser General Public License Version 3.0 (the "License");
// -------------------------------------------------------------------------------------------------

use std::{collections::VecDeque, time::Duration};

use nautilus_core::{UnixNanos, string::secret::SecretString};
use nautilus_network::websocket::{
    TransportBackend, WebSocketClient, WebSocketConfig, channel_message_handler,
};
use tokio_tungstenite::tungstenite::Message;

use super::{
    dispatch::{AlpacaDataDispatcher, AlpacaDataEvent},
    messages::{AlpacaWsAuth, AlpacaWsSubscription},
    session::{AlpacaSessionEvent, AlpacaSessionState},
};
use crate::common::credential::AlpacaCredential;

const AUTHENTICATION_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Clone, Debug, PartialEq)]
pub enum AlpacaLiveMessage {
    Authenticated,
    Data(AlpacaDataEvent),
    SubscriptionUpdated(super::messages::AlpacaSubscriptionState),
    VenueError { code: u32, message: String },
    Reconnected,
    Error(String),
}

/// Reconnecting Alpaca market-data WebSocket client using Nautilus's shared transport.
#[derive(Debug)]
pub struct AlpacaWebSocketClient {
    url: String,
    credential: AlpacaCredential,
    backend: TransportBackend,
    proxy_url: Option<SecretString>,
    socket: Option<WebSocketClient>,
    raw_rx: Option<tokio::sync::mpsc::UnboundedReceiver<Message>>,
    session: AlpacaSessionState,
    dispatcher: AlpacaDataDispatcher,
    output: VecDeque<AlpacaLiveMessage>,
}

impl AlpacaWebSocketClient {
    #[must_use]
    pub fn new(
        url: impl Into<String>,
        credential: AlpacaCredential,
        backend: TransportBackend,
        proxy_url: Option<String>,
    ) -> Self {
        Self {
            url: url.into(),
            credential,
            backend,
            proxy_url: proxy_url.map(SecretString::from),
            socket: None,
            raw_rx: None,
            session: AlpacaSessionState::new(),
            dispatcher: AlpacaDataDispatcher::default(),
            output: VecDeque::new(),
        }
    }

    #[must_use]
    pub fn dispatcher_mut(&mut self) -> &mut AlpacaDataDispatcher {
        &mut self.dispatcher
    }

    #[must_use]
    pub fn is_authenticated(&self) -> bool {
        self.session.is_authenticated()
    }

    /// Connects the shared transport and completes Alpaca authentication.
    ///
    /// # Errors
    ///
    /// Returns an error for transport creation, serialization, send, authentication rejection,
    /// timeout, or premature socket closure.
    pub async fn connect(&mut self) -> anyhow::Result<()> {
        if self.socket.is_some() {
            anyhow::bail!("Alpaca WebSocket client is already connected");
        }

        let (handler, raw_rx) = channel_message_handler();
        let config = WebSocketConfig {
            url: self.url.clone(),
            headers: Vec::new(),
            heartbeat_interval_secs: Some(20),
            heartbeat_payload: None,
            connect_timeout_ms: Some(10_000),
            reconnect_delay_initial_ms: Some(500),
            reconnect_delay_max_ms: Some(30_000),
            reconnect_backoff_factor: Some(2.0),
            reconnect_jitter_ms: Some(100),
            reconnect_max_attempts: None,
            heartbeat_timeout_secs: Some(60),
            idle_timeout_ms: None,
            backend: self.backend,
            proxy_url: self
                .proxy_url
                .as_ref()
                .map(|value| value.expose_secret().to_owned()),
        };
        let socket = WebSocketClient::builder()
            .config(config)
            .message_handler(handler)
            .connect()
            .await?;
        socket.set_auth_tracker(self.session.auth_tracker(), true);
        self.socket = Some(socket);
        self.raw_rx = Some(raw_rx);

        if let Err(error) = self.authenticate().await {
            self.disconnect().await;
            return Err(error);
        }
        Ok(())
    }

    /// Sends a subscription request and records intent for reconnect replay.
    ///
    /// # Errors
    ///
    /// Returns an error when disconnected, unauthenticated, or serialization/sending fails.
    pub async fn subscribe(&self, request: &AlpacaWsSubscription) -> anyhow::Result<()> {
        // Commands can arrive while the shared transport is re-authenticating. Persist the desired
        // state first so the reconnect replay cannot lose a subscription change made in that gap.
        let Some(request) = self.record_subscription_intent(request) else {
            return Ok(());
        };
        if !self.session.is_authenticated() {
            anyhow::bail!("Alpaca WebSocket client is not authenticated");
        }
        self.send_json(&request).await
    }

    /// Returns the next control or domain-data event.
    pub async fn next_message(&mut self) -> Option<AlpacaLiveMessage> {
        loop {
            if let Some(message) = self.output.pop_front() {
                return Some(message);
            }
            match self.read_and_process_one().await {
                Ok(true) => {}
                Ok(false) => return None,
                Err(error) => {
                    // A failed reconnect authentication leaves the transport active but unusable.
                    // Ask the shared controller for a fresh connection so authentication and
                    // subscription replay get another attempt instead of stalling permanently.
                    if !self.session.is_authenticated()
                        && self
                            .socket
                            .as_ref()
                            .is_some_and(WebSocketClient::request_reconnect)
                    {
                        log::warn!(
                            "Alpaca WebSocket re-authentication failed; reconnect requested"
                        );
                    }
                    return Some(AlpacaLiveMessage::Error(error.to_string()));
                }
            }
        }
    }

    pub async fn disconnect(&mut self) {
        if let Some(socket) = self.socket.take() {
            socket.disconnect().await;
        }
        self.raw_rx = None;
        self.output.clear();
    }

    async fn authenticate(&mut self) -> anyhow::Result<()> {
        let mut receiver = self.session.begin_authentication();
        let auth = AlpacaWsAuth::new(
            self.credential.api_key().to_string(),
            self.credential.api_secret().to_string(),
        );
        self.send_json(&auth).await?;

        tokio::time::timeout(AUTHENTICATION_TIMEOUT, async {
            loop {
                tokio::select! {
                    result = &mut receiver => {
                        return result
                            .map_err(|_| anyhow::anyhow!("Alpaca authentication channel closed"))?
                            .map_err(anyhow::Error::msg);
                    }
                    result = self.read_and_process_one() => {
                        anyhow::ensure!(
                            result?,
                            "Alpaca WebSocket message channel closed during authentication"
                        );
                    },
                }
            }
        })
        .await
        .map_err(|_| anyhow::anyhow!("Alpaca authentication timed out"))?
    }

    async fn send_json<T: serde::Serialize>(&self, value: &T) -> anyhow::Result<()> {
        let payload = SecretString::from(serde_json::to_string(value)?);
        let socket = self
            .socket
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Alpaca WebSocket client is disconnected"))?;
        socket
            .send_text(payload.expose_secret().to_owned(), None)
            .await?;
        Ok(())
    }

    /// Reads one transport message, returning `false` when the transport channel is terminal.
    async fn read_and_process_one(&mut self) -> anyhow::Result<bool> {
        let Some(raw_rx) = self.raw_rx.as_mut() else {
            return Ok(false);
        };
        let Some(message) = raw_rx.recv().await else {
            return Ok(false);
        };

        let payload = match message {
            Message::Text(text) => text.as_bytes().to_vec(),
            Message::Binary(bytes) => bytes.to_vec(),
            Message::Ping(bytes) => {
                if let Some(socket) = &self.socket {
                    socket.send_pong(bytes.to_vec()).await?;
                }
                return Ok(true);
            }
            Message::Close(_) => anyhow::bail!("Alpaca WebSocket closed"),
            _ => return Ok(true),
        };

        let session_events = self.session.on_frame(&payload)?;
        for event in session_events {
            self.apply_session_event(event).await?;
        }
        Ok(true)
    }

    async fn apply_session_event(&mut self, event: AlpacaSessionEvent) -> anyhow::Result<()> {
        match event {
            AlpacaSessionEvent::Authenticated => {
                self.output.push_back(AlpacaLiveMessage::Authenticated);
            }
            AlpacaSessionEvent::SubscriptionUpdated(state) => self
                .output
                .push_back(AlpacaLiveMessage::SubscriptionUpdated(state)),
            AlpacaSessionEvent::VenueError { code, message } => self
                .output
                .push_back(AlpacaLiveMessage::VenueError { code, message }),
            AlpacaSessionEvent::ProtocolWarning(message) => {
                self.output.push_back(AlpacaLiveMessage::Error(message));
            }
            AlpacaSessionEvent::MarketData(messages) => {
                let ts_init = UnixNanos::from(jiff::Timestamp::now());
                for message in messages {
                    match self.dispatcher.dispatch(&message, ts_init) {
                        Ok(event) => self.output.push_back(AlpacaLiveMessage::Data(event)),
                        Err(error) => self
                            .output
                            .push_back(AlpacaLiveMessage::Error(error.to_string())),
                    }
                }
            }
            AlpacaSessionEvent::Reconnected { topics_to_replay } => {
                Box::pin(self.authenticate()).await?;
                for request in subscriptions_from_topics(&topics_to_replay) {
                    self.send_json(&request).await?;
                }
                self.output.push_back(AlpacaLiveMessage::Reconnected);
            }
        }
        Ok(())
    }

    fn record_subscription_intent(
        &self,
        request: &AlpacaWsSubscription,
    ) -> Option<AlpacaWsSubscription> {
        let mut effective = AlpacaWsSubscription {
            action: request.action,
            ..Default::default()
        };
        for (channel, symbols) in request_channels(request) {
            for symbol in symbols {
                let topic = format!("{channel}|{symbol}");
                let should_send = match request.action {
                    super::messages::AlpacaWsAction::Subscribe => {
                        self.session.subscriptions().add_reference(&topic)
                    }
                    super::messages::AlpacaWsAction::Unsubscribe => {
                        self.session.subscriptions().remove_reference(&topic)
                    }
                };
                if !should_send {
                    continue;
                }
                match request.action {
                    super::messages::AlpacaWsAction::Subscribe => {
                        self.session
                            .mark_subscribe(channel, std::slice::from_ref(symbol));
                    }
                    super::messages::AlpacaWsAction::Unsubscribe => {
                        self.session
                            .mark_unsubscribe(channel, std::slice::from_ref(symbol));
                    }
                }
                push_subscription_symbol(&mut effective, channel, symbol.clone());
            }
        }
        (!request_channels(&effective)
            .iter()
            .all(|(_, symbols)| symbols.is_empty()))
        .then_some(effective)
    }
}

fn push_subscription_symbol(request: &mut AlpacaWsSubscription, channel: &str, symbol: String) {
    match channel {
        "trades" => request.trades.push(symbol),
        "quotes" => request.quotes.push(symbol),
        "bars" => request.bars.push(symbol),
        "dailyBars" => request.daily_bars.push(symbol),
        "updatedBars" => request.updated_bars.push(symbol),
        "statuses" => request.statuses.push(symbol),
        "lulds" => request.lulds.push(symbol),
        _ => unreachable!("known Alpaca subscription channel"),
    }
}

fn request_channels(request: &AlpacaWsSubscription) -> [(&str, &[String]); 7] {
    [
        ("trades", &request.trades),
        ("quotes", &request.quotes),
        ("bars", &request.bars),
        ("dailyBars", &request.daily_bars),
        ("updatedBars", &request.updated_bars),
        ("statuses", &request.statuses),
        ("lulds", &request.lulds),
    ]
}

fn subscriptions_from_topics(topics: &[String]) -> Vec<AlpacaWsSubscription> {
    let mut request = AlpacaWsSubscription::default();
    for topic in topics {
        let Some((channel, symbol)) = topic.split_once('|') else {
            continue;
        };
        match channel {
            "trades" => request.trades.push(symbol.to_string()),
            "quotes" => request.quotes.push(symbol.to_string()),
            "bars" => request.bars.push(symbol.to_string()),
            "dailyBars" => request.daily_bars.push(symbol.to_string()),
            "updatedBars" => request.updated_bars.push(symbol.to_string()),
            "statuses" => request.statuses.push(symbol.to_string()),
            "lulds" => request.lulds.push(symbol.to_string()),
            _ => {}
        }
    }
    vec![request]
}

#[cfg(test)]
mod tests {
    use futures_util::{SinkExt, StreamExt};
    use nautilus_model::{identifiers::InstrumentId, types::Price};
    use rstest::rstest;
    use tokio::net::TcpListener;
    use tokio_tungstenite::{accept_async, tungstenite::Message};

    use super::*;
    use crate::websocket::messages::AlpacaWsAction;

    #[rstest]
    fn test_replay_groups_topics_into_one_subscription() {
        let requests = subscriptions_from_topics(&[
            "quotes|AMD".to_string(),
            "trades|AAPL".to_string(),
            "bars|SPY".to_string(),
            "statuses|MSFT".to_string(),
            "lulds|IONM".to_string(),
        ]);

        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].action, AlpacaWsAction::Subscribe);
        assert_eq!(requests[0].trades, ["AAPL"]);
        assert_eq!(requests[0].quotes, ["AMD"]);
        assert_eq!(requests[0].bars, ["SPY"]);
        assert_eq!(requests[0].statuses, ["MSFT"]);
        assert_eq!(requests[0].lulds, ["IONM"]);
    }

    #[rstest]
    fn test_subscription_intent_is_reference_counted() {
        let client = AlpacaWebSocketClient::new(
            "ws://127.0.0.1:1",
            AlpacaCredential::new("test-key", "test-secret"),
            TransportBackend::Tungstenite,
            None,
        );
        let subscribe = AlpacaWsSubscription {
            trades: vec!["AAPL".to_string()],
            ..Default::default()
        };
        let unsubscribe = AlpacaWsSubscription {
            action: AlpacaWsAction::Unsubscribe,
            trades: vec!["AAPL".to_string()],
            ..Default::default()
        };

        assert!(client.record_subscription_intent(&subscribe).is_some());
        assert!(client.record_subscription_intent(&subscribe).is_none());
        assert_eq!(
            client
                .session
                .subscriptions()
                .get_reference_count("trades|AAPL"),
            2
        );
        assert!(client.record_subscription_intent(&unsubscribe).is_none());
        assert!(client.record_subscription_intent(&unsubscribe).is_some());
        assert_eq!(
            client
                .session
                .subscriptions()
                .get_reference_count("trades|AAPL"),
            0
        );
        assert_eq!(
            client.session.subscriptions().pending_unsubscribe_topics(),
            ["trades|AAPL"]
        );
    }

    #[rstest]
    #[tokio::test]
    async fn test_next_message_returns_none_when_transport_channel_closes() {
        let mut client = AlpacaWebSocketClient::new(
            "ws://127.0.0.1:1",
            AlpacaCredential::new("test-key", "test-secret"),
            TransportBackend::Tungstenite,
            None,
        );
        let (raw_tx, raw_rx) = tokio::sync::mpsc::unbounded_channel();
        client.raw_rx = Some(raw_rx);
        drop(raw_tx);

        assert_eq!(client.next_message().await, None);
    }

    #[rstest]
    #[tokio::test]
    async fn test_subscription_intent_is_retained_while_unauthenticated() {
        let client = AlpacaWebSocketClient::new(
            "ws://127.0.0.1:1",
            AlpacaCredential::new("test-key", "test-secret"),
            TransportBackend::Tungstenite,
            None,
        );
        let request = AlpacaWsSubscription {
            trades: vec!["AAPL".to_string()],
            ..Default::default()
        };

        let error = client.subscribe(&request).await.unwrap_err();

        assert!(error.to_string().contains("not authenticated"));
        assert_eq!(
            client.session.subscriptions().pending_subscribe_topics(),
            ["trades|AAPL"]
        );
    }

    #[rstest]
    #[tokio::test]
    async fn test_mock_server_auth_subscription_and_trade_end_to_end() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut socket = accept_async(stream).await.unwrap();
            socket
                .send(Message::Text(
                    r#"[{"T":"success","msg":"connected"}]"#.into(),
                ))
                .await
                .unwrap();

            let auth = socket.next().await.unwrap().unwrap().into_text().unwrap();
            assert_eq!(
                serde_json::from_str::<serde_json::Value>(&auth).unwrap(),
                serde_json::json!({"action":"auth","key":"test-key","secret":"test-secret"})
            );
            socket
                .send(Message::Text(
                    r#"[{"T":"success","msg":"authenticated"}]"#.into(),
                ))
                .await
                .unwrap();

            let subscription = socket.next().await.unwrap().unwrap().into_text().unwrap();
            assert_eq!(
                serde_json::from_str::<serde_json::Value>(&subscription).unwrap(),
                serde_json::json!({"action":"subscribe","trades":["AAPL"]})
            );
            socket
                .send(Message::Text(
                    r#"[{"T":"subscription","trades":["AAPL"]}]"#.into(),
                ))
                .await
                .unwrap();
            socket
                .send(Message::Text(
                    r#"[{"T":"t","i":96921,"S":"AAPL","x":"D","p":126.55,"s":1,"t":"2021-02-22T15:51:44.208Z","c":["@","I"],"z":"C"}]"#.into(),
                ))
                .await
                .unwrap();
        });

        let mut client = AlpacaWebSocketClient::new(
            format!("ws://{address}"),
            AlpacaCredential::new("test-key", "test-secret"),
            TransportBackend::Tungstenite,
            None,
        );
        client.dispatcher_mut().register_instrument(
            "AAPL",
            InstrumentId::from("AAPL.ALPACA"),
            2,
            0,
        );

        client.connect().await.unwrap();
        client
            .subscribe(&AlpacaWsSubscription {
                trades: vec!["AAPL".to_string()],
                ..Default::default()
            })
            .await
            .unwrap();

        assert_eq!(
            client.next_message().await,
            Some(AlpacaLiveMessage::Authenticated)
        );
        assert!(matches!(
            client.next_message().await,
            Some(AlpacaLiveMessage::SubscriptionUpdated(_))
        ));
        assert!(matches!(
            client.next_message().await,
            Some(AlpacaLiveMessage::Data(AlpacaDataEvent::Trade(tick)))
                if tick.price == Price::from("126.55")
        ));

        server.await.unwrap();
        client.disconnect().await;
    }

    #[rstest]
    #[tokio::test]
    async fn test_failed_reconnect_authentication_requests_fresh_session_and_replays() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut first = accept_async(stream).await.unwrap();
            first
                .send(Message::Text(
                    r#"[{"T":"success","msg":"connected"}]"#.into(),
                ))
                .await
                .unwrap();
            first.next().await.unwrap().unwrap();
            first
                .send(Message::Text(
                    r#"[{"T":"success","msg":"authenticated"}]"#.into(),
                ))
                .await
                .unwrap();
            let subscription = first.next().await.unwrap().unwrap().into_text().unwrap();
            first
                .send(Message::Text(
                    r#"[{"T":"subscription","trades":["AAPL"]}]"#.into(),
                ))
                .await
                .unwrap();
            drop(first);

            let (stream, _) = listener.accept().await.unwrap();
            let mut rejected = accept_async(stream).await.unwrap();
            rejected
                .send(Message::Text(
                    r#"[{"T":"success","msg":"connected"}]"#.into(),
                ))
                .await
                .unwrap();
            rejected.next().await.unwrap().unwrap();
            rejected
                .send(Message::Text(
                    r#"[{"T":"error","code":402,"msg":"auth failed"}]"#.into(),
                ))
                .await
                .unwrap();

            let (stream, _) = listener.accept().await.unwrap();
            let mut recovered = accept_async(stream).await.unwrap();
            recovered
                .send(Message::Text(
                    r#"[{"T":"success","msg":"connected"}]"#.into(),
                ))
                .await
                .unwrap();
            recovered.next().await.unwrap().unwrap();
            recovered
                .send(Message::Text(
                    r#"[{"T":"success","msg":"authenticated"}]"#.into(),
                ))
                .await
                .unwrap();
            let replay = recovered
                .next()
                .await
                .unwrap()
                .unwrap()
                .into_text()
                .unwrap();
            assert_eq!(
                serde_json::from_str::<serde_json::Value>(&replay).unwrap(),
                serde_json::from_str::<serde_json::Value>(&subscription).unwrap()
            );
        });

        let mut client = AlpacaWebSocketClient::new(
            format!("ws://{address}"),
            AlpacaCredential::new("test-key", "test-secret"),
            TransportBackend::Tungstenite,
            None,
        );
        client.connect().await.unwrap();
        client
            .subscribe(&AlpacaWsSubscription {
                trades: vec!["AAPL".to_string()],
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(
            client.next_message().await,
            Some(AlpacaLiveMessage::Authenticated)
        );
        assert!(matches!(
            client.next_message().await,
            Some(AlpacaLiveMessage::SubscriptionUpdated(_))
        ));

        tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                if matches!(
                    client.next_message().await,
                    Some(AlpacaLiveMessage::Reconnected)
                ) {
                    break;
                }
            }
        })
        .await
        .unwrap();

        assert!(client.is_authenticated());
        server.await.unwrap();
        client.disconnect().await;
    }
}
