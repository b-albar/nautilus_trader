// -------------------------------------------------------------------------------------------------
//  Copyright (C) 2015-2026 Nautech Systems Pty Ltd. All rights reserved.
//  https://nautechsystems.io
//
//  Licensed under the GNU Lesser General Public License Version 3.0 (the "License");
// -------------------------------------------------------------------------------------------------

//! Nautilus live-data client for Alpaca equities.

use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

use async_trait::async_trait;
use nautilus_common::{
    clients::DataClient,
    live::runner::get_data_event_sender,
    messages::{
        DataEvent, DataResponse,
        data::{
            BarsResponse, InstrumentResponse, InstrumentsResponse, QuotesResponse, RequestBars,
            RequestInstrument, RequestInstruments, RequestQuotes, RequestTrades, SubscribeBars,
            SubscribeInstrument, SubscribeInstrumentStatus, SubscribeInstruments, SubscribeQuotes,
            SubscribeTrades, TradesResponse, UnsubscribeBars, UnsubscribeInstrumentStatus,
            UnsubscribeQuotes, UnsubscribeTrades,
        },
    },
    providers::InstrumentProvider,
};
use nautilus_core::{Params, UnixNanos, time::get_atomic_clock_realtime};
use nautilus_model::{
    data::{BarType, Data, QuoteTick, TradeTick},
    enums::{AggregationSource, AggressorSide, BarAggregation, PriceType},
    identifiers::{ClientId, InstrumentId, TradeId, Venue},
    instruments::{Instrument, InstrumentAny},
};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

use crate::{
    common::enums::{AlpacaBarAdjustment, AlpacaDataFeed},
    config::AlpacaDataClientConfig,
    http::{
        client::AlpacaRawHttpClient,
        query::{AlpacaBarsQuery, AlpacaTicksQuery},
    },
    provider::AlpacaInstrumentProvider,
    websocket::{
        AlpacaLiveMessage, AlpacaWebSocketClient,
        dispatch::AlpacaDataEvent,
        messages::{AlpacaWsAction, AlpacaWsSubscription},
    },
};

fn alpaca_venue() -> Venue {
    Venue::new("ALPACA")
}

#[derive(Debug)]
enum SocketCommand {
    Subscribe(AlpacaWsSubscription),
    RegisterBar {
        symbol: String,
        minute: Option<BarType>,
        daily: Option<BarType>,
    },
    Disconnect,
}

/// Native Alpaca client implementing Nautilus's [`DataClient`] boundary.
#[derive(Debug)]
pub struct AlpacaDataClient {
    client_id: ClientId,
    config: AlpacaDataClientConfig,
    provider: AlpacaInstrumentProvider,
    http: AlpacaRawHttpClient,
    socket: Option<AlpacaWebSocketClient>,
    socket_tx: Option<UnboundedSender<SocketCommand>>,
    socket_task: Option<tokio::task::JoinHandle<AlpacaWebSocketClient>>,
    pending_requests: Mutex<Vec<tokio::task::JoinHandle<()>>>,
    connected: Arc<AtomicBool>,
    data_sender: UnboundedSender<DataEvent>,
    instruments: Arc<Mutex<HashMap<InstrumentId, InstrumentAny>>>,
}

impl AlpacaDataClient {
    #[must_use]
    pub fn new(
        client_id: ClientId,
        config: AlpacaDataClientConfig,
        provider: AlpacaInstrumentProvider,
        http: AlpacaRawHttpClient,
        socket: AlpacaWebSocketClient,
    ) -> Self {
        Self {
            client_id,
            config,
            provider,
            http,
            socket: Some(socket),
            socket_tx: None,
            socket_task: None,
            pending_requests: Mutex::new(Vec::new()),
            connected: Arc::new(AtomicBool::new(false)),
            data_sender: get_data_event_sender(),
            instruments: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    fn send_socket(&self, command: SocketCommand) -> anyhow::Result<()> {
        self.socket_tx
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Alpaca data client is disconnected"))?
            .send(command)
            .map_err(|_| anyhow::anyhow!("Alpaca socket task is unavailable"))
    }

    fn symbol_request(
        symbol: String,
        action: AlpacaWsAction,
        channel: &str,
    ) -> AlpacaWsSubscription {
        let mut request = AlpacaWsSubscription {
            action,
            ..Default::default()
        };
        match channel {
            "quotes" => request.quotes.push(symbol),
            "trades" => request.trades.push(symbol),
            "bars" => request.bars.push(symbol),
            "dailyBars" => request.daily_bars.push(symbol),
            "statuses" => request.statuses.push(symbol),
            _ => unreachable!("validated Alpaca channel"),
        }
        request
    }

    fn bar_channel(bar_type: BarType) -> anyhow::Result<&'static str> {
        anyhow::ensure!(
            bar_type.aggregation_source() == AggregationSource::External,
            "Alpaca requires EXTERNAL bars"
        );
        let spec = bar_type.spec();
        anyhow::ensure!(
            spec.price_type == PriceType::Last && spec.step.get() == 1,
            "Alpaca live bars support 1-MINUTE-LAST or 1-DAY-LAST"
        );
        match spec.aggregation {
            BarAggregation::Minute => Ok("bars"),
            BarAggregation::Day => Ok("dailyBars"),
            _ => anyhow::bail!("Alpaca live bars support only minute and daily bars"),
        }
    }

    fn cached_instruments(&self) -> Vec<InstrumentAny> {
        self.instruments
            .lock()
            .expect("instrument cache poisoned")
            .values()
            .cloned()
            .collect()
    }

    fn historical_instrument(&self, instrument_id: InstrumentId) -> anyhow::Result<InstrumentAny> {
        self.instruments
            .lock()
            .expect("instrument cache poisoned")
            .get(&instrument_id)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("Unknown Alpaca instrument {instrument_id}"))
    }

