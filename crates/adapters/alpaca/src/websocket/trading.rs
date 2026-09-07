// -------------------------------------------------------------------------------------------------
//  Copyright (C) 2015-2026 Nautech Systems Pty Ltd. All rights reserved.
//  https://nautechsystems.io
//
//  Licensed under the GNU Lesser General Public License Version 3.0 (the "License");
// -------------------------------------------------------------------------------------------------

//! Authenticated Alpaca account/order update stream.

use std::time::Duration;

use jiff::Timestamp;
use nautilus_core::string::secret::SecretString;
use nautilus_network::{
    RECONNECTED,
    websocket::{TransportBackend, WebSocketClient, WebSocketConfig, channel_message_handler},
};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use tokio_tungstenite::tungstenite::Message;

use crate::{common::credential::AlpacaCredential, http::models::AlpacaOrder};

const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(tag = "stream", content = "data")]
pub enum AlpacaTradingMessage {
    #[serde(rename = "authorization")]
    Authorization(AlpacaAuthorization),
    #[serde(rename = "listening")]
    Listening(AlpacaListening),
    #[serde(rename = "trade_updates")]
    TradeUpdate(Box<AlpacaTradeUpdate>),
    #[serde(other)]
    Unknown,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct AlpacaAuthorization {
    pub status: String,
    pub action: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct AlpacaListening {
    #[serde(default)]
    pub streams: Vec<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AlpacaTradeEvent {
    New,
    Fill,
    PartialFill,
    Canceled,
    Expired,
    DoneForDay,
    Replaced,
    Accepted,
    Rejected,
    PendingNew,
    Stopped,
    PendingCancel,
    PendingReplace,
    Calculated,
    Suspended,
    OrderReplaceRejected,
    OrderCancelRejected,
    #[serde(other)]
    Unknown,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct AlpacaTradeUpdate {
    pub event: AlpacaTradeEvent,
    pub order: AlpacaOrder,
    pub execution_id: Option<String>,
    pub price: Option<Decimal>,
    pub qty: Option<Decimal>,
    pub position_qty: Option<Decimal>,
    pub timestamp: Option<Timestamp>,
}

#[derive(Serialize)]
struct AuthRequest<'a> {
    action: &'static str,
    key: &'a str,
    secret: &'a str,
}

#[derive(Serialize)]
struct ListenRequest {
    action: &'static str,
    data: ListenData,
}

#[derive(Serialize)]
struct ListenData {
    streams: [&'static str; 1],
}

/// Reconnecting private-stream client. Re-authentication is completed before a reconnect event is
/// returned, so consumers never observe a nominally ready but unauthorized session.
#[derive(Debug)]
pub struct AlpacaTradingWebSocketClient {
    url: String,
    credential: AlpacaCredential,
    backend: TransportBackend,
    proxy_url: Option<SecretString>,
    socket: Option<WebSocketClient>,
    rx: Option<tokio::sync::mpsc::UnboundedReceiver<Message>>,
}

impl AlpacaTradingWebSocketClient {
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
            rx: None,
        }
    }

    pub async fn connect(&mut self) -> anyhow::Result<()> {
        anyhow::ensure!(
            self.socket.is_none(),
            "Alpaca trading stream is already connected"
        );
        let (handler, rx) = channel_message_handler();
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
        self.socket = Some(
            WebSocketClient::builder()
                .config(config)
                .message_handler(handler)
                .connect()
                .await?,
        );
        self.rx = Some(rx);
        if let Err(error) = self.authenticate_and_listen().await {
            self.disconnect().await;
            return Err(error);
        }
        Ok(())
    }

    pub async fn next_message(&mut self) -> Option<anyhow::Result<AlpacaTradingMessage>> {
        loop {
            let frame = match self.next_frame().await {
                Ok(frame) => frame,
                Err(error) => return Some(Err(error)),
            };
            if frame == RECONNECTED.as_bytes() {
                if let Err(error) = self.authenticate_and_listen().await {
                    return Some(Err(error));
                }
                continue;
            }
            match serde_json::from_slice::<AlpacaTradingMessage>(&frame) {
                Ok(AlpacaTradingMessage::TradeUpdate(update)) => {
                    return Some(Ok(AlpacaTradingMessage::TradeUpdate(update)));
                }
                Ok(message) => return Some(Ok(message)),
                Err(error) => return Some(Err(error.into())),
            }
        }
    }

    pub async fn disconnect(&mut self) {
        if let Some(socket) = self.socket.take() {
            socket.disconnect().await;
        }
        self.rx = None;
    }

    async fn authenticate_and_listen(&mut self) -> anyhow::Result<()> {
        let auth = AuthRequest {
            action: "auth",
            key: self.credential.api_key(),
            secret: self.credential.api_secret(),
        };
        self.send_json(&auth).await?;
        tokio::time::timeout(HANDSHAKE_TIMEOUT, async {
            loop {
                let frame = self.next_frame().await?;
                if frame == RECONNECTED.as_bytes() {
                    continue;
                }
                match serde_json::from_slice::<AlpacaTradingMessage>(&frame)? {
                    AlpacaTradingMessage::Authorization(auth) if auth.status == "authorized" => {
                        break;
                    }
                    AlpacaTradingMessage::Authorization(auth) => anyhow::bail!(
                        "Alpaca trading stream authorization failed: {}",
                        auth.status
                    ),
                    _ => {}
                }
            }
            let listen = ListenRequest {
                action: "listen",
                data: ListenData {
                    streams: ["trade_updates"],
                },
            };
            self.send_json(&listen).await?;
            loop {
                let frame = self.next_frame().await?;
                match serde_json::from_slice::<AlpacaTradingMessage>(&frame)? {
                    AlpacaTradingMessage::Listening(state)
                        if state.streams.iter().any(|value| value == "trade_updates") =>
                    {
                        return Ok(());
                    }
                    AlpacaTradingMessage::Authorization(auth) if auth.status != "authorized" => {
                        anyhow::bail!("Alpaca listen authorization failed: {}", auth.status)
                    }
                    _ => {}
                }
            }
        })
        .await
        .map_err(|_| anyhow::anyhow!("Alpaca trading stream handshake timed out"))?
    }

    async fn send_json(&self, value: &impl Serialize) -> anyhow::Result<()> {
        let payload = serde_json::to_string(value)?;
        self.socket
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Alpaca trading stream is disconnected"))?
            .send_text(payload, None)
            .await?;
        Ok(())
    }

    async fn next_frame(&mut self) -> anyhow::Result<Vec<u8>> {
        loop {
            let message = self
                .rx
                .as_mut()
                .ok_or_else(|| anyhow::anyhow!("Alpaca trading stream is disconnected"))?
                .recv()
                .await
                .ok_or_else(|| anyhow::anyhow!("Alpaca trading stream closed"))?;
            match message {
                Message::Text(value) => return Ok(value.as_bytes().to_vec()),
                Message::Binary(value) => return Ok(value.to_vec()),
                Message::Ping(value) => {
                    if let Some(socket) = &self.socket {
                        socket.send_pong(value.to_vec()).await?;
                    }
                }
                Message::Close(_) => anyhow::bail!("Alpaca trading stream closed"),
                _ => {}
            }
        }
    }
}

#[cfg(test)]
mod protocol_tests {
    use rstest::rstest;

    use super::*;

    #[rstest]
    fn test_unknown_private_stream_payload_is_rejected_not_discarded() {
        let result = serde_json::from_str::<AlpacaTradingMessage>(
            r#"{"stream":"future_account_event","data":{"value":1}}"#,
        );

        assert!(result.is_err());
    }
}

#[cfg(test)]
mod tests {
    use futures_util::{SinkExt, StreamExt};
    use rstest::rstest;
    use tokio::net::TcpListener;
    use tokio_tungstenite::{accept_async, tungstenite::Message};

    use super::*;

    #[rstest]
    #[tokio::test]
    async fn test_binary_paper_stream_auth_listen_and_fill() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut ws = accept_async(stream).await.unwrap();
            let auth: serde_json::Value =
                serde_json::from_str(ws.next().await.unwrap().unwrap().to_text().unwrap()).unwrap();
            assert_eq!(auth["action"], "auth");
            assert_eq!(auth["key"], "key");
            ws.send(Message::Binary(br#"{"stream":"authorization","data":{"status":"authorized","action":"authenticate"}}"#.to_vec().into())).await.unwrap();
            let listen: serde_json::Value =
                serde_json::from_str(ws.next().await.unwrap().unwrap().to_text().unwrap()).unwrap();
            assert_eq!(listen["data"]["streams"][0], "trade_updates");
            ws.send(Message::Binary(
                br#"{"stream":"listening","data":{"streams":["trade_updates"]}}"#
                    .to_vec()
                    .into(),
            ))
            .await
            .unwrap();
            ws.send(Message::Binary(br#"{"stream":"trade_updates","data":{"event":"fill","execution_id":"exec-1","order":{"id":"order-1","client_order_id":"client-1","symbol":"AAPL","asset_class":"us_equity","qty":"2","filled_qty":"2","filled_avg_price":"185.25","side":"buy","type":"market","time_in_force":"day","limit_price":null,"stop_price":null,"status":"filled","extended_hours":false,"created_at":"2024-01-01T00:00:00Z","updated_at":"2024-01-01T00:00:01Z","submitted_at":"2024-01-01T00:00:00Z","filled_at":"2024-01-01T00:00:01Z","canceled_at":null,"expired_at":null,"failed_at":null},"price":"185.25","qty":"2","position_qty":"2","timestamp":"2024-01-01T00:00:01Z"}}"#.to_vec().into())).await.unwrap();
        });
        let mut client = AlpacaTradingWebSocketClient::new(
            format!("ws://{address}"),
            AlpacaCredential::new("key", "secret"),
            TransportBackend::Tungstenite,
            None,
        );
        client.connect().await.unwrap();
        let message = client.next_message().await.unwrap().unwrap();
        let AlpacaTradingMessage::TradeUpdate(update) = message else {
            panic!("expected trade update")
        };
        assert_eq!(update.event, AlpacaTradeEvent::Fill);
        assert_eq!(update.execution_id.as_deref(), Some("exec-1"));
        assert_eq!(update.price, Some(Decimal::new(18_525, 2)));
        client.disconnect().await;
        server.await.unwrap();
    }
}
