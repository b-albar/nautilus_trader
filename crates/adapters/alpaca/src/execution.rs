// -------------------------------------------------------------------------------------------------
//  Copyright (C) 2015-2026 Nautech Systems Pty Ltd. All rights reserved.
//  https://nautechsystems.io
//
//  Licensed under the GNU Lesser General Public License Version 3.0 (the "License");
// -------------------------------------------------------------------------------------------------

//! Nautilus execution client for Alpaca paper and live equity trading.

use std::{
    collections::{HashMap, HashSet, VecDeque},
    sync::{Arc, Mutex},
};

use async_trait::async_trait;
use nautilus_common::{
    clients::ExecutionClient,
    live::runner::get_exec_event_sender,
    messages::execution::{
        BatchCancelOrders, CancelAllOrders, CancelOrder, GenerateFillReports,
        GenerateOrderStatusReport, GenerateOrderStatusReports, GeneratePositionStatusReports,
        ModifyOrder, QueryAccount, QueryOrder, SubmitOrder, SubmitOrderList,
    },
};
use nautilus_core::{Params, UnixNanos, time::get_atomic_clock_realtime};
use nautilus_live::{ExecutionClientCore, ExecutionEventEmitter};
use nautilus_model::{
    accounts::AccountAny,
    enums::{
        ContingencyType, LiquiditySide, OmsType, OrderSide, OrderStatus, OrderType, PositionSide,
        TimeInForce, TrailingOffsetType,
    },
    events::OrderDeniedReason,
    identifiers::{
        AccountId, ClientId, ClientOrderId, InstrumentId, StrategyId, TradeId, Venue, VenueOrderId,
    },
    instruments::{Instrument, InstrumentAny},
    orders::{Order, OrderAny},
    reports::{FillReport, OrderStatusReport, PositionStatusReport},
    types::{AccountBalance, Currency, MarginBalance, Money},
};
use rust_decimal::Decimal;

use crate::config::AlpacaExecutionClientConfig;
use crate::http::{
    client::AlpacaRawHttpClient,
    models::{
        AlpacaAccount, AlpacaOrder, AlpacaOrderClass, AlpacaOrderRequest, AlpacaOrderSide,
        AlpacaOrderStatus, AlpacaOrderType, AlpacaPosition, AlpacaReplaceOrderRequest,
        AlpacaStopLossRequest, AlpacaTakeProfitRequest, AlpacaTimeInForce, AlpacaTradeActivity,
    },
    query::{AlpacaActivitiesQuery, AlpacaOrdersQuery},
};
use crate::websocket::{
    AlpacaTradeEvent, AlpacaTradeUpdate, AlpacaTradingMessage, AlpacaTradingWebSocketClient,
};

/// Native Alpaca implementation of Nautilus's [`ExecutionClient`] contract.
#[derive(Debug)]
pub struct AlpacaExecutionClient {
    core: ExecutionClientCore,
    config: AlpacaExecutionClientConfig,
    http: AlpacaRawHttpClient,
    emitter: ExecutionEventEmitter,
    pending: Mutex<Vec<tokio::task::JoinHandle<()>>>,
    trading_socket: Option<AlpacaTradingWebSocketClient>,
    trading_stop: Option<tokio::sync::oneshot::Sender<()>>,
    trading_task: Option<tokio::task::JoinHandle<AlpacaTradingWebSocketClient>>,
    order_contexts: Arc<Mutex<HashMap<ClientOrderId, (OrderAny, InstrumentAny)>>>,
}