    fn validate_live_instrument(&self, instrument_id: InstrumentId) -> anyhow::Result<()> {
        anyhow::ensure!(
            instrument_id.venue == alpaca_venue(),
            "Alpaca subscriptions require venue ALPACA, received {instrument_id}"
        );
        anyhow::ensure!(
            self.instruments
                .lock()
                .expect("instrument cache poisoned")
                .contains_key(&instrument_id),
            "Unknown Alpaca instrument {instrument_id}"
        );
        Ok(())
    }

    fn historical_feed(&self) -> anyhow::Result<AlpacaDataFeed> {
        let feed = self.config.effective_historical_feed();
        anyhow::ensure!(
            feed.supports_historical_ranges(),
            "Alpaca historical ranges support IEX, SIP, BOATS, or OTC; configured {feed}"
        );
        Ok(feed)
    }

    fn spawn_request(&self, name: &'static str, future: impl Future<Output = ()> + Send + 'static) {
        let task = tokio::spawn(future);
        let mut pending = self
            .pending_requests
            .lock()
            .expect("Alpaca historical request registry poisoned");
        pending.retain(|task| !task.is_finished());
        pending.push(task);
        log::debug!("Spawned Alpaca historical request {name}");
    }

    fn abort_requests(&self) {
        for task in self
            .pending_requests
            .lock()
            .expect("Alpaca historical request registry poisoned")
            .drain(..)
        {
            task.abort();
        }
    }

    async fn reap_socket_task(&mut self) -> anyhow::Result<()> {
        if let Some(task) = self.socket_task.take() {
            self.socket = Some(task.await?);
            self.socket_tx = None;
        }
        Ok(())
    }
}

#[async_trait(?Send)]
impl DataClient for AlpacaDataClient {
    fn client_id(&self) -> ClientId {
        self.client_id
    }

    fn venue(&self) -> Option<Venue> {
        Some(alpaca_venue())
    }

    fn start(&mut self) -> anyhow::Result<()> {
        Ok(())
    }

    fn stop(&mut self) -> anyhow::Result<()> {
        self.abort_requests();
        if let Some(tx) = &self.socket_tx {
            let _ = tx.send(SocketCommand::Disconnect);
        }
        self.connected.store(false, Ordering::Release);
        Ok(())
    }

    fn reset(&mut self) -> anyhow::Result<()> {
        self.stop()
    }

    fn dispose(&mut self) -> anyhow::Result<()> {
        self.stop()
    }

    fn is_connected(&self) -> bool {
        self.connected.load(Ordering::Acquire)
    }

    fn is_disconnected(&self) -> bool {
        !self.is_connected()
    }

    async fn connect(&mut self) -> anyhow::Result<()> {
        if self.is_connected() {
            return Ok(());
        }
        // A terminal transport closes the runner and flips `connected` itself. Reap that task here
        // so a node-level reconnect does not require an artificial disconnect call in between.
        self.reap_socket_task().await?;

        if let Some(instrument_ids) = &self.config.instrument_ids {
            self.provider.load_ids(instrument_ids).await?;
        } else {
            self.provider.load_all(None).await?;
        }
        let instruments: Vec<_> = self.provider.store().get_all().values().cloned().collect();
        anyhow::ensure!(
            !instruments.is_empty(),
            "Alpaca returned no usable instruments"
        );

        let mut socket = self
            .socket
            .take()
            .ok_or_else(|| anyhow::anyhow!("Alpaca socket cannot be reconnected after disposal"))?;
        for instrument in &instruments {
            socket.dispatcher_mut().register_instrument(
                instrument.raw_symbol().to_string(),
                instrument.id(),
                instrument.price_precision(),
                instrument.size_precision(),
            );
        }
        if let Err(error) = socket.connect().await {
            self.socket = Some(socket);
            return Err(error);
        }

        {
            let mut cache = self.instruments.lock().expect("instrument cache poisoned");
            cache.clear();
            for instrument in &instruments {
                cache.insert(instrument.id(), instrument.clone());
            }
        }
        for instrument in instruments {
            if let Err(error) = self.data_sender.send(DataEvent::Instrument(instrument)) {
                socket.disconnect().await;
                self.socket = Some(socket);
                self.instruments
                    .lock()
                    .expect("instrument cache poisoned")
                    .clear();
                return Err(error.into());
            }
        }

        let (tx, rx) = unbounded_channel();
        let sender = self.data_sender.clone();
        let connected = Arc::clone(&self.connected);
        connected.store(true, Ordering::Release);
        self.socket_task = Some(tokio::spawn(run_socket(socket, rx, sender, connected)));
        self.socket_tx = Some(tx);
        Ok(())
    }

    async fn disconnect(&mut self) -> anyhow::Result<()> {
        self.stop()?;
        self.socket_tx = None;
        self.reap_socket_task().await?;
        Ok(())
    }

    fn subscribe_instruments(&mut self, _cmd: SubscribeInstruments) -> anyhow::Result<()> {
        for instrument in self.cached_instruments() {
            self.data_sender.send(DataEvent::Instrument(instrument))?;
        }
        Ok(())
    }

    fn subscribe_instrument(&mut self, cmd: SubscribeInstrument) -> anyhow::Result<()> {
        self.validate_live_instrument(cmd.instrument_id)?;
        let instrument = self
            .instruments
            .lock()
            .expect("instrument cache poisoned")
            .get(&cmd.instrument_id)
            .cloned()
            .expect("validated Alpaca instrument disappeared from cache");
        self.data_sender.send(DataEvent::Instrument(instrument))?;
        Ok(())
    }

    fn subscribe_quotes(&mut self, cmd: SubscribeQuotes) -> anyhow::Result<()> {
        self.validate_live_instrument(cmd.instrument_id)?;
        self.send_socket(SocketCommand::Subscribe(Self::symbol_request(
            cmd.instrument_id.symbol.to_string(),
            AlpacaWsAction::Subscribe,
            "quotes",
        )))
    }