impl AlpacaExecutionClient {
    #[must_use]
    pub fn new(
        core: ExecutionClientCore,
        config: AlpacaExecutionClientConfig,
        http: AlpacaRawHttpClient,
        trading_socket: AlpacaTradingWebSocketClient,
    ) -> Self {
        let emitter = ExecutionEventEmitter::new(
            get_atomic_clock_realtime(),
            core.trader_id,
            core.account_id,
            core.account_type,
            core.base_currency,
        );
        Self {
            core,
            config,
            http,
            emitter,
            pending: Mutex::new(Vec::new()),
            trading_socket: Some(trading_socket),
            trading_stop: None,
            trading_task: None,
            order_contexts: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    fn spawn(&self, name: &'static str, future: impl Future<Output = ()> + Send + 'static) {
        let task = tokio::spawn(future);
        let mut pending = self.pending.lock().expect("Alpaca task registry poisoned");
        pending.retain(|task| !task.is_finished());
        pending.push(task);
        log::debug!("Spawned Alpaca execution task {name}");
    }

    fn abort_pending(&self) {
        for task in self
            .pending
            .lock()
            .expect("Alpaca task registry poisoned")
            .drain(..)
        {
            task.abort();
        }
    }

    fn stop_trading_stream(&mut self) {
        if let Some(stop) = self.trading_stop.take() {
            let _ = stop.send(());
        }
    }

    async fn request_orders(
        &self,
        open_only: bool,
        instrument_id: Option<nautilus_model::identifiers::InstrumentId>,
        start: Option<UnixNanos>,
        end: Option<UnixNanos>,
    ) -> anyhow::Result<Vec<AlpacaOrder>> {
        let symbol = instrument_id.map(|value| value.symbol.to_string());
        let mut cursor = None::<String>;
        let mut orders = Vec::new();
        loop {
            let first_page = cursor.is_none();
            let query = AlpacaOrdersQuery {
                status: if open_only { "open" } else { "all" },
                limit: 500,
                direction: "desc",
                nested: false,
                after: first_page
                    .then(|| start.map(|value| value.to_datetime_utc()))
                    .flatten(),
                until: first_page
                    .then(|| end.map(|value| value.to_datetime_utc()))
                    .flatten(),
                symbols: symbol.as_deref(),
                before_order_id: cursor.as_deref(),
            };
            let page = self.http.get_orders(&query).await?;
            let page_len = page.len();
            let next_cursor = page.last().map(|order| order.id.clone());
            anyhow::ensure!(
                next_cursor != cursor || page_len < 500,
                "Alpaca order pagination did not advance"
            );
            let crossed_start = start.is_some_and(|bound| {
                page.last().is_some_and(|order| {
                    UnixNanos::from(order.submitted_at.unwrap_or(order.created_at)) <= bound
                })
            });
            cursor = next_cursor;
            orders.extend(page);
            if page_len < 500 || crossed_start {
                break;
            }
        }
        orders.retain(|order| {
            let submitted = UnixNanos::from(order.submitted_at.unwrap_or(order.created_at));
            start.is_none_or(|bound| submitted > bound) && end.is_none_or(|bound| submitted < bound)
        });
        Ok(orders)
    }

    async fn request_fills(
        &self,
        start: Option<UnixNanos>,
        end: Option<UnixNanos>,
        venue_order_id: Option<VenueOrderId>,
    ) -> anyhow::Result<Vec<AlpacaTradeActivity>> {
        let order_id = venue_order_id.map(|value| value.to_string());
        let mut cursor = None::<String>;
        let mut fills = Vec::new();
        loop {
            let query = AlpacaActivitiesQuery {
                direction: "asc",
                page_size: 100,
                after: start.map(|value| value.to_datetime_utc()),
                until: end.map(|value| value.to_datetime_utc()),
                order_id: order_id.as_deref(),
                page_token: cursor.as_deref(),
            };
            let page = self.http.get_fill_activities(&query).await?;
            let page_len = page.len();
            let next_cursor = page.last().map(|fill| fill.id.clone());
            anyhow::ensure!(
                next_cursor != cursor || page_len < 100,
                "Alpaca fill pagination did not advance"
            );
            cursor = next_cursor;
            fills.extend(page);
            if page_len < 100 {
                break;
            }
        }
        Ok(fills)
    }
}

#[async_trait(?Send)]
impl ExecutionClient for AlpacaExecutionClient {
    fn is_connected(&self) -> bool {
        self.core.is_connected()
    }

    fn client_id(&self) -> ClientId {
        self.core.client_id
    }

    fn account_id(&self) -> AccountId {
        self.core.account_id
    }

    fn venue(&self) -> Venue {
        self.core.venue
    }

    fn oms_type(&self) -> OmsType {
        self.core.oms_type
    }

    fn get_account(&self) -> Option<AccountAny> {
        self.core.cache().account_owned(&self.core.account_id)
    }

    fn generate_account_state(
        &self,
        balances: Vec<AccountBalance>,
        margins: Vec<MarginBalance>,
        reported: bool,
        ts_event: UnixNanos,
        info: Option<Params>,
    ) -> anyhow::Result<()> {
        self.emitter
            .emit_account_state(balances, margins, reported, ts_event, info);
        Ok(())
    }

    fn start(&mut self) -> anyhow::Result<()> {
        if !self.core.is_started() {
            self.emitter.set_sender(get_exec_event_sender());
            self.core.set_started();
        }
        Ok(())
    }

    fn stop(&mut self) -> anyhow::Result<()> {
        if self.core.is_stopped() {
            return Ok(());
        }
        self.abort_pending();
        self.stop_trading_stream();
        self.core.set_disconnected();
        self.core.set_stopped();
        Ok(())
    }

    fn reset(&mut self) -> anyhow::Result<()> {
        self.abort_pending();
        self.stop_trading_stream();
        self.core.set_disconnected();
        Ok(())
    }

    fn dispose(&mut self) -> anyhow::Result<()> {
        self.stop()
    }

    async fn connect(&mut self) -> anyhow::Result<()> {
        if self.core.is_connected() {
            return Ok(());
        }
        if let Some(task) = self.trading_task.take() {
            self.trading_socket = Some(task.await?);
        }
        anyhow::ensure!(
            self.emitter.is_initialized(),
            "Alpaca execution client must be started before connect"
        );
        let account = self.http.get_account().await?;
        anyhow::ensure!(
            !account.trading_blocked
                && !account.account_blocked
                && !account.trade_suspended_by_user,
            "Alpaca account {} is blocked or suspended",
            account.account_number
        );
        let (balance, info, ts_event) = account_balance(&account)?;
        let mut socket = self
            .trading_socket
            .take()
            .ok_or_else(|| anyhow::anyhow!("Alpaca trade-update stream is unavailable"))?;
        if let Err(error) = socket.connect().await {
            self.trading_socket = Some(socket);
            return Err(error);
        }
        self.emitter
            .emit_account_state(vec![balance], Vec::new(), true, ts_event, info);
        let (stop_tx, stop_rx) = tokio::sync::oneshot::channel();
        self.trading_stop = Some(stop_tx);
        self.trading_task = Some(tokio::spawn(run_trade_updates(
            socket,
            stop_rx,
            self.emitter.clone(),
            Arc::clone(&self.order_contexts),
            self.http.clone(),
        )));
        self.core.set_connected();
        Ok(())
    }

    async fn disconnect(&mut self) -> anyhow::Result<()> {
        self.abort_pending();
        self.stop_trading_stream();
        if let Some(task) = self.trading_task.take() {
            self.trading_socket = Some(task.await?);
        }
        self.core.set_disconnected();
        Ok(())
    }

    fn query_account(&self, _cmd: QueryAccount) -> anyhow::Result<()> {
        let http = self.http.clone();
        let emitter = self.emitter.clone();
        self.spawn("query_account", async move {
            let result = match http.get_account().await {
                Ok(account) => account_balance(&account),
                Err(error) => Err(error.into()),
            };
            match result {
                Ok((balance, info, ts_event)) => {
                    emitter.emit_account_state(vec![balance], Vec::new(), true, ts_event, info);
                }
                Err(error) => log::error!("Alpaca account query failed: {error}"),
            }
        });
        Ok(())
    }

    fn query_order(&self, cmd: QueryOrder) -> anyhow::Result<()> {
        let instrument = self
            .core
            .cache()
            .instrument(&cmd.instrument_id)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("Missing Alpaca instrument {}", cmd.instrument_id))?;
        let http = self.http.clone();
        let emitter = self.emitter.clone();
        let account_id = self.core.account_id;
        self.spawn("query_order", async move {
            let result = match cmd.venue_order_id {
                Some(venue_order_id) => http.get_order(venue_order_id.as_str()).await,
                None => {
                    http.get_order_by_client_order_id(cmd.client_order_id.as_str())
                        .await
                }
            };
            match result {
                Ok(order) => {
                    match parse_order_report_for_instrument(&order, account_id, &instrument) {
                        Ok(report) => emitter.send_order_status_report(report),
                        Err(error) => log::error!("Failed to parse queried Alpaca order: {error}"),
                    }
                }
                Err(error) => log::error!("Failed to query Alpaca order: {error}"),
            }
        });
        Ok(())
    }

    async fn generate_order_status_report(
        &self,
        cmd: &GenerateOrderStatusReport,
    ) -> anyhow::Result<Option<OrderStatusReport>> {
        let order = if let Some(venue_order_id) = cmd.venue_order_id {
            Some(self.http.get_order(venue_order_id.as_str()).await?)
        } else {
            let orders = self
                .request_orders(false, cmd.instrument_id, None, None)
                .await?;
            orders.into_iter().find(|order| {
                cmd.client_order_id
                    .is_some_and(|value| value.as_str() == order.client_order_id)
            })
        };
        order
            .map(|order| parse_order_report(&order, &self.core))
            .transpose()
    }

    async fn generate_order_status_reports(
        &self,
        cmd: &GenerateOrderStatusReports,
    ) -> anyhow::Result<Vec<OrderStatusReport>> {
        self.request_orders(cmd.open_only, cmd.instrument_id, cmd.start, cmd.end)
            .await?
            .iter()
            .map(|order| parse_order_report(order, &self.core))
            .collect()
    }

    async fn generate_fill_reports(
        &self,
        cmd: GenerateFillReports,
    ) -> anyhow::Result<Vec<FillReport>> {
        let mut fills = self
            .request_fills(cmd.start, cmd.end, cmd.venue_order_id)
            .await?;
        fills.retain(|fill| {
            cmd.instrument_id
                .is_none_or(|id| fill.symbol == id.symbol.as_str())
        });

        let mut orders: HashMap<_, _> = if fills.is_empty() {
            HashMap::new()
        } else if let Some(venue_order_id) = cmd.venue_order_id {
            let order = self.http.get_order(venue_order_id.as_str()).await?;
            HashMap::from([(order.id.clone(), order)])
        } else {
            self.request_orders(false, cmd.instrument_id, cmd.start, cmd.end)
                .await?
                .into_iter()
                .map(|order| (order.id.clone(), order))
                .collect()
        };

        // An order can be submitted before the reconciliation window and fill inside it. Recover
        // only those referenced orders directly instead of expanding the list query to all time.
        let missing_order_ids = missing_fill_order_ids(&fills, &orders);
        for order_id in missing_order_ids {
            let order = self.http.get_order(&order_id).await?;
            orders.insert(order.id.clone(), order);
        }

        fills
            .into_iter()
            .map(|fill| {
                let order = orders.get(&fill.order_id).ok_or_else(|| {
                    anyhow::anyhow!(
                        "Missing Alpaca order {} for fill {}",
                        fill.order_id,
                        fill.id
                    )
                })?;
                parse_fill_report(&fill, order, &self.core)
            })
            .collect()
    }

    async fn generate_position_status_reports(
        &self,
        cmd: &GeneratePositionStatusReports,
    ) -> anyhow::Result<Vec<PositionStatusReport>> {
        self.http
            .get_positions()
            .await?
            .iter()
            .filter(|position| {
                cmd.instrument_id
                    .is_none_or(|id| position.symbol == id.symbol.as_str())
            })
            .map(|position| parse_position_report(position, &self.core))
            .collect()
    }

    fn register_external_order(
        &self,
        client_order_id: ClientOrderId,
        venue_order_id: VenueOrderId,
        instrument_id: InstrumentId,
        _strategy_id: StrategyId,
        _ts_init: UnixNanos,
    ) {
        let cache = self.core.cache();
        let Some(order) = cache.order(&client_order_id) else {
            log::warn!(
                "Cannot register external Alpaca order {client_order_id}/{venue_order_id}: cached order missing"
            );
            return;
        };
        let Some(instrument) = cache.instrument(&instrument_id) else {
            log::warn!(
                "Cannot register external Alpaca order {client_order_id}/{venue_order_id}: instrument {instrument_id} missing"
            );
            return;
        };
        self.order_contexts
            .lock()
            .expect("Alpaca order context cache poisoned")
            .insert(client_order_id, (order.cloned(), instrument.clone()));
    }

    fn submit_order(&self, cmd: SubmitOrder) -> anyhow::Result<()> {
        anyhow::ensure!(
            self.core.is_connected(),
            "Alpaca execution client is disconnected"
        );
        let order = self.core.get_order(&cmd.client_order_id)?;
        if order.is_closed() {
            return Ok(());
        }
        let request = to_alpaca_order(&order, self.venue(), self.config.extended_hours)?;
        let instrument = self
            .core
            .cache()
            .instrument(&order.instrument_id())
            .cloned()
            .ok_or_else(|| {
                anyhow::anyhow!("Missing Alpaca instrument {}", order.instrument_id())
            })?;
        self.order_contexts
            .lock()
            .expect("Alpaca order context cache poisoned")
            .insert(order.client_order_id(), (order.clone(), instrument.clone()));
        self.emitter.emit_order_submitted(&order);
        let http = self.http.clone();
        let emitter = self.emitter.clone();
        let account_id = self.core.account_id;
        self.spawn("submit_order", async move {
            match http.submit_order(&request).await {
                Ok(response) => log::debug!(
                    "Alpaca accepted REST submission {} as {}; awaiting trade_updates",
                    order.client_order_id(),
                    response.id
                ),
                Err(error) if error.is_ambiguous_write() => {
                    log::warn!(
                        "Alpaca submission outcome for {} is ambiguous ({error}); reconciling by client order ID",
                        order.client_order_id()
                    );
                    let mut last_error = None;
                    for attempt in 1..=3 {
                        tokio::time::sleep(std::time::Duration::from_millis(250 * attempt)).await;
                        match http
                            .get_order_by_client_order_id(order.client_order_id().as_str())
                            .await
                        {
                            Ok(response) => {
                                match parse_order_report_for_instrument(
                                    &response,
                                    account_id,
                                    &instrument,
                                ) {
                                    Ok(report) => emitter.send_order_status_report(report),
                                    Err(parse_error) => log::error!(
                                        "Failed to parse reconciled Alpaca order {}: {parse_error}",
                                        order.client_order_id()
                                    ),
                                }
                                log::info!(
                                    "Reconciled ambiguous Alpaca submission {} as {}",
                                    order.client_order_id(),
                                    response.id
                                );
                                return;
                            }
                            Err(query_error) => last_error = Some(query_error),
                        }
                    }
                    log::error!(
                        "Could not reconcile ambiguous Alpaca submission {}: {}. The order remains submitted and must be resolved by trade_updates or reconciliation",
                        order.client_order_id(),
                        last_error.map_or_else(
                            || "order not found".to_string(),
                            |value| value.to_string()
                        )
                    );
                }
                Err(error) => emitter.emit_order_rejected(
                    &order,
                    &error.to_string(),
                    UnixNanos::from(jiff::Timestamp::now()),
                    false,
                ),
            }
        });
        Ok(())
    }

    fn submit_order_list(&self, cmd: SubmitOrderList) -> anyhow::Result<()> {
        let orders = self.core.get_orders_for_list(&cmd.order_list)?;
        let translation =
            match to_alpaca_advanced_order(&orders, self.venue(), self.config.extended_hours) {
                Ok(translation) => translation,
                Err(error) => {
                    let reason = OrderDeniedReason::UnsupportedOrderList {
                        detail: error.to_string(),
                    }
                    .to_string();
                    for order in &orders {
                        self.emitter.emit_order_denied(order, &reason);
                    }
                    return Ok(());
                }
            };

        let primary = orders[translation.primary].clone();
        let children = translation
            .children
            .iter()
            .map(|index| orders[*index].clone())
            .collect::<Vec<_>>();
        let request = translation.request;
        let order_class = translation.label;
        let instrument = self
            .core
            .cache()
            .instrument(&primary.instrument_id())
            .cloned()
            .ok_or_else(|| {
                anyhow::anyhow!("Missing Alpaca instrument {}", primary.instrument_id())
            })?;
        {
            let mut contexts = self
                .order_contexts
                .lock()
                .expect("Alpaca order context cache poisoned");
            for order in &orders {
                contexts.insert(order.client_order_id(), (order.clone(), instrument.clone()));
                self.emitter.emit_order_submitted(order);
            }
        }

        let http = self.http.clone();
        let emitter = self.emitter.clone();
        let contexts = Arc::clone(&self.order_contexts);
        self.spawn("submit_order_list", async move {
            match http.submit_order(&request).await {
                Ok(response) => {
                    let response = if response.legs.is_some() {
                        response
                    } else {
                        match http.get_order_with_legs(&response.id, true).await {
                            Ok(nested) => nested,
                            Err(error) => {
                                log::warn!(
                                    "Could not retrieve nested Alpaca {order_class} {}: {error}",
                                    response.id
                                );
                                response
                            }
                        }
                    };
                    bind_alpaca_leg_contexts(&response, &children, &contexts);
                    log::debug!(
                        "Alpaca accepted {order_class} {} as {}; awaiting trade_updates",
                        primary.client_order_id(),
                        response.id
                    );
                }
                Err(error) if error.is_ambiguous_write() => {
                    log::warn!(
                        "Alpaca {order_class} outcome for {} is ambiguous ({error}); reconciling by client order ID",
                        primary.client_order_id()
                    );
                    match http
                        .get_order_by_client_order_id(primary.client_order_id().as_str())
                        .await
                    {
                        Ok(response) => match http.get_order_with_legs(&response.id, true).await {
                            Ok(nested) => bind_alpaca_leg_contexts(&nested, &children, &contexts),
                            Err(nested_error) => log::error!(
                                "Recovered Alpaca {order_class} {}, but could not retrieve its legs: {nested_error}",
                                response.id
                            ),
                        },
                        Err(query_error) => log::error!(
                            "Could not reconcile ambiguous Alpaca {order_class} {}: {query_error}",
                            primary.client_order_id()
                        ),
                    }
                }
                Err(error) => {
                    let ts = UnixNanos::from(jiff::Timestamp::now());
                    emitter.emit_order_rejected(&primary, &error.to_string(), ts, false);
                    for child in &children {
                        emitter.emit_order_rejected(child, &error.to_string(), ts, false);
                    }
                }
            }
        });
        Ok(())
    }

    fn modify_order(&self, cmd: ModifyOrder) -> anyhow::Result<()> {
        anyhow::ensure!(
            self.core.is_connected(),
            "Alpaca execution client is disconnected"
        );
        let venue_order_id = cmd
            .venue_order_id
            .or_else(|| {
                self.core
                    .cache()
                    .order(&cmd.client_order_id)
                    .and_then(|order| order.venue_order_id())
            })
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "Cannot modify {} without an Alpaca venue order ID",
                    cmd.client_order_id
                )
            })?;
        anyhow::ensure!(
            cmd.quantity.is_some() || cmd.price.is_some() || cmd.trigger_price.is_some(),
            "Alpaca modify requires quantity, price, or trigger price"
        );
        if let Some(quantity) = cmd.quantity {
            let order = self.core.get_order(&cmd.client_order_id)?;
            validate_equity_quantity(quantity.as_decimal())?;
            validate_fractional_time_in_force(quantity.as_decimal(), order.time_in_force())?;
        }
        validate_equity_order_price("limit_price", cmd.price.map(|value| value.as_decimal()))?;
        validate_equity_order_price(
            "stop_price",
            cmd.trigger_price.map(|value| value.as_decimal()),
        )?;
        let request = AlpacaReplaceOrderRequest {
            qty: cmd.quantity.map(|value| value.as_decimal()),
            limit_price: cmd.price.map(|value| value.as_decimal()),
            stop_price: cmd.trigger_price.map(|value| value.as_decimal()),
        };
        let http = self.http.clone();
        let emitter = self.emitter.clone();
        let strategy_id = cmd.strategy_id;
        let instrument_id = cmd.instrument_id;
        let client_order_id = cmd.client_order_id;
        self.spawn("modify_order", async move {
            match http.replace_order(venue_order_id.as_str(), &request).await {
                Ok(response) => log::debug!(
                    "Alpaca accepted replace request for {venue_order_id} as {}; awaiting trade_updates",
                    response.id
                ),
                Err(error) if error.is_ambiguous_write() => log::error!(
                    "Alpaca replace outcome for {venue_order_id} is ambiguous ({error}); awaiting trade_updates or reconciliation"
                ),
                Err(error) => emitter.emit_order_modify_rejected_event(
                    strategy_id,
                    instrument_id,
                    client_order_id,
                    Some(venue_order_id),
                    &error.to_string(),
                    UnixNanos::from(jiff::Timestamp::now()),
                ),
            }
        });
        Ok(())
    }

    fn cancel_order(&self, cmd: CancelOrder) -> anyhow::Result<()> {
        anyhow::ensure!(
            self.core.is_connected(),
            "Alpaca execution client is disconnected"
        );
        let order = self.core.get_order(&cmd.client_order_id)?;
        if !self
            .order_contexts
            .lock()
            .expect("Alpaca order context cache poisoned")
            .contains_key(&cmd.client_order_id)
        {
            let instrument = self
                .core
                .cache()
                .instrument(&order.instrument_id())
                .cloned()
                .ok_or_else(|| {
                    anyhow::anyhow!("Missing Alpaca instrument {}", order.instrument_id())
                })?;
            self.order_contexts
                .lock()
                .expect("Alpaca order context cache poisoned")
                .insert(cmd.client_order_id, (order.clone(), instrument));
        }
        let venue_order_id = cmd
            .venue_order_id
            .or_else(|| order.venue_order_id())
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "Cannot cancel {} without an Alpaca venue order ID",
                    cmd.client_order_id
                )
            })?;
        let http = self.http.clone();
        let emitter = self.emitter.clone();
        self.spawn("cancel_order", async move {
            match http.cancel_order(venue_order_id.as_str()).await {
                Ok(()) => log::debug!(
                    "Alpaca accepted cancel request for {venue_order_id}; awaiting trade_updates"
                ),
                Err(error) if error.is_ambiguous_write() => log::error!(
                    "Alpaca cancel outcome for {venue_order_id} is ambiguous ({error}); awaiting trade_updates or reconciliation"
                ),
                Err(error) => emitter.emit_order_cancel_rejected(
                    &order,
                    Some(venue_order_id),
                    &error.to_string(),
                    UnixNanos::from(jiff::Timestamp::now()),
                ),
            }
        });
        Ok(())
    }

    fn cancel_all_orders(&self, cmd: CancelAllOrders) -> anyhow::Result<()> {
        anyhow::ensure!(
            self.core.is_connected(),
            "Alpaca execution client is disconnected"
        );
        let http = self.http.clone();
        let emitter = self.emitter.clone();
        let symbol = cmd.instrument_id.symbol.to_string();
        let instrument_id = cmd.instrument_id;
        let strategy_id = cmd.strategy_id;
        let side = cmd.order_side;
        self.spawn("cancel_all_orders", async move {
            let mut cursor = None::<String>;
            let mut open_orders = Vec::new();
            loop {
                let query = AlpacaOrdersQuery {
                    status: "open",
                    limit: 500,
                    direction: "desc",
                    nested: false,
                    symbols: Some(&symbol),
                    before_order_id: cursor.as_deref(),
                    ..Default::default()
                };
                let orders = match http.get_orders(&query).await {
                    Ok(orders) => orders,
                    Err(error) => {
                        log::error!(
                            "Alpaca failed to list open {instrument_id} orders for cancel-all: {error}"
                        );
                        return;
                    }
                };
                let page_len = orders.len();
                let next_cursor = orders.last().map(|order| order.id.clone());
                if next_cursor == cursor && page_len == 500 {
                    log::error!("Alpaca cancel-all pagination did not advance for {instrument_id}");
                    return;
                }
                open_orders.extend(orders);
                if page_len < 500 {
                    break;
                }
                cursor = next_cursor;
            }
            for order in open_orders.into_iter().filter(|order| {
                side.is_none_or(|expected| parse_order_side(order.side) == expected)
            }) {
                if let Err(error) = http.cancel_order(&order.id).await {
                    if error.is_ambiguous_write() {
                        log::error!(
                            "Alpaca cancel-all outcome for {} is ambiguous ({error}); awaiting trade_updates or reconciliation",
                            order.id
                        );
                        continue;
                    }
                    emitter.emit_order_cancel_rejected_event(
                        strategy_id,
                        instrument_id,
                        ClientOrderId::new(order.client_order_id.as_str()),
                        Some(VenueOrderId::new(order.id.as_str())),
                        &error.to_string(),
                        UnixNanos::from(jiff::Timestamp::now()),
                    );
                }
            }
        });
        Ok(())
    }

    fn batch_cancel_orders(&self, cmd: BatchCancelOrders) -> anyhow::Result<()> {
        for cancel in cmd.cancels {
            self.cancel_order(cancel)?;
        }
        Ok(())
    }
}

fn missing_fill_order_ids(
    fills: &[AlpacaTradeActivity],
    orders: &HashMap<String, AlpacaOrder>,
) -> HashSet<String> {
    fills
        .iter()
        .map(|fill| fill.order_id.clone())
        .filter(|order_id| !orders.contains_key(order_id))
        .collect()
}

fn cached_instrument(core: &ExecutionClientCore, symbol: &str) -> anyhow::Result<InstrumentAny> {
    let instrument_id =
        nautilus_model::identifiers::InstrumentId::from(format!("{symbol}.{}", core.venue));
    core.cache()
        .instrument(&instrument_id)
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("Missing instrument {instrument_id} for reconciliation"))
}

fn parse_order_report(
    order: &AlpacaOrder,
    core: &ExecutionClientCore,
) -> anyhow::Result<OrderStatusReport> {
    let instrument = cached_instrument(core, &order.symbol)?;
    parse_order_report_for_instrument(order, core.account_id, &instrument)
}

fn parse_order_report_for_instrument(
    order: &AlpacaOrder,
    account_id: AccountId,
    instrument: &InstrumentAny,
) -> anyhow::Result<OrderStatusReport> {
    let ts_init = get_atomic_clock_realtime().get_time_ns();
    let mut report = OrderStatusReport::new(
        account_id,
        instrument.id(),
        Some(ClientOrderId::new(order.client_order_id.as_str())),
        VenueOrderId::new(order.id.as_str()),
        Some(parse_order_side(order.side)),
        parse_order_type(order.order_type),
        parse_time_in_force(order.time_in_force),
        parse_order_status(order.status, order.qty, order.filled_qty)?,
        nautilus_model::types::Quantity::from_decimal_dp(order.qty, instrument.size_precision())?,
        nautilus_model::types::Quantity::from_decimal_dp(
            order.filled_qty,
            instrument.size_precision(),
        )?,
        UnixNanos::from(order.submitted_at.unwrap_or(order.created_at)),
        UnixNanos::from(order.updated_at),
        ts_init,
        None,
    );
    report.price = order
        .limit_price
        .map(|value| {
            nautilus_model::types::Price::from_decimal_dp(value, instrument.price_precision())
        })
        .transpose()?;
    report.trigger_price = order
        .stop_price
        .map(|value| {
            nautilus_model::types::Price::from_decimal_dp(value, instrument.price_precision())
        })
        .transpose()?;
    report.avg_px = order.filled_avg_price;
    Ok(report)
}

fn parse_fill_report(
    fill: &AlpacaTradeActivity,
    order: &AlpacaOrder,
    core: &ExecutionClientCore,
) -> anyhow::Result<FillReport> {
    let instrument = cached_instrument(core, &fill.symbol)?;
    let currency = instrument.quote_currency();
    Ok(FillReport::new(
        core.account_id,
        instrument.id(),
        VenueOrderId::new(fill.order_id.as_str()),
        TradeId::new(fill.id.as_str()),
        parse_order_side(fill.side),
        nautilus_model::types::Quantity::from_decimal_dp(fill.qty, instrument.size_precision())?,
        nautilus_model::types::Price::from_decimal_dp(fill.price, instrument.price_precision())?,
        Money::from_decimal(Decimal::ZERO, currency)?,
        LiquiditySide::NoLiquiditySide,
        Some(ClientOrderId::new(order.client_order_id.as_str())),
        None,
        UnixNanos::from(fill.transaction_time),
        get_atomic_clock_realtime().get_time_ns(),
        None,
    ))
}

fn parse_position_report(
    position: &AlpacaPosition,
    core: &ExecutionClientCore,
) -> anyhow::Result<PositionStatusReport> {
    let instrument = cached_instrument(core, &position.symbol)?;
    let side = match position.side.as_str() {
        "long" => PositionSide::Long,
        "short" => PositionSide::Short,
        value => anyhow::bail!("Unknown Alpaca position side {value}"),
    };
    let ts_init = get_atomic_clock_realtime().get_time_ns();
    Ok(PositionStatusReport::new(
        core.account_id,
        instrument.id(),
        side,
        nautilus_model::types::Quantity::from_decimal_dp(
            position.qty.abs(),
            instrument.size_precision(),
        )?,
        ts_init,
        ts_init,
        None,
        None,
        Some(position.avg_entry_price),
    ))
}

const fn parse_order_side(side: AlpacaOrderSide) -> OrderSide {
    match side {
        AlpacaOrderSide::Buy => OrderSide::Buy,
        AlpacaOrderSide::Sell => OrderSide::Sell,
    }
}

const fn parse_order_type(order_type: AlpacaOrderType) -> OrderType {
    match order_type {
        AlpacaOrderType::Market => OrderType::Market,
        AlpacaOrderType::Limit => OrderType::Limit,
        AlpacaOrderType::Stop => OrderType::StopMarket,
        AlpacaOrderType::StopLimit => OrderType::StopLimit,
        AlpacaOrderType::TrailingStop => OrderType::TrailingStopMarket,
    }
}