    fn subscribe_trades(&mut self, cmd: SubscribeTrades) -> anyhow::Result<()> {
        self.validate_live_instrument(cmd.instrument_id)?;
        self.send_socket(SocketCommand::Subscribe(Self::symbol_request(
            cmd.instrument_id.symbol.to_string(),
            AlpacaWsAction::Subscribe,
            "trades",
        )))
    }

    fn subscribe_instrument_status(
        &mut self,
        cmd: SubscribeInstrumentStatus,
    ) -> anyhow::Result<()> {
        self.validate_live_instrument(cmd.instrument_id)?;
        self.send_socket(SocketCommand::Subscribe(Self::symbol_request(
            cmd.instrument_id.symbol.to_string(),
            AlpacaWsAction::Subscribe,
            "statuses",
        )))
    }

    fn subscribe_bars(&mut self, cmd: SubscribeBars) -> anyhow::Result<()> {
        self.validate_live_instrument(cmd.bar_type.instrument_id())?;
        let channel = Self::bar_channel(cmd.bar_type)?;
        let symbol = cmd.bar_type.instrument_id().symbol.to_string();
        let (minute, daily) = if channel == "bars" {
            (Some(cmd.bar_type), None)
        } else {
            (None, Some(cmd.bar_type))
        };
        self.send_socket(SocketCommand::RegisterBar {
            symbol: symbol.clone(),
            minute,
            daily,
        })?;
        self.send_socket(SocketCommand::Subscribe(Self::symbol_request(
            symbol,
            AlpacaWsAction::Subscribe,
            channel,
        )))
    }

    fn unsubscribe_quotes(&mut self, cmd: &UnsubscribeQuotes) -> anyhow::Result<()> {
        self.validate_live_instrument(cmd.instrument_id)?;
        self.send_socket(SocketCommand::Subscribe(Self::symbol_request(
            cmd.instrument_id.symbol.to_string(),
            AlpacaWsAction::Unsubscribe,
            "quotes",
        )))
    }

    fn unsubscribe_trades(&mut self, cmd: &UnsubscribeTrades) -> anyhow::Result<()> {
        self.validate_live_instrument(cmd.instrument_id)?;
        self.send_socket(SocketCommand::Subscribe(Self::symbol_request(
            cmd.instrument_id.symbol.to_string(),
            AlpacaWsAction::Unsubscribe,
            "trades",
        )))
    }

    fn unsubscribe_instrument_status(
        &mut self,
        cmd: &UnsubscribeInstrumentStatus,
    ) -> anyhow::Result<()> {
        self.validate_live_instrument(cmd.instrument_id)?;
        self.send_socket(SocketCommand::Subscribe(Self::symbol_request(
            cmd.instrument_id.symbol.to_string(),
            AlpacaWsAction::Unsubscribe,
            "statuses",
        )))
    }

    fn unsubscribe_bars(&mut self, cmd: &UnsubscribeBars) -> anyhow::Result<()> {
        self.validate_live_instrument(cmd.bar_type.instrument_id())?;
        let channel = Self::bar_channel(cmd.bar_type)?;
        self.send_socket(SocketCommand::Subscribe(Self::symbol_request(
            cmd.bar_type.instrument_id().symbol.to_string(),
            AlpacaWsAction::Unsubscribe,
            channel,
        )))
    }

    fn request_instruments(&self, request: RequestInstruments) -> anyhow::Result<()> {
        let response = InstrumentsResponse::new(
            request.request_id,
            request.client_id.unwrap_or(self.client_id),
            alpaca_venue(),
            self.cached_instruments(),
            request.start.map(UnixNanos::from),
            request.end.map(UnixNanos::from),
            get_atomic_clock_realtime().get_time_ns(),
            request.params,
        );
        self.data_sender
            .send(DataEvent::Response(DataResponse::Instruments(response)))?;
        Ok(())
    }

    fn request_instrument(&self, request: RequestInstrument) -> anyhow::Result<()> {
        let instrument = self
            .instruments
            .lock()
            .expect("instrument cache poisoned")
            .get(&request.instrument_id)
            .cloned()
            .ok_or_else(|| {
                anyhow::anyhow!("Unknown Alpaca instrument {}", request.instrument_id)
            })?;
        let response = InstrumentResponse::new(
            request.request_id,
            request.client_id.unwrap_or(self.client_id),
            request.instrument_id,
            instrument,
            request.start.map(UnixNanos::from),
            request.end.map(UnixNanos::from),
            get_atomic_clock_realtime().get_time_ns(),
            request.params,
        );
        self.data_sender
            .send(DataEvent::Response(DataResponse::Instrument(Box::new(
                response,
            ))))?;
        Ok(())
    }

    fn request_bars(&self, request: RequestBars) -> anyhow::Result<()> {
        let timeframe = historical_timeframe(request.bar_type)?;
        let adjustment = request
            .params
            .as_ref()
            .and_then(|params| params.get_str("adjustment"))
            .map(str::parse::<AlpacaBarAdjustment>)
            .transpose()
            .map_err(|_| {
                anyhow::anyhow!("Alpaca bar adjustment must be raw, split, dividend, or all")
            })?
            .unwrap_or(self.config.bar_adjustment);
        let (start, end) = historical_range(request.start, request.end, "bars")?;
        let instrument = self
            .instruments
            .lock()
            .expect("instrument cache poisoned")
            .get(&request.bar_type.instrument_id())
            .cloned()
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "Unknown Alpaca instrument {}",
                    request.bar_type.instrument_id()
                )
            })?;
        let http = self.http.clone();
        let sender = self.data_sender.clone();
        let feed = self.historical_feed()?;
        let client_id = request.client_id.unwrap_or(self.client_id);
        let task = HistoricalBarsTask {
            http,
            sender,
            request,
            instrument,
            timeframe,
            start,
            end,
            feed,
            adjustment,
            client_id,
        };
        self.spawn_request("bars", async move {
            if let Err(error) = request_bars_task(task).await {
                log::error!("Alpaca historical bars request failed: {error}");
            }
        });
        Ok(())
    }

    fn request_quotes(&self, request: RequestQuotes) -> anyhow::Result<()> {
        let instrument = self.historical_instrument(request.instrument_id)?;
        let (start, end) = historical_range(request.start, request.end, "quotes")?;
        let task = HistoricalTicksTask {
            http: self.http.clone(),
            sender: self.data_sender.clone(),
            instrument,
            start,
            end,
            feed: self.historical_feed()?,
            client_id: request.client_id.unwrap_or(self.client_id),
        };
        self.spawn_request("quotes", async move {
            if let Err(error) = request_quotes_task(task, request).await {
                log::error!("Alpaca historical quotes request failed: {error}");
            }
        });
        Ok(())
    }

    fn request_trades(&self, request: RequestTrades) -> anyhow::Result<()> {
        let instrument = self.historical_instrument(request.instrument_id)?;
        let (start, end) = historical_range(request.start, request.end, "trades")?;
        let task = HistoricalTicksTask {
            http: self.http.clone(),
            sender: self.data_sender.clone(),
            instrument,
            start,
            end,
            feed: self.historical_feed()?,
            client_id: request.client_id.unwrap_or(self.client_id),
        };
        self.spawn_request("trades", async move {
            if let Err(error) = request_trades_task(task, request).await {
                log::error!("Alpaca historical trades request failed: {error}");
            }
        });
        Ok(())
    }
}

async fn run_socket(
    mut socket: AlpacaWebSocketClient,
    mut rx: UnboundedReceiver<SocketCommand>,
    sender: UnboundedSender<DataEvent>,
    connected: Arc<AtomicBool>,
) -> AlpacaWebSocketClient {
    loop {
        tokio::select! {
            command = rx.recv() => match command {
                Some(SocketCommand::Subscribe(request)) => if let Err(error) = socket.subscribe(&request).await { log::error!("Alpaca subscription failed: {error}"); },
                Some(SocketCommand::RegisterBar { symbol, minute, daily }) => if let Err(error) = socket.dispatcher_mut().register_bar_types(&symbol, minute, daily) { log::error!("Alpaca bar registration failed: {error}"); },
                Some(SocketCommand::Disconnect) | None => break,
            },
            message = socket.next_message() => match message {
                Some(AlpacaLiveMessage::Data(AlpacaDataEvent::Trade(value))) => { let _ = sender.send(DataEvent::Data(Data::Trade(value))); },
                Some(AlpacaLiveMessage::Data(AlpacaDataEvent::Quote(value))) => { let _ = sender.send(DataEvent::Data(Data::Quote(value))); },
                Some(AlpacaLiveMessage::Data(AlpacaDataEvent::Bar(value))) => { let _ = sender.send(DataEvent::Data(Data::Bar(value))); },
                Some(AlpacaLiveMessage::Data(AlpacaDataEvent::InstrumentStatus(value))) => { let _ = sender.send(DataEvent::Data(Data::InstrumentStatus(value))); },
                Some(AlpacaLiveMessage::Data(AlpacaDataEvent::Luld(value))) => log::warn!(
                    "Alpaca LULD bands for {}: down={} up={} indicator={} (Nautilus has no native LULD event)",
                    value.symbol,
                    value.limit_down_price,
                    value.limit_up_price,
                    value.indicator,
                ),
                Some(AlpacaLiveMessage::Data(AlpacaDataEvent::TradeCorrection(value))) => log::warn!(
                    "Alpaca trade correction for {}: original trade {} -> corrected trade {} (Nautilus has no market-trade retraction event)",
                    value.symbol,
                    value.original_trade_id,
                    value.corrected_trade_id,
                ),
                Some(AlpacaLiveMessage::Data(AlpacaDataEvent::TradeCancelError(value))) => log::warn!(
                    "Alpaca trade cancel/error for {}: trade {} action {} (Nautilus has no market-trade retraction event)",
                    value.symbol,
                    value.trade_id,
                    value.action,
                ),
                Some(AlpacaLiveMessage::Error(error)) => log::error!("Alpaca WebSocket: {error}"),
                Some(AlpacaLiveMessage::VenueError { code, message }) => log::warn!("Alpaca WebSocket error {code}: {message}"),
                Some(_) => {},
                None => break,
            }
        }
    }
    socket.disconnect().await;
    connected.store(false, Ordering::Release);
    socket
}

fn historical_timeframe(bar_type: BarType) -> anyhow::Result<String> {
    anyhow::ensure!(
        bar_type.aggregation_source() == AggregationSource::External
            && bar_type.spec().price_type == PriceType::Last,
        "Alpaca historical bars require LAST-EXTERNAL bar types"
    );
    let spec = bar_type.spec();
    let suffix = match spec.aggregation {
        BarAggregation::Minute => "Min",
        BarAggregation::Hour => "Hour",
        BarAggregation::Day => "Day",
        BarAggregation::Week => "Week",
        BarAggregation::Month => "Month",
        _ => anyhow::bail!(
            "Unsupported Alpaca historical bar aggregation {}",
            spec.aggregation
        ),
    };
    Ok(format!("{}{suffix}", spec.step))
}

fn historical_range(
    start: Option<jiff::Timestamp>,
    end: Option<jiff::Timestamp>,
    data_kind: &str,
) -> anyhow::Result<(jiff::Timestamp, jiff::Timestamp)> {
    let start = start.ok_or_else(|| {
        anyhow::anyhow!("Alpaca historical {data_kind} require an explicit start time")
    })?;
    let end = end.unwrap_or_else(jiff::Timestamp::now);
    anyhow::ensure!(
        start < end,
        "Alpaca historical {data_kind} require start before end"
    );
    Ok((start, end))
}