const fn parse_time_in_force(value: AlpacaTimeInForce) -> TimeInForce {
    match value {
        AlpacaTimeInForce::Day => TimeInForce::Day,
        AlpacaTimeInForce::Gtc => TimeInForce::Gtc,
        AlpacaTimeInForce::Opg => TimeInForce::AtTheOpen,
        AlpacaTimeInForce::Cls => TimeInForce::AtTheClose,
        AlpacaTimeInForce::Ioc => TimeInForce::Ioc,
        AlpacaTimeInForce::Fok => TimeInForce::Fok,
    }
}

fn parse_order_status(
    value: AlpacaOrderStatus,
    quantity: Decimal,
    filled_qty: Decimal,
) -> anyhow::Result<OrderStatus> {
    Ok(match value {
        AlpacaOrderStatus::New
        | AlpacaOrderStatus::Accepted
        | AlpacaOrderStatus::AcceptedForBidding
        | AlpacaOrderStatus::Held => OrderStatus::Accepted,
        AlpacaOrderStatus::PartiallyFilled => OrderStatus::PartiallyFilled,
        AlpacaOrderStatus::Filled => OrderStatus::Filled,
        AlpacaOrderStatus::Canceled | AlpacaOrderStatus::Replaced => OrderStatus::Canceled,
        AlpacaOrderStatus::Expired => OrderStatus::Expired,
        AlpacaOrderStatus::PendingCancel => OrderStatus::PendingCancel,
        AlpacaOrderStatus::PendingReplace => OrderStatus::PendingUpdate,
        AlpacaOrderStatus::PendingNew => OrderStatus::Submitted,
        AlpacaOrderStatus::Stopped => OrderStatus::Triggered,
        AlpacaOrderStatus::Rejected | AlpacaOrderStatus::Suspended => OrderStatus::Rejected,
        AlpacaOrderStatus::DoneForDay | AlpacaOrderStatus::Calculated => {
            if filled_qty >= quantity {
                OrderStatus::Filled
            } else {
                OrderStatus::Canceled
            }
        }
        AlpacaOrderStatus::Unknown => anyhow::bail!("Unknown Alpaca order status"),
    })
}

async fn run_trade_updates(
    mut socket: AlpacaTradingWebSocketClient,
    mut stop: tokio::sync::oneshot::Receiver<()>,
    emitter: ExecutionEventEmitter,
    order_contexts: Arc<Mutex<HashMap<ClientOrderId, (OrderAny, InstrumentAny)>>>,
    http: AlpacaRawHttpClient,
) -> AlpacaTradingWebSocketClient {
    let mut fill_ids = HashSet::new();
    let mut fill_order = VecDeque::new();
    let mut reconnect_delay = std::time::Duration::from_millis(500);
    let mut account_refreshes = tokio::task::JoinSet::new();
    let mut account_refresh_pending = false;
    loop {
        tokio::select! {
            _ = &mut stop => break,
            Some(result) = account_refreshes.join_next(), if !account_refreshes.is_empty() => {
                if let Err(error) = result {
                    log::warn!("Alpaca account refresh task failed: {error}");
                }
                if account_refresh_pending {
                    account_refresh_pending = false;
                    spawn_account_refresh(&mut account_refreshes, http.clone(), emitter.clone());
                }
            }
            message = socket.next_message() => match message {
                Some(Ok(AlpacaTradingMessage::TradeUpdate(update))) => {
                    reconnect_delay = std::time::Duration::from_millis(500);
                    match handle_trade_update(&update, &order_contexts, &emitter, &mut fill_ids, &mut fill_order) {
                        Ok(true) => queue_account_refresh(
                                &mut account_refreshes,
                                &mut account_refresh_pending,
                                http.clone(),
                                emitter.clone(),
                            ),
                        Ok(false) => {}
                        Err(error) => log::error!("Failed to handle Alpaca trade update: {error}"),
                    }
                }
                Some(Ok(AlpacaTradingMessage::Unknown)) => log::error!(
                    "Alpaca sent an unsupported private-stream message type; account/order state may require reconciliation"
                ),
                Some(Ok(_)) => {}
                Some(Err(error)) => {
                    log::error!("Alpaca trade-update stream failed: {error}");
                    socket.disconnect().await;
                    if !reconnect_trade_updates(&mut socket, &mut stop, &mut reconnect_delay).await {
                        break;
                    }
                    queue_account_refresh(
                        &mut account_refreshes,
                        &mut account_refresh_pending,
                        http.clone(),
                        emitter.clone(),
                    );
                }
                None => {
                    log::warn!("Alpaca trade-update stream ended; reconnecting");
                    socket.disconnect().await;
                    if !reconnect_trade_updates(&mut socket, &mut stop, &mut reconnect_delay).await {
                        break;
                    }
                    queue_account_refresh(
                        &mut account_refreshes,
                        &mut account_refresh_pending,
                        http.clone(),
                        emitter.clone(),
                    );
                }
            }
        }
    }
    account_refreshes.abort_all();
    while account_refreshes.join_next().await.is_some() {}
    socket.disconnect().await;
    socket
}

fn queue_account_refresh(
    refreshes: &mut tokio::task::JoinSet<()>,
    pending: &mut bool,
    http: AlpacaRawHttpClient,
    emitter: ExecutionEventEmitter,
) {
    if refreshes.is_empty() {
        spawn_account_refresh(refreshes, http, emitter);
    } else {
        *pending = true;
    }
}

fn spawn_account_refresh(
    refreshes: &mut tokio::task::JoinSet<()>,
    http: AlpacaRawHttpClient,
    emitter: ExecutionEventEmitter,
) {
    refreshes.spawn(async move {
        let result = match http.get_account().await {
            Ok(account) => account_balance(&account),
            Err(error) => Err(error.into()),
        };
        match result {
            Ok((balance, info, ts_event)) => {
                emitter.emit_account_state(vec![balance], Vec::new(), true, ts_event, info);
            }
            Err(error) => log::warn!("Failed to refresh Alpaca account state: {error}"),
        }
    });
}

async fn reconnect_trade_updates(
    socket: &mut AlpacaTradingWebSocketClient,
    stop: &mut tokio::sync::oneshot::Receiver<()>,
    delay: &mut std::time::Duration,
) -> bool {
    loop {
        tokio::select! {
            _ = &mut *stop => return false,
            () = tokio::time::sleep(*delay) => {}
        }
        let result = tokio::select! {
            _ = &mut *stop => return false,
            result = socket.connect() => result,
        };
        match result {
            Ok(()) => {
                log::info!("Reconnected Alpaca trade-update stream");
                return true;
            }
            Err(error) => {
                log::error!("Failed to reconnect Alpaca trade-update stream: {error}");
                socket.disconnect().await;
                *delay = (*delay * 2).min(std::time::Duration::from_secs(30));
            }
        }
    }
}

fn handle_trade_update(
    update: &AlpacaTradeUpdate,
    order_contexts: &Arc<Mutex<HashMap<ClientOrderId, (OrderAny, InstrumentAny)>>>,
    emitter: &ExecutionEventEmitter,
    fill_ids: &mut HashSet<String>,
    fill_order: &mut VecDeque<String>,
) -> anyhow::Result<bool> {
    let client_order_id = ClientOrderId::new(update.order.client_order_id.as_str());
    let (order, instrument) = order_contexts
        .lock()
        .expect("Alpaca order context cache poisoned")
        .get(&client_order_id)
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("No local context for Alpaca order {client_order_id}"))?;
    let venue_order_id = VenueOrderId::new(update.order.id.as_str());
    let ts_event = update
        .timestamp
        .or(update.order.filled_at)
        .unwrap_or(update.order.updated_at);
    let ts_event = UnixNanos::from(ts_event);

    match update.event {
        AlpacaTradeEvent::Accepted | AlpacaTradeEvent::New => {
            if order.venue_order_id().is_none() {
                emitter.emit_order_accepted(&order, venue_order_id, ts_event);
            }
        }
        AlpacaTradeEvent::Fill | AlpacaTradeEvent::PartialFill => {
            let execution_id = update
                .execution_id
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("Alpaca fill has no execution_id"))?;
            if !fill_ids.insert(execution_id.clone()) {
                return Ok(false);
            }
            fill_order.push_back(execution_id.clone());
            if fill_order.len() > 10_000
                && let Some(oldest) = fill_order.pop_front()
            {
                fill_ids.remove(&oldest);
            }
            let price = update
                .price
                .ok_or_else(|| anyhow::anyhow!("Alpaca fill has no price"))?;
            let quantity = update
                .qty
                .ok_or_else(|| anyhow::anyhow!("Alpaca fill has no quantity"))?;
            emitter.emit_order_filled(
                &order,
                venue_order_id,
                None,
                TradeId::new(execution_id),
                nautilus_model::types::Quantity::from_decimal_dp(
                    quantity,
                    instrument.size_precision(),
                )?,
                nautilus_model::types::Price::from_decimal_dp(price, instrument.price_precision())?,
                instrument.quote_currency(),
                None,
                LiquiditySide::NoLiquiditySide,
                ts_event,
            );
            if update.event == AlpacaTradeEvent::Fill {
                order_contexts
                    .lock()
                    .expect("Alpaca order context cache poisoned")
                    .remove(&client_order_id);
            }
        }
        AlpacaTradeEvent::Canceled => {
            emitter.emit_order_canceled(&order, Some(venue_order_id), ts_event);
            order_contexts
                .lock()
                .expect("Alpaca order context cache poisoned")
                .remove(&client_order_id);
        }
        AlpacaTradeEvent::Expired => {
            emitter.emit_order_expired(&order, Some(venue_order_id), ts_event);
            order_contexts
                .lock()
                .expect("Alpaca order context cache poisoned")
                .remove(&client_order_id);
        }
        AlpacaTradeEvent::Rejected => {
            emitter.emit_order_rejected(&order, "Rejected by Alpaca", ts_event, false);
            order_contexts
                .lock()
                .expect("Alpaca order context cache poisoned")
                .remove(&client_order_id);
        }
        AlpacaTradeEvent::OrderCancelRejected => {
            emitter.emit_order_cancel_rejected(
                &order,
                Some(venue_order_id),
                "Cancel rejected by Alpaca",
                ts_event,
            );
        }
        AlpacaTradeEvent::Stopped => {
            emitter.emit_order_triggered(&order, Some(venue_order_id), ts_event);
        }
        AlpacaTradeEvent::Replaced => {
            emitter.emit_order_updated(
                &order,
                venue_order_id,
                nautilus_model::types::Quantity::from_decimal_dp(
                    update.order.qty,
                    instrument.size_precision(),
                )?,
                update
                    .order
                    .limit_price
                    .map(|value| {
                        nautilus_model::types::Price::from_decimal_dp(
                            value,
                            instrument.price_precision(),
                        )
                    })
                    .transpose()?,
                update
                    .order
                    .stop_price
                    .map(|value| {
                        nautilus_model::types::Price::from_decimal_dp(
                            value,
                            instrument.price_precision(),
                        )
                    })
                    .transpose()?,
                None,
                ts_event,
            );
        }
        AlpacaTradeEvent::OrderReplaceRejected => {
            emitter.emit_order_modify_rejected(
                &order,
                Some(venue_order_id),
                "Replace rejected by Alpaca",
                ts_event,
            );
        }
        AlpacaTradeEvent::DoneForDay
        | AlpacaTradeEvent::PendingNew
        | AlpacaTradeEvent::PendingCancel
        | AlpacaTradeEvent::PendingReplace
        | AlpacaTradeEvent::Calculated
        | AlpacaTradeEvent::Suspended
        | AlpacaTradeEvent::Unknown => {
            log::debug!("Ignoring non-terminal Alpaca event {:?}", update.event);
        }
    }
    Ok(matches!(
        update.event,
        AlpacaTradeEvent::Accepted
            | AlpacaTradeEvent::New
            | AlpacaTradeEvent::Fill
            | AlpacaTradeEvent::PartialFill
            | AlpacaTradeEvent::Canceled
            | AlpacaTradeEvent::Expired
            | AlpacaTradeEvent::Rejected
            | AlpacaTradeEvent::Replaced
    ))
}

fn account_balance(
    account: &AlpacaAccount,
) -> anyhow::Result<(AccountBalance, Option<Params>, UnixNanos)> {
    let currency = Currency::from(account.currency.as_str());
    let total = Money::from_decimal(account.equity, currency)?;
    let free = Money::from_decimal(account.cash, currency)?;
    let locked = Money::from_decimal(account.equity - account.cash, currency)?;
    let balance = AccountBalance::new_checked(total, locked, free)?;
    let mut info = Params::new();
    for (key, value) in [
        ("cash", account.cash),
        ("buying_power", account.buying_power),
        ("equity", account.equity),
        ("portfolio_value", account.portfolio_value),
        ("long_market_value", account.long_market_value),
        ("short_market_value", account.short_market_value),
        ("multiplier", account.multiplier),
    ] {
        // Preserve provider precision for downstream dashboards, audit logs, and assistants.
        info.insert(
            key.to_string(),
            serde_json::Value::String(value.to_string()),
        );
    }
    info.insert(
        "status".to_string(),
        serde_json::Value::String(account.status.clone()),
    );
    for (key, value) in [
        ("regt_buying_power", account.regt_buying_power),
        (
            "non_marginable_buying_power",
            account.non_marginable_buying_power,
        ),
        ("initial_margin", account.initial_margin),
        ("maintenance_margin", account.maintenance_margin),
        ("last_equity", account.last_equity),
        ("last_maintenance_margin", account.last_maintenance_margin),
        ("sma", account.sma),
        ("accrued_fees", account.accrued_fees),
        ("pending_transfer_in", account.pending_transfer_in),
        ("options_buying_power", account.options_buying_power),
    ] {
        if let Some(value) = value {
            info.insert(
                key.to_string(),
                serde_json::Value::String(value.to_string()),
            );
        }
    }
    if let Some(value) = account.pattern_day_trader {
        info.insert(
            "pattern_day_trader".to_string(),
            serde_json::Value::Bool(value),
        );
    }
    for (key, value) in [
        ("options_approved_level", account.options_approved_level),
        ("options_trading_level", account.options_trading_level),
    ] {
        if let Some(value) = value {
            info.insert(key.to_string(), serde_json::Value::from(value));
        }
    }
    if let Some(value) = &account.crypto_status {
        info.insert(
            "crypto_status".to_string(),
            serde_json::Value::String(value.clone()),
        );
    }
    for (key, value) in [
        ("trading_blocked", account.trading_blocked),
        ("transfers_blocked", account.transfers_blocked),
        ("account_blocked", account.account_blocked),
        ("trade_suspended_by_user", account.trade_suspended_by_user),
        ("shorting_enabled", account.shorting_enabled),
    ] {
        info.insert(key.to_string(), serde_json::Value::Bool(value));
    }
    Ok((balance, Some(info), UnixNanos::from(jiff::Timestamp::now())))
}

fn to_alpaca_order(
    order: &OrderAny,
    venue: Venue,
    extended_hours: bool,
) -> anyhow::Result<AlpacaOrderRequest> {
    to_alpaca_order_inner(order, venue, extended_hours, false)
}

fn to_alpaca_order_inner(
    order: &OrderAny,
    venue: Venue,
    extended_hours: bool,
    advanced_leg: bool,
) -> anyhow::Result<AlpacaOrderRequest> {
    anyhow::ensure!(
        order.instrument_id().venue == venue,
        "Alpaca cannot route instrument {}",
        order.instrument_id()
    );
    anyhow::ensure!(
        !order.is_quote_quantity(),
        "Alpaca adapter requires base/share quantity orders"
    );
    if !advanced_leg {
        anyhow::ensure!(
            !order.is_post_only(),
            "Alpaca Trading API does not expose post-only equity orders"
        );
        anyhow::ensure!(
            !order.is_reduce_only(),
            "Alpaca Trading API does not expose reduce-only equity orders"
        );
    }
    anyhow::ensure!(
        order.display_qty().is_none(),
        "Alpaca Trading API does not expose iceberg/display quantity"
    );
    if !advanced_leg {
        anyhow::ensure!(
            order.contingency_type().is_none(),
            "Alpaca contingent orders require explicit bracket/OCO translation"
        );
    }
    anyhow::ensure!(
        order.client_order_id().as_str().len() <= 128,
        "Alpaca client_order_id exceeds 128 characters"
    );

    let side = match order.order_side() {
        OrderSide::Buy => AlpacaOrderSide::Buy,
        OrderSide::Sell => AlpacaOrderSide::Sell,
    };
    let order_type = match order.order_type() {
        OrderType::Market => AlpacaOrderType::Market,
        OrderType::Limit => AlpacaOrderType::Limit,
        OrderType::StopMarket => AlpacaOrderType::Stop,
        OrderType::StopLimit => AlpacaOrderType::StopLimit,
        OrderType::TrailingStopMarket => AlpacaOrderType::TrailingStop,
        value => anyhow::bail!("Alpaca does not support Nautilus order type {value}"),
    };
    let time_in_force = match order.time_in_force() {
        TimeInForce::Day => AlpacaTimeInForce::Day,
        TimeInForce::Gtc => AlpacaTimeInForce::Gtc,
        TimeInForce::Ioc => AlpacaTimeInForce::Ioc,
        TimeInForce::Fok => AlpacaTimeInForce::Fok,
        TimeInForce::AtTheOpen => AlpacaTimeInForce::Opg,
        TimeInForce::AtTheClose => AlpacaTimeInForce::Cls,
        TimeInForce::Gtd => anyhow::bail!("Alpaca equities do not support GTD orders"),
    };
    validate_equity_quantity(order.quantity().as_decimal())?;
    validate_fractional_time_in_force(order.quantity().as_decimal(), order.time_in_force())?;
    if extended_hours {
        anyhow::ensure!(
            order_type == AlpacaOrderType::Limit
                && matches!(
                    time_in_force,
                    AlpacaTimeInForce::Day | AlpacaTimeInForce::Gtc
                ),
            "Alpaca extended-hours orders require LIMIT with DAY or GTC"
        );
    }
    let (trail_price, trail_percent) = match (order.trailing_offset(), order.trailing_offset_type())
    {
        (Some(offset), Some(TrailingOffsetType::Price)) => (Some(offset), None),
        (Some(offset), Some(TrailingOffsetType::BasisPoints)) => {
            (None, Some(offset / Decimal::from(100)))
        }
        (None, _) => (None, None),
        _ => anyhow::bail!("Unsupported Alpaca trailing offset type"),
    };
    if order_type == AlpacaOrderType::TrailingStop {
        anyhow::ensure!(
            trail_price.is_some() || trail_percent.is_some(),
            "Alpaca trailing stops require a price or percent offset"
        );
    }
    if matches!(
        order_type,
        AlpacaOrderType::Limit | AlpacaOrderType::StopLimit
    ) {
        anyhow::ensure!(
            order.price().is_some(),
            "Alpaca limit orders require a limit price"
        );
    }
    if matches!(
        order_type,
        AlpacaOrderType::Stop | AlpacaOrderType::StopLimit
    ) {
        anyhow::ensure!(
            order.trigger_price().is_some(),
            "Alpaca stop orders require a stop price"
        );
    }
    validate_equity_order_price("limit_price", order.price().map(|value| value.as_decimal()))?;
    validate_equity_order_price(
        "stop_price",
        order.trigger_price().map(|value| value.as_decimal()),
    )?;

    Ok(AlpacaOrderRequest {
        symbol: order.instrument_id().symbol.to_string(),
        qty: order.quantity().as_decimal(),
        side,
        order_type,
        time_in_force,
        client_order_id: order.client_order_id().to_string(),
        limit_price: order.price().map(|value| value.as_decimal()),
        stop_price: order.trigger_price().map(|value| value.as_decimal()),
        trail_price,
        trail_percent,
        extended_hours,
        order_class: None,
        take_profit: None,
        stop_loss: None,
    })
}

struct AlpacaAdvancedOrder {
    request: AlpacaOrderRequest,
    primary: usize,
    children: Vec<usize>,
    label: &'static str,
}

fn to_alpaca_advanced_order(
    orders: &[OrderAny],
    venue: Venue,
    extended_hours: bool,
) -> anyhow::Result<AlpacaAdvancedOrder> {
    if orders.len() == 3 {
        return Ok(AlpacaAdvancedOrder {
            request: to_alpaca_bracket_order(orders, venue, extended_hours)?,
            primary: 0,
            children: vec![1, 2],
            label: "bracket",
        });
    }
    anyhow::ensure!(
        orders.len() == 2,
        "Alpaca order lists require two or three orders"
    );
    if orders[0].contingency_type() == Some(ContingencyType::Oto) {
        return to_alpaca_oto_order(orders, venue, extended_hours);
    }
    to_alpaca_oco_order(orders, venue, extended_hours)
}

fn to_alpaca_bracket_order(
    orders: &[OrderAny],
    venue: Venue,
    extended_hours: bool,
) -> anyhow::Result<AlpacaOrderRequest> {
    anyhow::ensure!(
        orders.len() == 3,
        "Alpaca bracket orders require exactly three legs"
    );
    anyhow::ensure!(
        !extended_hours,
        "Alpaca bracket orders do not support extended hours"
    );

    let entry = &orders[0];
    anyhow::ensure!(
        entry.parent_order_id().is_none() && entry.contingency_type() == Some(ContingencyType::Oto),
        "Alpaca bracket entry must be the first OTO parent"
    );
    anyhow::ensure!(
        matches!(entry.order_type(), OrderType::Market | OrderType::Limit),
        "Alpaca bracket entries require MARKET or LIMIT"
    );
    anyhow::ensure!(
        !entry.is_post_only() && !entry.is_reduce_only(),
        "Alpaca bracket entries cannot be post-only or reduce-only"
    );
    anyhow::ensure!(
        matches!(entry.time_in_force(), TimeInForce::Day | TimeInForce::Gtc),
        "Alpaca bracket orders require DAY or GTC"
    );

    let mut take_profit = None;
    let mut stop_loss = None;
    for child in &orders[1..] {
        anyhow::ensure!(
            child.parent_order_id() == Some(entry.client_order_id()),
            "Alpaca bracket child {} has the wrong parent",
            child.client_order_id()
        );
        anyhow::ensure!(
            child.instrument_id() == entry.instrument_id()
                && child.order_side() != entry.order_side()
                && child.quantity() == entry.quantity()
                && child.time_in_force() == entry.time_in_force(),
            "Alpaca bracket legs must share instrument, quantity, and time in force with opposing exit sides"
        );
        anyhow::ensure!(
            child.contingency_type() == Some(ContingencyType::Ouo),
            "Alpaca bracket exits require OUO contingency for proportional quantity updates"
        );
        anyhow::ensure!(
            child.is_reduce_only(),
            "Alpaca bracket exits must be reduce-only in the Nautilus model"
        );
        match child.order_type() {
            OrderType::Limit => {
                anyhow::ensure!(
                    take_profit.is_none(),
                    "Alpaca bracket has multiple take-profit legs"
                );
                take_profit = Some(AlpacaTakeProfitRequest {
                    limit_price: child
                        .price()
                        .ok_or_else(|| {
                            anyhow::anyhow!("Alpaca take-profit requires a limit price")
                        })?
                        .as_decimal(),
                });
            }
            OrderType::StopMarket | OrderType::StopLimit => {
                anyhow::ensure!(
                    stop_loss.is_none(),
                    "Alpaca bracket has multiple stop-loss legs"
                );
                stop_loss = Some(AlpacaStopLossRequest {
                    stop_price: child
                        .trigger_price()
                        .ok_or_else(|| anyhow::anyhow!("Alpaca stop-loss requires a stop price"))?
                        .as_decimal(),
                    limit_price: child.price().map(|price| price.as_decimal()),
                });
            }
            value => anyhow::bail!("Alpaca bracket does not support {value} exit legs"),
        }
    }
    anyhow::ensure!(
        take_profit.is_some() && stop_loss.is_some(),
        "Alpaca bracket requires one take-profit and one stop-loss"
    );

    let mut request = to_alpaca_order_inner(entry, venue, false, true)?;
    request.order_class = Some(AlpacaOrderClass::Bracket);
    request.take_profit = take_profit;
    request.stop_loss = stop_loss;
    validate_alpaca_nested_prices(&request)?;
    Ok(request)
}