#[derive(Debug)]
struct HistoricalBarsTask {
    http: AlpacaRawHttpClient,
    sender: UnboundedSender<DataEvent>,
    request: RequestBars,
    instrument: InstrumentAny,
    timeframe: String,
    start: jiff::Timestamp,
    end: jiff::Timestamp,
    feed: AlpacaDataFeed,
    adjustment: AlpacaBarAdjustment,
    client_id: ClientId,
}

async fn request_bars_task(task: HistoricalBarsTask) -> anyhow::Result<()> {
    let HistoricalBarsTask {
        http,
        sender,
        request,
        instrument,
        timeframe,
        start,
        end,
        feed,
        adjustment,
        client_id,
    } = task;
    let symbol = request.bar_type.instrument_id().symbol.to_string();
    let mut page_token = None::<String>;
    let mut bars = Vec::new();
    loop {
        let mut query = AlpacaBarsQuery::new(&symbol, &timeframe, start, end, feed, adjustment);
        query.limit = Some(remaining_page_limit(request.limit, bars.len()));
        query.page_token = page_token.as_deref();
        let response = http.get_stock_bars(&query).await?;
        for value in response.bars.get(&symbol).into_iter().flatten() {
            bars.push(nautilus_model::data::Bar::new_checked(
                request.bar_type,
                nautilus_model::types::Price::from_decimal_dp(
                    value.open,
                    instrument.price_precision(),
                )?,
                nautilus_model::types::Price::from_decimal_dp(
                    value.high,
                    instrument.price_precision(),
                )?,
                nautilus_model::types::Price::from_decimal_dp(
                    value.low,
                    instrument.price_precision(),
                )?,
                nautilus_model::types::Price::from_decimal_dp(
                    value.close,
                    instrument.price_precision(),
                )?,
                nautilus_model::types::Quantity::from_decimal(value.volume)?,
                UnixNanos::from(value.timestamp),
                get_atomic_clock_realtime().get_time_ns(),
            )?);
        }
        let next = response.next_page_token;
        anyhow::ensure!(
            next != page_token || next.is_none(),
            "Alpaca historical bar pagination did not advance"
        );
        page_token = next;
        if page_token.is_none() || reached_limit(request.limit, bars.len()) {
            break;
        }
    }
    truncate_to_limit(&mut bars, request.limit);
    let response = BarsResponse::new(
        request.request_id,
        client_id,
        request.bar_type,
        bars,
        request.start.map(UnixNanos::from),
        request.end.map(UnixNanos::from),
        get_atomic_clock_realtime().get_time_ns(),
        Some(historical_response_params(
            request.params,
            feed,
            Some(adjustment),
        )),
    );
    sender.send(DataEvent::Response(DataResponse::Bars(response)))?;
    Ok(())
}

#[derive(Debug)]
struct HistoricalTicksTask {
    http: AlpacaRawHttpClient,
    sender: UnboundedSender<DataEvent>,
    instrument: InstrumentAny,
    start: jiff::Timestamp,
    end: jiff::Timestamp,
    feed: AlpacaDataFeed,
    client_id: ClientId,
}

async fn request_trades_task(
    task: HistoricalTicksTask,
    request: RequestTrades,
) -> anyhow::Result<()> {
    let symbol = request.instrument_id.symbol.to_string();
    let mut page_token = None::<String>;
    let mut trades = Vec::new();
    loop {
        let mut query = AlpacaTicksQuery::new(&symbol, task.start, task.end, task.feed);
        query.limit = remaining_page_limit(request.limit, trades.len());
        query.page_token = page_token.as_deref();
        let response = task.http.get_stock_trades(&query).await?;
        for value in response.trades.get(&symbol).into_iter().flatten() {
            trades.push(TradeTick::new_checked(
                request.instrument_id,
                nautilus_model::types::Price::from_decimal_dp(
                    value.price,
                    task.instrument.price_precision(),
                )?,
                nautilus_model::types::Quantity::from_decimal(value.size)?,
                AggressorSide::NoAggressor,
                TradeId::new(value.trade_id.to_string()),
                UnixNanos::from(value.timestamp),
                get_atomic_clock_realtime().get_time_ns(),
            )?);
        }
        let next = response.next_page_token;
        anyhow::ensure!(
            next != page_token || next.is_none(),
            "Alpaca historical trade pagination did not advance"
        );
        page_token = next;
        if page_token.is_none() || reached_limit(request.limit, trades.len()) {
            break;
        }
    }
    truncate_to_limit(&mut trades, request.limit);
    task.sender.send(DataEvent::Response(DataResponse::Trades(
        TradesResponse::new(
            request.request_id,
            task.client_id,
            request.instrument_id,
            trades,
            request.start.map(UnixNanos::from),
            request.end.map(UnixNanos::from),
            get_atomic_clock_realtime().get_time_ns(),
            Some(historical_response_params(request.params, task.feed, None)),
        ),
    )))?;
    Ok(())
}

async fn request_quotes_task(
    task: HistoricalTicksTask,
    request: RequestQuotes,
) -> anyhow::Result<()> {
    let symbol = request.instrument_id.symbol.to_string();
    let mut page_token = None::<String>;
    let mut quotes = Vec::new();
    let lot_size = rust_decimal::Decimal::from(100);
    loop {
        let mut query = AlpacaTicksQuery::new(&symbol, task.start, task.end, task.feed);
        query.limit = remaining_page_limit(request.limit, quotes.len());
        query.page_token = page_token.as_deref();
        let response = task.http.get_stock_quotes(&query).await?;
        for value in response.quotes.get(&symbol).into_iter().flatten() {
            quotes.push(QuoteTick::new_checked(
                request.instrument_id,
                nautilus_model::types::Price::from_decimal_dp(
                    value.bid_price,
                    task.instrument.price_precision(),
                )?,
                nautilus_model::types::Price::from_decimal_dp(
                    value.ask_price,
                    task.instrument.price_precision(),
                )?,
                nautilus_model::types::Quantity::from_decimal(value.bid_size_lots * lot_size)?,
                nautilus_model::types::Quantity::from_decimal(value.ask_size_lots * lot_size)?,
                UnixNanos::from(value.timestamp),
                get_atomic_clock_realtime().get_time_ns(),
            )?);
        }
        let next = response.next_page_token;
        anyhow::ensure!(
            next != page_token || next.is_none(),
            "Alpaca historical quote pagination did not advance"
        );
        page_token = next;
        if page_token.is_none() || reached_limit(request.limit, quotes.len()) {
            break;
        }
    }
    truncate_to_limit(&mut quotes, request.limit);
    task.sender.send(DataEvent::Response(DataResponse::Quotes(
        QuotesResponse::new(
            request.request_id,
            task.client_id,
            request.instrument_id,
            quotes,
            request.start.map(UnixNanos::from),
            request.end.map(UnixNanos::from),
            get_atomic_clock_realtime().get_time_ns(),
            Some(historical_response_params(request.params, task.feed, None)),
        ),
    )))?;
    Ok(())
}