fn to_alpaca_oto_order(
    orders: &[OrderAny],
    venue: Venue,
    extended_hours: bool,
) -> anyhow::Result<AlpacaAdvancedOrder> {
    anyhow::ensure!(
        !extended_hours,
        "Alpaca OTO orders do not support extended hours"
    );
    let parent = &orders[0];
    let child = &orders[1];
    anyhow::ensure!(
        parent.parent_order_id().is_none()
            && parent.contingency_type() == Some(ContingencyType::Oto)
            && child.parent_order_id() == Some(parent.client_order_id()),
        "Alpaca OTO requires a parent followed by its child"
    );
    anyhow::ensure!(
        matches!(parent.order_type(), OrderType::Market | OrderType::Limit)
            && matches!(parent.time_in_force(), TimeInForce::Day | TimeInForce::Gtc)
            && !parent.is_post_only()
            && !parent.is_reduce_only(),
        "Alpaca OTO parents require a non-reducing MARKET or LIMIT order with DAY or GTC"
    );
    validate_advanced_exit(parent, child)?;
    let mut request = to_alpaca_order_inner(parent, venue, false, true)?;
    request.order_class = Some(AlpacaOrderClass::Oto);
    set_advanced_exit(&mut request, child)?;
    validate_alpaca_nested_prices(&request)?;
    Ok(AlpacaAdvancedOrder {
        request,
        primary: 0,
        children: vec![1],
        label: "OTO",
    })
}

fn to_alpaca_oco_order(
    orders: &[OrderAny],
    venue: Venue,
    extended_hours: bool,
) -> anyhow::Result<AlpacaAdvancedOrder> {
    anyhow::ensure!(
        !extended_hours,
        "Alpaca OCO orders do not support extended hours"
    );
    let profit_index = orders
        .iter()
        .position(|order| order.order_type() == OrderType::Limit)
        .ok_or_else(|| anyhow::anyhow!("Alpaca OCO requires a LIMIT take-profit"))?;
    let stop_index = 1 - profit_index;
    let profit = &orders[profit_index];
    let stop = &orders[stop_index];
    anyhow::ensure!(
        orders.iter().all(|order| {
            order.parent_order_id().is_none()
                && order.contingency_type() == Some(ContingencyType::Oco)
                && order.is_reduce_only()
        }),
        "Alpaca OCO legs require parentless OCO reducing orders"
    );
    anyhow::ensure!(
        matches!(
            stop.order_type(),
            OrderType::StopMarket | OrderType::StopLimit
        ),
        "Alpaca OCO requires one stop-loss leg"
    );
    anyhow::ensure!(
        profit.instrument_id() == stop.instrument_id()
            && profit.order_side() == stop.order_side()
            && profit.quantity() == stop.quantity()
            && profit.time_in_force() == stop.time_in_force()
            && matches!(profit.time_in_force(), TimeInForce::Day | TimeInForce::Gtc),
        "Alpaca OCO legs must share instrument, side, quantity, and DAY/GTC time in force"
    );
    let mut request = to_alpaca_order_inner(profit, venue, false, true)?;
    request.limit_price = None;
    request.order_class = Some(AlpacaOrderClass::Oco);
    request.take_profit = Some(AlpacaTakeProfitRequest {
        limit_price: profit.price().expect("validated LIMIT order").as_decimal(),
    });
    request.stop_loss = Some(AlpacaStopLossRequest {
        stop_price: stop
            .trigger_price()
            .ok_or_else(|| anyhow::anyhow!("Alpaca OCO stop-loss requires a stop price"))?
            .as_decimal(),
        limit_price: stop.price().map(|price| price.as_decimal()),
    });
    validate_alpaca_nested_prices(&request)?;
    Ok(AlpacaAdvancedOrder {
        request,
        primary: profit_index,
        children: vec![stop_index],
        label: "OCO",
    })
}

fn validate_advanced_exit(parent: &OrderAny, child: &OrderAny) -> anyhow::Result<()> {
    anyhow::ensure!(
        child.instrument_id() == parent.instrument_id()
            && child.order_side() != parent.order_side()
            && child.quantity() == parent.quantity()
            && child.time_in_force() == parent.time_in_force()
            && child.is_reduce_only(),
        "Alpaca OTO child must be an opposing reducing order with matching instrument, quantity, and time in force"
    );
    Ok(())
}

fn set_advanced_exit(request: &mut AlpacaOrderRequest, child: &OrderAny) -> anyhow::Result<()> {
    match child.order_type() {
        OrderType::Limit => {
            request.take_profit = Some(AlpacaTakeProfitRequest {
                limit_price: child
                    .price()
                    .ok_or_else(|| anyhow::anyhow!("Alpaca take-profit requires a limit price"))?
                    .as_decimal(),
            });
        }
        OrderType::StopMarket | OrderType::StopLimit => {
            request.stop_loss = Some(AlpacaStopLossRequest {
                stop_price: child
                    .trigger_price()
                    .ok_or_else(|| anyhow::anyhow!("Alpaca stop-loss requires a stop price"))?
                    .as_decimal(),
                limit_price: child.price().map(|price| price.as_decimal()),
            });
        }
        value => anyhow::bail!("Alpaca OTO does not support {value} child orders"),
    }
    Ok(())
}

fn bind_alpaca_leg_contexts(
    response: &AlpacaOrder,
    children: &[OrderAny],
    contexts: &Arc<Mutex<HashMap<ClientOrderId, (OrderAny, InstrumentAny)>>>,
) {
    let mut contexts = contexts
        .lock()
        .expect("Alpaca order context cache poisoned");
    for leg in response.legs.iter().flatten() {
        let child = match leg.order_type {
            AlpacaOrderType::Limit => children
                .iter()
                .find(|order| order.order_type() == OrderType::Limit),
            AlpacaOrderType::Stop | AlpacaOrderType::StopLimit => children.iter().find(|order| {
                matches!(
                    order.order_type(),
                    OrderType::StopMarket | OrderType::StopLimit
                )
            }),
            _ => None,
        };
        if let Some(child) = child
            && let Some((_, instrument)) = contexts.get(&child.client_order_id()).cloned()
        {
            contexts.insert(
                ClientOrderId::new(leg.client_order_id.as_str()),
                (child.clone(), instrument),
            );
        }
    }
}

fn validate_fractional_time_in_force(
    quantity: Decimal,
    time_in_force: TimeInForce,
) -> anyhow::Result<()> {
    if !quantity.fract().is_zero() {
        anyhow::ensure!(
            time_in_force == TimeInForce::Day,
            "Alpaca fractional-share orders require DAY time in force"
        );
    }
    Ok(())
}

fn validate_equity_quantity(quantity: Decimal) -> anyhow::Result<()> {
    anyhow::ensure!(
        quantity > Decimal::ZERO,
        "Alpaca order quantity must be positive"
    );
    anyhow::ensure!(
        quantity.normalize().scale() <= 9,
        "Alpaca equity quantity accepts at most 9 decimal places"
    );
    Ok(())
}

fn validate_equity_order_price(label: &str, price: Option<Decimal>) -> anyhow::Result<()> {
    let Some(price) = price else {
        return Ok(());
    };
    anyhow::ensure!(price > Decimal::ZERO, "Alpaca {label} must be positive");
    let maximum_scale = if price >= Decimal::ONE { 2 } else { 4 };
    anyhow::ensure!(
        price.normalize().scale() <= maximum_scale,
        "Alpaca {label} accepts at most {maximum_scale} decimals at price {price}"
    );
    Ok(())
}