fn remaining_page_limit(limit: Option<std::num::NonZeroUsize>, current: usize) -> u32 {
    limit.map_or(10_000, |value| {
        value.get().saturating_sub(current).clamp(1, 10_000) as u32
    })
}

fn reached_limit(limit: Option<std::num::NonZeroUsize>, current: usize) -> bool {
    limit.is_some_and(|value| current >= value.get())
}

fn truncate_to_limit<T>(values: &mut Vec<T>, limit: Option<std::num::NonZeroUsize>) {
    if let Some(limit) = limit {
        values.truncate(limit.get());
    }
}

fn historical_response_params(
    params: Option<Params>,
    feed: AlpacaDataFeed,
    adjustment: Option<AlpacaBarAdjustment>,
) -> Params {
    let mut params = params.unwrap_or_default();
    params.insert("provider".to_string(), serde_json::json!("alpaca"));
    params.insert("feed".to_string(), serde_json::json!(feed.to_string()));
    if let Some(adjustment) = adjustment {
        params.insert(
            "adjustment".to_string(),
            serde_json::json!(adjustment.to_string()),
        );
    }
    params
}

#[cfg(test)]
mod tests {
    use std::{num::NonZeroUsize, str::FromStr};

    use std::sync::atomic::AtomicUsize;

    use axum::{Json, Router, extract::Query, extract::State, routing::get};
    use nautilus_common::live::runner::replace_data_event_sender;
    use nautilus_common::messages::data::{RequestBars, RequestQuotes, RequestTrades};
    use nautilus_core::UUID4;
    use nautilus_model::instruments::stubs::equity_aapl;
    use rstest::rstest;
    use serde_json::{Value, json};
    use tokio::net::TcpListener;

    use super::*;
    use crate::common::credential::AlpacaCredential;

    #[rstest]
    #[case("AAPL.ALPACA-1-MINUTE-LAST-EXTERNAL", "bars")]
    #[case("AAPL.ALPACA-1-DAY-LAST-EXTERNAL", "dailyBars")]
    fn test_live_bar_channel(#[case] value: &str, #[case] expected: &str) {
        let bar_type = BarType::from_str(value).unwrap();
        assert_eq!(AlpacaDataClient::bar_channel(bar_type).unwrap(), expected);
    }

    #[rstest]
    #[case("AAPL.ALPACA-1-MINUTE-LAST-EXTERNAL", "1Min")]
    #[case("AAPL.ALPACA-4-HOUR-LAST-EXTERNAL", "4Hour")]
    #[case("AAPL.ALPACA-1-DAY-LAST-EXTERNAL", "1Day")]
    fn test_historical_timeframe(#[case] value: &str, #[case] expected: &str) {
        let bar_type = BarType::from_str(value).unwrap();
        assert_eq!(historical_timeframe(bar_type).unwrap(), expected);
    }

    #[rstest]
    fn test_live_bar_channel_rejects_internal_and_unsupported_widths() {
        let internal = BarType::from_str("AAPL.ALPACA-1-MINUTE-LAST-INTERNAL").unwrap();
        let wide = BarType::from_str("AAPL.ALPACA-5-MINUTE-LAST-EXTERNAL").unwrap();

        assert!(AlpacaDataClient::bar_channel(internal).is_err());
        assert!(AlpacaDataClient::bar_channel(wide).is_err());
    }

    #[rstest]
    fn test_live_subscription_requires_exact_loaded_alpaca_instrument() {
        let (data_sender, _data_receiver) = unbounded_channel();
        replace_data_event_sender(data_sender);
        let credential = AlpacaCredential::new("test-key", "test-secret");
        let http = AlpacaRawHttpClient::new(
            credential.clone(),
            Some("http://127.0.0.1:1".to_string()),
            1,
        )
        .unwrap();
        let provider = AlpacaInstrumentProvider::new(http.clone());
        let socket =
            AlpacaWebSocketClient::new("ws://127.0.0.1:1", credential, Default::default(), None);
        let client = AlpacaDataClient::new(
            ClientId::from("ALPACA"),
            AlpacaDataClientConfig::default(),
            provider,
            http,
            socket,
        );
        let alpaca_id = InstrumentId::from("AAPL.ALPACA");
        client
            .instruments
            .lock()
            .unwrap()
            .insert(alpaca_id, InstrumentAny::Equity(equity_aapl()));

        assert!(client.validate_live_instrument(alpaca_id).is_ok());
        assert_eq!(
            client
                .validate_live_instrument(InstrumentId::from("AAPL.XNAS"))
                .unwrap_err()
                .to_string(),
            "Alpaca subscriptions require venue ALPACA, received AAPL.XNAS"
        );
        assert_eq!(
            client
                .validate_live_instrument(InstrumentId::from("MSFT.ALPACA"))
                .unwrap_err()
                .to_string(),
            "Unknown Alpaca instrument MSFT.ALPACA"
        );
    }

    #[rstest]
    fn test_historical_range_requires_ordered_explicit_start() {
        let start: jiff::Timestamp = "2024-01-02T00:00:00Z".parse().unwrap();
        let end: jiff::Timestamp = "2024-01-01T00:00:00Z".parse().unwrap();

        let missing = historical_range(None, Some(end), "bars").unwrap_err();
        let reversed = historical_range(Some(start), Some(end), "quotes").unwrap_err();

        assert!(missing.to_string().contains("explicit start time"));
        assert!(reversed.to_string().contains("start before end"));
    }

    #[rstest]
    #[case(None, 0, 10_000)]
    #[case(NonZeroUsize::new(25_000), 0, 10_000)]
    #[case(NonZeroUsize::new(25_000), 10_000, 10_000)]
    #[case(NonZeroUsize::new(25_000), 20_000, 5_000)]
    #[case(NonZeroUsize::new(3), 2, 1)]
    fn test_historical_page_limit_respects_venue_maximum_and_remaining_rows(
        #[case] requested: Option<NonZeroUsize>,
        #[case] current: usize,
        #[case] expected: u32,
    ) {
        assert_eq!(remaining_page_limit(requested, current), expected);
    }

    #[rstest]
    fn test_historical_response_lineage_preserves_custom_params_and_normalizes_effective_values() {
        let mut params = Params::new();
        params.insert("experiment_id".to_string(), json!("exp-42"));
        params.insert("feed".to_string(), json!("stale"));

        let params = historical_response_params(
            Some(params),
            AlpacaDataFeed::Sip,
            Some(AlpacaBarAdjustment::Dividend),
        );

        assert_eq!(params.get_str("experiment_id"), Some("exp-42"));
        assert_eq!(params.get_str("provider"), Some("alpaca"));
        assert_eq!(params.get_str("feed"), Some("sip"));
        assert_eq!(params.get_str("adjustment"), Some("dividend"));
    }

    async fn historical_quotes() -> Json<Value> {
        Json(json!({
            "quotes":{"AAPL":[
                {"bp":"187.10","bs":"2","ap":"187.15","as":"3","t":"2024-01-01T00:00:01Z"},
                {"bp":"187.11","bs":"4","ap":"187.16","as":"5","t":"2024-01-01T00:00:02Z"}
            ]},
            "next_page_token":null
        }))
    }

    async fn historical_trades() -> Json<Value> {
        Json(json!({
            "trades":{"AAPL":[
                {"i":123,"p":"187.12","s":"0.25","t":"2024-01-01T00:00:01Z"},
                {"i":124,"p":"187.250","s":"1","t":"2024-01-01T00:00:02Z"}
            ]},
            "next_page_token":null
        }))
    }

    async fn paginated_historical_bars(
        State(page): State<Arc<AtomicUsize>>,
        Query(query): Query<HashMap<String, String>>,
    ) -> Json<Value> {
        let page = page.fetch_add(1, Ordering::SeqCst);
        let (expected_limit, expected_token, next_token, timestamps) = if page == 0 {
            ("3", None, Some("next"), [1, 2])
        } else {
            ("1", Some("next"), None, [3, 4])
        };
        assert_eq!(query.get("limit").map(String::as_str), Some(expected_limit));
        assert_eq!(query.get("page_token").map(String::as_str), expected_token);
        assert_eq!(query.get("adjustment").map(String::as_str), Some("split"));
        let bars: Vec<_> = timestamps
            .into_iter()
            .map(|second| {
                json!({
                    "t": format!("2024-01-01T00:00:0{second}Z"),
                    "o": "100.00", "h": "101.00", "l": "99.00", "c": "100.50",
                    "v": "10", "n": 2, "vw": "100.25"
                })
            })
            .collect();
        Json(json!({"bars":{"AAPL":bars}, "next_page_token":next_token}))
    }

    #[rstest]
    #[tokio::test]
    async fn test_historical_bars_paginate_with_exact_remaining_limit_and_truncate() {
        let page = Arc::new(AtomicUsize::new(0));
        let app = Router::new()
            .route("/v2/stocks/bars", get(paginated_historical_bars))
            .with_state(Arc::clone(&page));
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let http = AlpacaRawHttpClient::new(
            AlpacaCredential::new("test-key", "test-secret"),
            Some(format!("http://{address}")),
            10,
        )
        .unwrap();
        let instrument = InstrumentAny::Equity(equity_aapl());
        let bar_type = BarType::from_str("AAPL.XNAS-1-DAY-LAST-EXTERNAL").unwrap();
        let start: jiff::Timestamp = "2024-01-01T00:00:00Z".parse().unwrap();
        let end: jiff::Timestamp = "2024-01-02T00:00:00Z".parse().unwrap();
        let request_id = UUID4::new();
        let request = RequestBars::new(
            bar_type,
            Some(start),
            Some(end),
            NonZeroUsize::new(3),
            Some(ClientId::from("ALPACA")),
            request_id,
            UnixNanos::default(),
            None,
        );
        let (sender, mut receiver) = unbounded_channel();

        request_bars_task(HistoricalBarsTask {
            http,
            sender,
            request,
            instrument,
            timeframe: "1Day".to_string(),
            start,
            end,
            feed: AlpacaDataFeed::Iex,
            adjustment: AlpacaBarAdjustment::Split,
            client_id: ClientId::from("ALPACA"),
        })
        .await
        .unwrap();

        match receiver.recv().await.unwrap() {
            DataEvent::Response(DataResponse::Bars(response)) => {
                assert_eq!(response.correlation_id, request_id);
                assert_eq!(response.data.len(), 3);
                let params = response.params.unwrap();
                assert_eq!(params.get_str("provider"), Some("alpaca"));
                assert_eq!(params.get_str("feed"), Some("iex"));
                assert_eq!(params.get_str("adjustment"), Some("split"));
            }
            event => panic!("Expected historical bars response, got {event:?}"),
        }
        assert_eq!(page.load(Ordering::SeqCst), 2);
        server.abort();
    }