fn validate_alpaca_nested_prices(request: &AlpacaOrderRequest) -> anyhow::Result<()> {
    if let Some(take_profit) = &request.take_profit {
        validate_equity_order_price("take_profit.limit_price", Some(take_profit.limit_price))?;
    }
    if let Some(stop_loss) = &request.stop_loss {
        validate_equity_order_price("stop_loss.stop_price", Some(stop_loss.stop_price))?;
        validate_equity_order_price("stop_loss.limit_price", stop_loss.limit_price)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use axum::{Json, Router, routing::get};
    use futures_util::{SinkExt, StreamExt};
    use nautilus_common::messages::ExecutionEvent;
    use nautilus_live::ExecutionEventEmitter;
    use nautilus_model::{
        enums::{AccountType, ContingencyType, OrderSide, OrderType, TimeInForce},
        events::OrderEventAny,
        identifiers::{ClientOrderId, InstrumentId},
        instruments::stubs::equity_aapl,
        orders::builder::OrderTestBuilder,
        types::{Price, Quantity},
    };
    use rstest::rstest;
    use serde_json::{Value, json};
    use tokio::net::TcpListener;
    use tokio_tungstenite::{accept_async, tungstenite::Message};

    use super::*;
    use crate::common::credential::AlpacaCredential;

    #[rstest]
    fn test_translates_native_limit_order_without_float_conversion() {
        let order = OrderTestBuilder::new(OrderType::Limit)
            .instrument_id(InstrumentId::from("AAPL.ALPACA"))
            .side(OrderSide::Buy)
            .quantity(Quantity::from("12"))
            .price(Price::from("187.25"))
            .time_in_force(TimeInForce::Day)
            .build();

        let request = to_alpaca_order(&order, Venue::new("ALPACA"), false).unwrap();

        assert_eq!(request.symbol, "AAPL");
        assert_eq!(request.qty, Decimal::from(12));
        assert_eq!(request.limit_price, Some(Decimal::new(18_725, 2)));
        assert_eq!(request.order_type, AlpacaOrderType::Limit);
        assert_eq!(request.time_in_force, AlpacaTimeInForce::Day);
    }

    #[rstest]
    fn test_translates_stop_limit_prices() {
        let order = OrderTestBuilder::new(OrderType::StopLimit)
            .instrument_id(InstrumentId::from("AAPL.ALPACA"))
            .side(OrderSide::Sell)
            .quantity(Quantity::from("3"))
            .price(Price::from("179.50"))
            .trigger_price(Price::from("180.00"))
            .time_in_force(TimeInForce::Gtc)
            .build();

        let request = to_alpaca_order(&order, Venue::new("ALPACA"), false).unwrap();

        assert_eq!(request.order_type, AlpacaOrderType::StopLimit);
        assert_eq!(request.limit_price, Some(Decimal::new(17_950, 2)));
        assert_eq!(request.stop_price, Some(Decimal::new(18_000, 2)));
    }

    #[rstest]
    fn test_translates_native_bracket_as_one_atomic_alpaca_request() {
        let instrument_id = InstrumentId::from("AAPL.ALPACA");
        let entry_id = ClientOrderId::from("ENTRY-1");
        let stop_id = ClientOrderId::from("STOP-1");
        let profit_id = ClientOrderId::from("PROFIT-1");
        let entry = OrderTestBuilder::new(OrderType::Market)
            .instrument_id(instrument_id)
            .client_order_id(entry_id)
            .side(OrderSide::Buy)
            .quantity(Quantity::from("5"))
            .time_in_force(TimeInForce::Gtc)
            .contingency_type(ContingencyType::Oto)
            .linked_order_ids(vec![stop_id, profit_id])
            .build();
        let stop = OrderTestBuilder::new(OrderType::StopLimit)
            .instrument_id(instrument_id)
            .client_order_id(stop_id)
            .side(OrderSide::Sell)
            .quantity(Quantity::from("5"))
            .price(Price::from("178.50"))
            .trigger_price(Price::from("179.00"))
            .time_in_force(TimeInForce::Gtc)
            .reduce_only(true)
            .contingency_type(ContingencyType::Ouo)
            .linked_order_ids(vec![profit_id])
            .parent_order_id(entry_id)
            .build();
        let profit = OrderTestBuilder::new(OrderType::Limit)
            .instrument_id(instrument_id)
            .client_order_id(profit_id)
            .side(OrderSide::Sell)
            .quantity(Quantity::from("5"))
            .price(Price::from("195.00"))
            .time_in_force(TimeInForce::Gtc)
            .reduce_only(true)
            .post_only(true)
            .contingency_type(ContingencyType::Ouo)
            .linked_order_ids(vec![stop_id])
            .parent_order_id(entry_id)
            .build();

        let request =
            to_alpaca_bracket_order(&[entry, stop, profit], Venue::new("ALPACA"), false).unwrap();
        let wire = serde_json::to_value(&request).unwrap();

        assert_eq!(wire["order_class"], "bracket");
        assert_eq!(wire["take_profit"]["limit_price"], "195.00");
        assert_eq!(wire["stop_loss"]["stop_price"], "179.00");
        assert_eq!(wire["stop_loss"]["limit_price"], "178.50");
        assert_eq!(wire["client_order_id"], "ENTRY-1");
    }

    #[rstest]
    fn test_translates_native_oto_stop_as_one_atomic_request() {
        let instrument_id = InstrumentId::from("AAPL.ALPACA");
        let parent_id = ClientOrderId::from("ENTRY-OTO");
        let child_id = ClientOrderId::from("STOP-OTO");
        let parent = OrderTestBuilder::new(OrderType::Limit)
            .instrument_id(instrument_id)
            .client_order_id(parent_id)
            .side(OrderSide::Buy)
            .quantity(Quantity::from("2"))
            .price(Price::from("185.00"))
            .time_in_force(TimeInForce::Day)
            .contingency_type(ContingencyType::Oto)
            .linked_order_ids(vec![child_id])
            .build();
        let child = OrderTestBuilder::new(OrderType::StopMarket)
            .instrument_id(instrument_id)
            .client_order_id(child_id)
            .side(OrderSide::Sell)
            .quantity(Quantity::from("2"))
            .trigger_price(Price::from("179.00"))
            .time_in_force(TimeInForce::Day)
            .reduce_only(true)
            .parent_order_id(parent_id)
            .build();

        let translation =
            to_alpaca_advanced_order(&[parent, child], Venue::new("ALPACA"), false).unwrap();
        let wire = serde_json::to_value(translation.request).unwrap();

        assert_eq!(translation.primary, 0);
        assert_eq!(wire["order_class"], "oto");
        assert_eq!(wire["stop_loss"]["stop_price"], "179.00");
        assert!(wire.get("take_profit").is_none());
    }

    #[rstest]
    fn test_translates_native_oco_with_take_profit_as_primary() {
        let instrument_id = InstrumentId::from("AAPL.ALPACA");
        let stop_id = ClientOrderId::from("STOP-OCO");
        let profit_id = ClientOrderId::from("PROFIT-OCO");
        let stop = OrderTestBuilder::new(OrderType::StopMarket)
            .instrument_id(instrument_id)
            .client_order_id(stop_id)
            .side(OrderSide::Sell)
            .quantity(Quantity::from("2"))
            .trigger_price(Price::from("179.00"))
            .time_in_force(TimeInForce::Gtc)
            .reduce_only(true)
            .contingency_type(ContingencyType::Oco)
            .linked_order_ids(vec![profit_id])
            .build();
        let profit = OrderTestBuilder::new(OrderType::Limit)
            .instrument_id(instrument_id)
            .client_order_id(profit_id)
            .side(OrderSide::Sell)
            .quantity(Quantity::from("2"))
            .price(Price::from("195.00"))
            .time_in_force(TimeInForce::Gtc)
            .reduce_only(true)
            .contingency_type(ContingencyType::Oco)
            .linked_order_ids(vec![stop_id])
            .build();

        let translation =
            to_alpaca_advanced_order(&[stop, profit], Venue::new("ALPACA"), false).unwrap();
        let wire = serde_json::to_value(translation.request).unwrap();

        assert_eq!(translation.primary, 1);
        assert_eq!(translation.children, vec![0]);
        assert_eq!(wire["client_order_id"], "PROFIT-OCO");
        assert_eq!(wire["order_class"], "oco");
        assert_eq!(wire["take_profit"]["limit_price"], "195.00");
        assert!(wire.get("limit_price").is_none());
    }

    #[rstest]
    fn test_rejects_wrong_venue_and_unsupported_market_to_limit() {
        let wrong_venue = OrderTestBuilder::new(OrderType::Market)
            .instrument_id(InstrumentId::from("AAPL.NASDAQ"))
            .quantity(Quantity::from("1"))
            .build();
        let unsupported = OrderTestBuilder::new(OrderType::MarketToLimit)
            .instrument_id(InstrumentId::from("AAPL.ALPACA"))
            .quantity(Quantity::from("1"))
            .build();

        assert!(to_alpaca_order(&wrong_venue, Venue::new("ALPACA"), false).is_err());
        assert!(to_alpaca_order(&unsupported, Venue::new("ALPACA"), false).is_err());
    }

    #[rstest]
    fn test_extended_hours_requires_day_or_gtc_limit() {
        let market = OrderTestBuilder::new(OrderType::Market)
            .instrument_id(InstrumentId::from("AAPL.ALPACA"))
            .quantity(Quantity::from("1"))
            .time_in_force(TimeInForce::Day)
            .build();

        let error = to_alpaca_order(&market, Venue::new("ALPACA"), true).unwrap_err();
        assert!(error.to_string().contains("extended-hours"));
    }

    #[rstest]
    fn test_fractional_orders_require_day_time_in_force() {
        let gtc = OrderTestBuilder::new(OrderType::Limit)
            .instrument_id(InstrumentId::from("AAPL.ALPACA"))
            .quantity(Quantity::from("0.125"))
            .price(Price::from("187.25"))
            .time_in_force(TimeInForce::Gtc)
            .build();
        let day = OrderTestBuilder::new(OrderType::Limit)
            .instrument_id(InstrumentId::from("AAPL.ALPACA"))
            .quantity(Quantity::from("0.125"))
            .price(Price::from("187.25"))
            .time_in_force(TimeInForce::Day)
            .build();

        let error = to_alpaca_order(&gtc, Venue::new("ALPACA"), false).unwrap_err();
        assert!(error.to_string().contains("fractional-share"));
        assert_eq!(
            to_alpaca_order(&day, Venue::new("ALPACA"), false)
                .unwrap()
                .qty,
            Decimal::new(125, 3)
        );
    }

    #[rstest]
    fn test_fractional_replacement_validation_requires_day() {
        assert!(validate_fractional_time_in_force(Decimal::new(125, 3), TimeInForce::Gtc).is_err());
        assert!(validate_fractional_time_in_force(Decimal::new(125, 3), TimeInForce::Day).is_ok());
        assert!(validate_fractional_time_in_force(Decimal::from(2), TimeInForce::Gtc).is_ok());
    }

    #[rstest]
    #[case("187.25", true)]
    #[case("187.251", false)]
    #[case("0.1234", true)]
    #[case("0.12345", false)]
    #[case("0", false)]
    fn test_equity_price_precision_matches_alpaca_sub_penny_rules(
        #[case] value: &str,
        #[case] valid: bool,
    ) {
        let result =
            validate_equity_order_price("limit_price", Some(Decimal::from_str(value).unwrap()));

        assert_eq!(result.is_ok(), valid);
    }

    #[rstest]
    fn test_order_translation_rejects_sub_penny_limit_before_network() {
        let order = OrderTestBuilder::new(OrderType::Limit)
            .instrument_id(InstrumentId::from("AAPL.ALPACA"))
            .quantity(Quantity::from("1"))
            .price(Price::from("187.251"))
            .time_in_force(TimeInForce::Day)
            .build();

        let error = to_alpaca_order(&order, Venue::new("ALPACA"), false).unwrap_err();

        assert!(error.to_string().contains("at most 2 decimals"));
    }

    #[rstest]
    fn test_equity_quantity_accepts_at_most_nine_decimals() {
        assert!(validate_equity_quantity(Decimal::from_str("0.000000001").unwrap()).is_ok());
        assert!(validate_equity_quantity(Decimal::from_str("0.0000000001").unwrap()).is_err());
        assert!(validate_equity_quantity(Decimal::ZERO).is_err());
    }

    #[rstest]
    fn test_account_snapshot_preserves_alpaca_risk_metadata() {
        let account = AlpacaAccount {
            id: "account-id".to_string(),
            account_number: "PA123456".to_string(),
            status: "ACTIVE".to_string(),
            currency: "USD".to_string(),
            cash: Decimal::new(10_000_125, 3),
            buying_power: Decimal::new(2_000_025, 2),
            equity: Decimal::new(12_500_125, 3),
            portfolio_value: Decimal::new(12_500_125, 3),
            long_market_value: Decimal::from(2_500),
            short_market_value: Decimal::ZERO,
            pattern_day_trader: Some(true),
            trading_blocked: false,
            transfers_blocked: false,
            account_blocked: false,
            trade_suspended_by_user: false,
            shorting_enabled: true,
            multiplier: Decimal::from(2),
            created_at: "2024-01-01T00:00:00Z".parse().unwrap(),
            regt_buying_power: Some(Decimal::new(2_000_025, 2)),
            non_marginable_buying_power: Some(Decimal::from(10_000)),
            initial_margin: Some(Decimal::from(1_250)),
            maintenance_margin: Some(Decimal::from(750)),
            last_equity: Some(Decimal::from(12_000)),
            last_maintenance_margin: Some(Decimal::from(700)),
            sma: Some(Decimal::ZERO),
            accrued_fees: Some(Decimal::new(125, 2)),
            pending_transfer_in: Some(Decimal::from(500)),
            options_buying_power: Some(Decimal::from(8_000)),
            options_approved_level: Some(2),
            options_trading_level: Some(1),
            crypto_status: Some("ACTIVE".to_string()),
        };

        let (balance, info, _) = account_balance(&account).unwrap();
        let info = info.unwrap();

        assert_eq!(balance.total, Money::from("12500.125 USD"));
        assert_eq!(info.get_str("buying_power"), Some("20000.25"));
        assert_eq!(info.get_str("portfolio_value"), Some("12500.125"));
        assert_eq!(info.get_str("status"), Some("ACTIVE"));
        assert_eq!(info.get_bool("pattern_day_trader"), Some(true));
        assert_eq!(info.get_bool("shorting_enabled"), Some(true));
        assert_eq!(info.get_str("accrued_fees"), Some("1.25"));
        assert_eq!(info.get_u64("options_trading_level"), Some(1));
        assert_eq!(info.get_str("crypto_status"), Some("ACTIVE"));
        assert!(!info.contains_key("account_number"));
    }

    async fn account_snapshot_handler() -> Json<Value> {
        Json(json!({
            "id":"account-id", "account_number":"PA123", "status":"ACTIVE",
            "currency":"USD", "cash":"10000.125", "buying_power":"20000.25",
            "equity":"12500.125", "portfolio_value":"12500.125",
            "long_market_value":"2500", "short_market_value":"0",
            "pattern_day_trader":true, "trading_blocked":false,
            "transfers_blocked":false, "account_blocked":false,
            "trade_suspended_by_user":false, "shorting_enabled":true,
            "multiplier":"2", "created_at":"2024-01-01T00:00:00Z"
        }))
    }

    #[rstest]
    #[tokio::test]
    async fn test_background_account_refresh_emits_fresh_broker_snapshot() {
        let app = Router::new().route("/v2/account", get(account_snapshot_handler));
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let http = AlpacaRawHttpClient::with_base_urls(
            AlpacaCredential::new("key", "secret"),
            format!("http://{address}"),
            format!("http://{address}"),
            10,
        )
        .unwrap();
        let mut emitter = ExecutionEventEmitter::new(
            get_atomic_clock_realtime(),
            nautilus_model::identifiers::TraderId::from("TRADER-001"),
            AccountId::from("ALPACA-001"),
            AccountType::Margin,
            Some(Currency::USD()),
        );
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        emitter.set_sender(tx);
        let mut refreshes = tokio::task::JoinSet::new();
        let mut pending = false;

        queue_account_refresh(&mut refreshes, &mut pending, http, emitter);
        assert!(!pending);
        refreshes.join_next().await.unwrap().unwrap();

        match rx.recv().await.unwrap() {
            ExecutionEvent::Account(state) => {
                let info = state.info.unwrap();
                assert_eq!(info.get_str("buying_power"), Some("20000.25"));
                assert_eq!(info.get_bool("pattern_day_trader"), Some(true));
            }
            event => panic!("Expected refreshed account state, got {event:?}"),
        }
        server.abort();
    }

    #[rstest]
    #[tokio::test]
    async fn test_account_refresh_requests_are_coalesced_while_one_is_running() {
        let credential = AlpacaCredential::new("key", "secret");
        let http = AlpacaRawHttpClient::new(credential, Some("http://127.0.0.1:1".to_string()), 10)
            .unwrap();
        let emitter = ExecutionEventEmitter::new(
            get_atomic_clock_realtime(),
            nautilus_model::identifiers::TraderId::from("TRADER-001"),
            AccountId::from("ALPACA-001"),
            AccountType::Margin,
            Some(Currency::USD()),
        );
        let mut refreshes = tokio::task::JoinSet::new();
        let mut pending = false;

        queue_account_refresh(&mut refreshes, &mut pending, http.clone(), emitter.clone());
        queue_account_refresh(&mut refreshes, &mut pending, http, emitter);

        assert_eq!(refreshes.len(), 1);
        assert!(pending);
        refreshes.abort_all();
        while refreshes.join_next().await.is_some() {}
    }

    #[rstest]
    #[tokio::test]
    async fn test_private_stream_supervisor_retries_failed_handshake() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (first, _) = listener.accept().await.unwrap();
            let mut first = accept_async(first).await.unwrap();
            let auth = first.next().await.unwrap().unwrap();
            assert!(auth.to_text().unwrap().contains(r#""action":"auth""#));
            first
                .send(Message::Text(
                    r#"{"stream":"authorization","data":{"status":"unauthorized","action":"authenticate"}}"#
                        .into(),
                ))
                .await
                .unwrap();

            let (second, _) = listener.accept().await.unwrap();
            let mut second = accept_async(second).await.unwrap();
            let auth = second.next().await.unwrap().unwrap();
            assert!(auth.to_text().unwrap().contains(r#""action":"auth""#));
            second
                .send(Message::Text(
                    r#"{"stream":"authorization","data":{"status":"authorized","action":"authenticate"}}"#
                        .into(),
                ))
                .await
                .unwrap();
            let listen = second.next().await.unwrap().unwrap();
            assert!(listen.to_text().unwrap().contains("trade_updates"));
            second
                .send(Message::Text(
                    r#"{"stream":"listening","data":{"streams":["trade_updates"]}}"#.into(),
                ))
                .await
                .unwrap();
        });
        let mut socket = AlpacaTradingWebSocketClient::new(
            format!("ws://{address}"),
            AlpacaCredential::new("key", "secret"),
            nautilus_network::websocket::TransportBackend::Tungstenite,
            None,
        );
        let (_stop_tx, mut stop_rx) = tokio::sync::oneshot::channel();
        let mut delay = std::time::Duration::from_millis(1);

        assert!(reconnect_trade_updates(&mut socket, &mut stop_rx, &mut delay).await);
        assert_eq!(delay, std::time::Duration::from_millis(2));

        socket.disconnect().await;
        server.await.unwrap();
    }

    #[rstest]
    #[case(AlpacaOrderStatus::Accepted, Decimal::ZERO, OrderStatus::Accepted)]
    #[case(
        AlpacaOrderStatus::PartiallyFilled,
        Decimal::ONE,
        OrderStatus::PartiallyFilled
    )]
    #[case(
        AlpacaOrderStatus::PendingReplace,
        Decimal::ZERO,
        OrderStatus::PendingUpdate
    )]
    #[case(AlpacaOrderStatus::Calculated, Decimal::from(2), OrderStatus::Filled)]
    #[case(AlpacaOrderStatus::DoneForDay, Decimal::ONE, OrderStatus::Canceled)]
    fn test_reconciliation_order_status_mapping(
        #[case] alpaca: AlpacaOrderStatus,
        #[case] filled: Decimal,
        #[case] expected: OrderStatus,
    ) {
        assert_eq!(
            parse_order_status(alpaca, Decimal::from(2), filled).unwrap(),
            expected
        );
    }

    #[rstest]
    fn test_unknown_reconciliation_status_is_not_silently_accepted() {
        assert!(
            parse_order_status(AlpacaOrderStatus::Unknown, Decimal::ONE, Decimal::ZERO).is_err()
        );
    }

    #[rstest]
    fn test_fill_reconciliation_fetches_each_missing_order_only_once() {
        let fill = |id: &str, order_id: &str| {
            serde_json::from_value::<AlpacaTradeActivity>(json!({
                "activity_type":"FILL", "id":id, "order_id":order_id,
                "symbol":"AAPL", "side":"buy", "qty":"1", "price":"185.25",
                "cum_qty":"1", "leaves_qty":"0", "type":"fill",
                "transaction_time":"2024-01-01T00:00:01Z"
            }))
            .unwrap()
        };
        let fills = [
            fill("fill-1", "known-order"),
            fill("fill-2", "missing-order"),
            fill("fill-3", "missing-order"),
        ];
        let known_order: AlpacaOrder = serde_json::from_value(json!({
            "id":"known-order", "client_order_id":"client-order-id", "symbol":"AAPL",
            "asset_class":"us_equity", "qty":"1", "filled_qty":"1",
            "filled_avg_price":"185.25", "side":"buy", "type":"market",
            "time_in_force":"day", "limit_price":null, "stop_price":null,
            "status":"filled", "extended_hours":false,
            "created_at":"2024-01-01T00:00:00Z", "updated_at":"2024-01-01T00:00:01Z",
            "submitted_at":"2024-01-01T00:00:00Z", "filled_at":"2024-01-01T00:00:01Z",
            "canceled_at":null, "expired_at":null, "failed_at":null
        }))
        .unwrap();
        let orders = HashMap::from([(known_order.id.clone(), known_order)]);

        let missing = missing_fill_order_ids(&fills, &orders);

        assert_eq!(missing, HashSet::from(["missing-order".to_string()]));
    }

    #[rstest]
    fn test_replaced_trade_update_emits_new_venue_identity_and_terms() {
        let instrument = InstrumentAny::Equity(equity_aapl());
        let order = OrderTestBuilder::new(OrderType::Limit)
            .instrument_id(instrument.id())
            .side(OrderSide::Buy)
            .quantity(Quantity::from("1"))
            .price(Price::from("185.00"))
            .time_in_force(TimeInForce::Day)
            .build();
        let client_order_id = order.client_order_id();
        let contexts = Arc::new(Mutex::new(HashMap::from([(
            client_order_id,
            (order, instrument),
        )])));
        let mut emitter = ExecutionEventEmitter::new(
            get_atomic_clock_realtime(),
            nautilus_model::identifiers::TraderId::from("TRADER-001"),
            AccountId::from("ALPACA-001"),
            AccountType::Margin,
            Some(Currency::USD()),
        );
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        emitter.set_sender(tx);
        let timestamp: jiff::Timestamp = "2024-01-01T00:00:02Z".parse().unwrap();
        let update = AlpacaTradeUpdate {
            event: AlpacaTradeEvent::Replaced,
            order: AlpacaOrder {
                id: "replacement-order-id".to_string(),
                client_order_id: client_order_id.to_string(),
                symbol: "AAPL".to_string(),
                asset_class: crate::http::models::AlpacaAssetClass::UsEquity,
                qty: Decimal::from(2),
                filled_qty: Decimal::ZERO,
                filled_avg_price: None,
                side: AlpacaOrderSide::Buy,
                order_type: AlpacaOrderType::Limit,
                time_in_force: AlpacaTimeInForce::Day,
                limit_price: Some(Decimal::new(18_750, 2)),
                stop_price: None,
                status: AlpacaOrderStatus::Replaced,
                extended_hours: false,
                created_at: timestamp,
                updated_at: timestamp,
                submitted_at: Some(timestamp),
                filled_at: None,
                canceled_at: None,
                expired_at: None,
                failed_at: None,
                legs: None,
            },
            execution_id: None,
            price: None,
            qty: None,
            position_qty: None,
            timestamp: Some(timestamp),
        };

        assert!(
            handle_trade_update(
                &update,
                &contexts,
                &emitter,
                &mut HashSet::new(),
                &mut VecDeque::new(),
            )
            .unwrap()
        );

        match rx.try_recv().unwrap() {
            ExecutionEvent::Order(OrderEventAny::Updated(event)) => {
                assert_eq!(event.client_order_id, client_order_id);
                assert_eq!(
                    event.venue_order_id,
                    Some(VenueOrderId::new("replacement-order-id"))
                );
                assert_eq!(event.quantity, Quantity::from("2"));
                assert_eq!(event.price, Some(Price::from("187.50")));
            }
            event => panic!("Expected Alpaca OrderUpdated, got {event:?}"),
        }
    }
}