    #[rstest]
    #[tokio::test]
    async fn test_historical_tick_tasks_emit_correlated_limited_domain_responses() {
        let app = Router::new()
            .route("/v2/stocks/quotes", get(historical_quotes))
            .route("/v2/stocks/trades", get(historical_trades));
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let http = AlpacaRawHttpClient::new(
            AlpacaCredential::new("test-key", "test-secret"),
            Some(format!("http://{address}")),
            10,
        )
        .unwrap();
        let instrument = InstrumentAny::Equity(equity_aapl());
        let instrument_id = instrument.id();
        let start: jiff::Timestamp = "2024-01-01T00:00:00Z".parse().unwrap();
        let end: jiff::Timestamp = "2024-01-02T00:00:00Z".parse().unwrap();
        let client_id = ClientId::from("ALPACA");
        let (sender, mut receiver) = unbounded_channel();
        let quote_request_id = UUID4::new();
        let quote_request = RequestQuotes::new(
            instrument_id,
            Some(start),
            Some(end),
            NonZeroUsize::new(1),
            Some(client_id),
            quote_request_id,
            UnixNanos::default(),
            None,
        );
        request_quotes_task(
            HistoricalTicksTask {
                http: http.clone(),
                sender: sender.clone(),
                instrument: instrument.clone(),
                start,
                end,
                feed: AlpacaDataFeed::Iex,
                client_id,
            },
            quote_request,
        )
        .await
        .unwrap();
        match receiver.recv().await.unwrap() {
            DataEvent::Response(DataResponse::Quotes(response)) => {
                assert_eq!(response.correlation_id, quote_request_id);
                assert_eq!(response.data.len(), 1);
                assert_eq!(response.data[0].bid_size.to_string(), "200");
                assert_eq!(response.data[0].ask_size.to_string(), "300");
                let params = response.params.unwrap();
                assert_eq!(params.get_str("provider"), Some("alpaca"));
                assert_eq!(params.get_str("feed"), Some("iex"));
            }
            event => panic!("Expected historical quote response, got {event:?}"),
        }

        let trade_request_id = UUID4::new();
        let trade_request = RequestTrades::new(
            instrument_id,
            Some(start),
            Some(end),
            NonZeroUsize::new(1),
            Some(client_id),
            trade_request_id,
            UnixNanos::default(),
            None,
        );
        request_trades_task(
            HistoricalTicksTask {
                http,
                sender,
                instrument,
                start,
                end,
                feed: AlpacaDataFeed::Iex,
                client_id,
            },
            trade_request,
        )
        .await
        .unwrap();
        match receiver.recv().await.unwrap() {
            DataEvent::Response(DataResponse::Trades(response)) => {
                assert_eq!(response.correlation_id, trade_request_id);
                assert_eq!(response.data.len(), 1);
                assert_eq!(response.data[0].trade_id, TradeId::new("123"));
                assert_eq!(response.data[0].price.to_string(), "187.12");
                let params = response.params.unwrap();
                assert_eq!(params.get_str("provider"), Some("alpaca"));
                assert_eq!(params.get_str("feed"), Some("iex"));
            }
            event => panic!("Expected historical trade response, got {event:?}"),
        }
        server.abort();
    }

    #[rstest]
    #[tokio::test(flavor = "current_thread")]
    async fn test_stop_aborts_and_drains_historical_requests() {
        let (data_sender, _data_receiver) = unbounded_channel();
        replace_data_event_sender(data_sender);
        let credential = AlpacaCredential::new("test-key", "test-secret");
        let http = AlpacaRawHttpClient::new(
            credential.clone(),
            Some("http://127.0.0.1:1".to_string()),
            1,
        )
        .unwrap();
        let provider = AlpacaInstrumentProvider::new(http.clone());
        let socket =
            AlpacaWebSocketClient::new("ws://127.0.0.1:1", credential, Default::default(), None);
        let mut client = AlpacaDataClient::new(
            ClientId::from("ALPACA"),
            AlpacaDataClientConfig::default(),
            provider,
            http,
            socket,
        );
        let (dropped_tx, dropped_rx) = tokio::sync::oneshot::channel::<()>();
        client.spawn_request("pending-test", async move {
            std::future::pending::<()>().await;
            let _ = dropped_tx.send(());
        });
        assert_eq!(client.pending_requests.lock().unwrap().len(), 1);

        client.stop().unwrap();
        tokio::task::yield_now().await;

        assert!(client.pending_requests.lock().unwrap().is_empty());
        assert!(dropped_rx.await.is_err());
    }

    #[rstest]
    #[tokio::test]
    async fn test_reap_socket_task_restores_client_for_reconnect() {
        let (data_sender, _data_receiver) = unbounded_channel();
        replace_data_event_sender(data_sender);
        let credential = AlpacaCredential::new("test-key", "test-secret");
        let http = AlpacaRawHttpClient::new(
            credential.clone(),
            Some("http://127.0.0.1:1".to_string()),
            1,
        )
        .unwrap();
        let provider = AlpacaInstrumentProvider::new(http.clone());
        let socket =
            AlpacaWebSocketClient::new("ws://127.0.0.1:1", credential, Default::default(), None);
        let mut client = AlpacaDataClient::new(
            ClientId::from("ALPACA"),
            AlpacaDataClientConfig::default(),
            provider,
            http,
            socket,
        );
        let socket = client.socket.take().unwrap();
        client.socket_task = Some(tokio::spawn(async move {
            tokio::task::yield_now().await;
            socket
        }));
        let (socket_tx, _socket_rx) = unbounded_channel();
        client.socket_tx = Some(socket_tx);

        client.reap_socket_task().await.unwrap();

        assert!(client.socket.is_some());
        assert!(client.socket_task.is_none());
        assert!(client.socket_tx.is_none());
    }
}
