# -------------------------------------------------------------------------------------------------
#  Copyright (C) 2015-2025 Nautech Systems Pty Ltd. All rights reserved.
#  https://nautechsystems.io
#
#  Licensed under the GNU Lesser General Public License Version 3.0 (the "License");
#  You may not use this file except in compliance with the License.
#  You may obtain a copy of the License at https://www.gnu.org/licenses/lgpl-3.0.en.html
#
#  Unless required by applicable law or agreed to in writing, software
#  distributed under the License is distributed on an "AS IS" BASIS,
#  WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
#  See the License for the specific language governing permissions and
#  limitations under the License.
# -------------------------------------------------------------------------------------------------
"""
Alpaca execution client for order management and trading.
"""

from __future__ import annotations

import asyncio
from datetime import datetime
from decimal import Decimal
from typing import Any

from nautilus_trader.adapters.alpaca.config import AlpacaExecClientConfig
from nautilus_trader.adapters.alpaca.constants import ALPACA_VENUE
from nautilus_trader.adapters.alpaca.enums import AlpacaOrderSide
from nautilus_trader.adapters.alpaca.enums import AlpacaOrderStatus
from nautilus_trader.adapters.alpaca.enums import AlpacaOrderType
from nautilus_trader.adapters.alpaca.enums import AlpacaTimeInForce
from nautilus_trader.adapters.alpaca.http import AlpacaApiError
from nautilus_trader.adapters.alpaca.http import AlpacaHttpClient
from nautilus_trader.adapters.alpaca.providers import AlpacaInstrumentProvider
from nautilus_trader.adapters.alpaca.websocket import AlpacaTradingWebSocketClient
from nautilus_trader.cache.cache import Cache
from nautilus_trader.common.component import LiveClock
from nautilus_trader.common.component import MessageBus
from nautilus_trader.common.enums import LogColor
from nautilus_trader.common.enums import LogLevel
from nautilus_trader.core.datetime import dt_to_unix_nanos
from nautilus_trader.core.uuid import UUID4
from nautilus_trader.execution.messages import BatchCancelOrders
from nautilus_trader.execution.messages import CancelAllOrders
from nautilus_trader.execution.messages import CancelOrder
from nautilus_trader.execution.messages import GenerateFillReports
from nautilus_trader.execution.messages import GenerateOrderStatusReport
from nautilus_trader.execution.messages import GenerateOrderStatusReports
from nautilus_trader.execution.messages import GeneratePositionStatusReports
from nautilus_trader.execution.messages import ModifyOrder
from nautilus_trader.execution.messages import QueryAccount
from nautilus_trader.execution.messages import QueryOrder
from nautilus_trader.execution.messages import SubmitOrder
from nautilus_trader.execution.messages import SubmitOrderList
from nautilus_trader.execution.reports import FillReport
from nautilus_trader.execution.reports import OrderStatusReport
from nautilus_trader.execution.reports import PositionStatusReport
from nautilus_trader.live.execution_client import LiveExecutionClient
from nautilus_trader.model.currencies import USD
from nautilus_trader.model.enums import AccountType
from nautilus_trader.model.enums import LiquiditySide
from nautilus_trader.model.enums import OmsType
from nautilus_trader.model.enums import OrderSide
from nautilus_trader.model.enums import OrderStatus
from nautilus_trader.model.enums import OrderType
from nautilus_trader.model.enums import PositionSide
from nautilus_trader.model.enums import TimeInForce
from nautilus_trader.model.enums import order_side_to_str
from nautilus_trader.model.identifiers import AccountId
from nautilus_trader.model.identifiers import ClientId
from nautilus_trader.model.identifiers import ClientOrderId
from nautilus_trader.model.identifiers import InstrumentId
from nautilus_trader.model.identifiers import PositionId
from nautilus_trader.model.identifiers import TradeId
from nautilus_trader.model.identifiers import VenueOrderId
from nautilus_trader.model.instruments import CurrencyPair
from nautilus_trader.model.instruments import Equity
from nautilus_trader.model.objects import AccountBalance
from nautilus_trader.model.objects import Currency
from nautilus_trader.model.objects import MarginBalance
from nautilus_trader.model.objects import Money
from nautilus_trader.model.objects import Price
from nautilus_trader.model.objects import Quantity


class AlpacaExecutionClient(LiveExecutionClient):
    """
    Provides an execution client for the Alpaca brokerage.

    Supports stocks and crypto trading with paper and live accounts.

    Parameters
    ----------
    loop : asyncio.AbstractEventLoop
        The event loop for the client.
    client : AlpacaHttpClient
        The Alpaca HTTP client.
    msgbus : MessageBus
        The message bus for the client.
    cache : Cache
        The cache for the client.
    clock : LiveClock
        The clock for the client.
    instrument_provider : AlpacaInstrumentProvider
        The instrument provider.
    config : AlpacaExecClientConfig
        The configuration for the client.
    name : str, optional
        The custom client ID.

    """

    def __init__(
        self,
        loop: asyncio.AbstractEventLoop,
        client: AlpacaHttpClient,
        msgbus: MessageBus,
        cache: Cache,
        clock: LiveClock,
        instrument_provider: AlpacaInstrumentProvider,
        config: AlpacaExecClientConfig,
        name: str | None = None,
    ) -> None:
        super().__init__(
            loop=loop,
            client_id=ClientId(name or ALPACA_VENUE.value),
            venue=ALPACA_VENUE,
            oms_type=OmsType.NETTING,
            account_type=AccountType.CASH,  # Alpaca is primarily cash account
            base_currency=USD,
            instrument_provider=instrument_provider,
            msgbus=msgbus,
            cache=cache,
            clock=clock,
        )

        # Configuration
        self._config = config
        self._http_client = client
        self._instrument_provider: AlpacaInstrumentProvider = instrument_provider

        # Log configuration details
        self._log.info(f"config.paper={config.paper}", LogColor.BLUE)
        self._log.info(f"config.http_timeout_secs={config.http_timeout_secs}", LogColor.BLUE)
        self._log.info(
            f"config.fractional_qty_enabled={config.fractional_qty_enabled}", LogColor.BLUE
        )

        # WebSocket for trading updates
        self._ws_client: AlpacaTradingWebSocketClient | None = None

        # Order tracking
        self._order_id_map: dict[ClientOrderId, str] = {}  # client_order_id -> venue_order_id
        self._venue_order_map: dict[str, ClientOrderId] = {}  # venue_order_id -> client_order_id

        self._log.info("Alpaca execution client initialized")

    @property
    def alpaca_instrument_provider(self) -> AlpacaInstrumentProvider:
        """Return the instrument provider."""
        return self._instrument_provider

    # -------------------------------------------------------------------------
    # Connection handlers
    # -------------------------------------------------------------------------

    async def _connect(self) -> None:
        """Connect to Alpaca trading API."""
        self._log.info("Connecting to Alpaca execution...", LogColor.BLUE)

        # Load instruments
        await self._instrument_provider.initialize()
        self._log.info(
            f"Loaded {len(self._instrument_provider.list_all())} instruments",
            LogColor.GREEN,
        )

        # Get account info and set account ID
        account_info = await self._http_client.get_account()
        account_number = account_info.get("account_number", "default")
        account_id = AccountId(f"{ALPACA_VENUE.value}-{account_number}")
        self._set_account_id(account_id)

        # Update account state
        await self._update_account_state(account_info)

        # Initialize WebSocket for trade updates
        self._ws_client = AlpacaTradingWebSocketClient(
            api_key=self._config.api_key,
            api_secret=self._config.api_secret,
            paper=self._config.paper,
            base_url=self._config.base_url_ws,
            logger=self._log,
        )

        await self._ws_client.connect(self._handle_trade_update)
        await self._ws_client.subscribe_trade_updates()

        self._log.info("Alpaca execution client connected", LogColor.GREEN)

    async def _disconnect(self) -> None:
        """Disconnect from Alpaca trading API."""
        if self._ws_client and not self._ws_client.is_closed():
            await self._ws_client.close()
            self._ws_client = None

        await self._http_client.close()

        self._log.info("Alpaca execution client disconnected", LogColor.GREEN)

    async def _update_account_state(self, account_info: dict | None = None) -> None:
        """Update account state from API data."""
        if account_info is None:
            account_info = await self._http_client.get_account()

        # Parse account balances
        equity = Decimal(account_info.get("equity", "0"))
        cash = Decimal(account_info.get("cash", "0"))
        buying_power = Decimal(account_info.get("buying_power", "0"))

        balances = [
            AccountBalance(
                total=Money(equity, USD),
                locked=Money(equity - cash, USD),
                free=Money(cash, USD),
            ),
        ]

        margins = []
        if account_info.get("multiplier", "1") != "1":
            # Margin account
            margin_used = equity - cash
            margins.append(
                MarginBalance(
                    initial=Money(margin_used, USD),
                    maintenance=Money(Decimal(account_info.get("maintenance_margin", "0")), USD),
                    currency=USD,
                ),
            )

        self.generate_account_state(
            balances=balances,
            margins=margins,
            reported=True,
            ts_event=self._clock.timestamp_ns(),
        )

    def _handle_trade_update(self, msg: dict) -> None:
        """Handle trade update message from WebSocket."""
        if msg.get("stream") != "trade_updates":
            return

        data = msg.get("data", {})
        event = data.get("event")
        order_data = data.get("order", {})

        try:
            if event == "new":
                self._handle_order_accepted(order_data)
            elif event == "fill":
                self._handle_order_filled(order_data, data)
            elif event == "partial_fill":
                self._handle_order_partial_fill(order_data, data)
            elif event == "canceled":
                self._handle_order_canceled(order_data)
            elif event == "expired":
                self._handle_order_expired(order_data)
            elif event == "rejected":
                self._handle_order_rejected(order_data)
            elif event == "replaced":
                self._handle_order_replaced(order_data)
            elif event == "pending_new":
                pass  # Order is being processed
            elif event == "pending_cancel":
                pass  # Cancel is being processed
            elif event == "pending_replace":
                pass  # Replace is being processed
            else:
                self._log.debug(f"Unhandled trade update event: {event}")

        except Exception as e:
            self._log.exception(f"Error handling trade update: {e}", e)

    def _handle_order_accepted(self, order_data: dict) -> None:
        """Handle order accepted event."""
        client_order_id_str = order_data.get("client_order_id")
        if not client_order_id_str:
            return

        client_order_id = ClientOrderId(client_order_id_str)
        venue_order_id = VenueOrderId(order_data.get("id"))

        # Update mappings
        self._order_id_map[client_order_id] = order_data.get("id")
        self._venue_order_map[order_data.get("id")] = client_order_id

        # Get original order from cache
        order = self._cache.order(client_order_id)
        if order is None:
            self._log.warning(f"Order not found in cache: {client_order_id}")
            return

        self.generate_order_accepted(
            strategy_id=order.strategy_id,
            instrument_id=order.instrument_id,
            client_order_id=client_order_id,
            venue_order_id=venue_order_id,
            ts_event=self._clock.timestamp_ns(),
        )

    def _handle_order_filled(self, order_data: dict, event_data: dict) -> None:
        """Handle order filled event."""
        client_order_id_str = order_data.get("client_order_id")
        if not client_order_id_str:
            return

        client_order_id = ClientOrderId(client_order_id_str)
        order = self._cache.order(client_order_id)
        if order is None:
            self._log.warning(f"Order not found in cache: {client_order_id}")
            return

        venue_order_id = VenueOrderId(order_data.get("id"))
        fill_price = Price.from_str(
            str(event_data.get("price", order_data.get("filled_avg_price", "0")))
        )
        fill_qty = Quantity.from_str(str(event_data.get("qty", order_data.get("filled_qty", "0"))))

        self.generate_order_filled(
            strategy_id=order.strategy_id,
            instrument_id=order.instrument_id,
            client_order_id=client_order_id,
            venue_order_id=venue_order_id,
            venue_position_id=None,
            trade_id=TradeId(str(event_data.get("execution_id", self._clock.timestamp_ns()))),
            order_side=order.side,
            order_type=order.order_type,
            last_qty=fill_qty,
            last_px=fill_price,
            quote_currency=USD,
            commission=Money(Decimal("0"), USD),  # Alpaca is commission-free
            liquidity_side=LiquiditySide.NO_LIQUIDITY_SIDE,
            ts_event=self._clock.timestamp_ns(),
        )

    def _handle_order_partial_fill(self, order_data: dict, event_data: dict) -> None:
        """Handle partial fill event."""
        # Same as filled but may have more to fill
        self._handle_order_filled(order_data, event_data)

    def _handle_order_canceled(self, order_data: dict) -> None:
        """Handle order canceled event."""
        client_order_id_str = order_data.get("client_order_id")
        if not client_order_id_str:
            return

        client_order_id = ClientOrderId(client_order_id_str)
        venue_order_id = VenueOrderId(order_data.get("id"))

        order = self._cache.order(client_order_id)
        if order is None:
            self._log.warning(f"Order not found in cache: {client_order_id}")
            return

        self.generate_order_canceled(
            strategy_id=order.strategy_id,
            instrument_id=order.instrument_id,
            client_order_id=client_order_id,
            venue_order_id=venue_order_id,
            ts_event=self._clock.timestamp_ns(),
        )

    def _handle_order_expired(self, order_data: dict) -> None:
        """Handle order expired event."""
        client_order_id_str = order_data.get("client_order_id")
        if not client_order_id_str:
            return

        client_order_id = ClientOrderId(client_order_id_str)
        venue_order_id = VenueOrderId(order_data.get("id"))

        order = self._cache.order(client_order_id)
        if order is None:
            self._log.warning(f"Order not found in cache: {client_order_id}")
            return

        self.generate_order_expired(
            strategy_id=order.strategy_id,
            instrument_id=order.instrument_id,
            client_order_id=client_order_id,
            venue_order_id=venue_order_id,
            ts_event=self._clock.timestamp_ns(),
        )

    def _handle_order_rejected(self, order_data: dict) -> None:
        """Handle order rejected event."""
        client_order_id_str = order_data.get("client_order_id")
        if not client_order_id_str:
            return

        client_order_id = ClientOrderId(client_order_id_str)
        order = self._cache.order(client_order_id)
        if order is None:
            self._log.warning(f"Order not found in cache: {client_order_id}")
            return

        reason = order_data.get("reject_reason", "Unknown rejection reason")

        self.generate_order_rejected(
            strategy_id=order.strategy_id,
            instrument_id=order.instrument_id,
            client_order_id=client_order_id,
            reason=reason,
            ts_event=self._clock.timestamp_ns(),
        )

    def _handle_order_replaced(self, order_data: dict) -> None:
        """Handle order replaced (modified) event."""
        client_order_id_str = order_data.get("client_order_id")
        if not client_order_id_str:
            return

        client_order_id = ClientOrderId(client_order_id_str)
        venue_order_id = VenueOrderId(order_data.get("id"))

        order = self._cache.order(client_order_id)
        if order is None:
            self._log.warning(f"Order not found in cache: {client_order_id}")
            return

        # Parse updated values
        qty_str = order_data.get("qty")
        limit_price_str = order_data.get("limit_price")
        stop_price_str = order_data.get("stop_price")

        self.generate_order_updated(
            strategy_id=order.strategy_id,
            instrument_id=order.instrument_id,
            client_order_id=client_order_id,
            venue_order_id=venue_order_id,
            quantity=Quantity.from_str(qty_str) if qty_str else order.quantity,
            price=Price.from_str(limit_price_str) if limit_price_str else order.price,
            trigger_price=Price.from_str(stop_price_str) if stop_price_str else order.trigger_price,
            ts_event=self._clock.timestamp_ns(),
        )

    # -------------------------------------------------------------------------
    # Order commands
    # -------------------------------------------------------------------------

    async def _submit_order(self, command: SubmitOrder) -> None:
        """Submit an order."""
        order = command.order

        if order.is_closed:
            self._log.warning(f"Order {order} is already closed")
            return

        self.generate_order_submitted(
            strategy_id=order.strategy_id,
            instrument_id=order.instrument_id,
            client_order_id=order.client_order_id,
            ts_event=self._clock.timestamp_ns(),
        )

        try:
            # Convert to Alpaca order parameters
            symbol = order.instrument_id.symbol.value
            side = self._convert_order_side(order.side)
            order_type = self._convert_order_type(order.order_type)
            time_in_force = self._convert_time_in_force(order.time_in_force)

            # Prepare quantity
            qty_str = str(order.quantity)

            # Prepare prices
            limit_price = str(order.price) if order.has_price else None
            stop_price = str(order.trigger_price) if order.has_trigger_price else None

            # Submit order
            response = await self._http_client.create_order(
                symbol=symbol,
                qty=qty_str,
                side=side,
                order_type=order_type,
                time_in_force=time_in_force,
                limit_price=limit_price,
                stop_price=stop_price,
                client_order_id=str(order.client_order_id),
                extended_hours=False,
            )

            # Store mapping
            venue_order_id_str = response.get("id")
            self._order_id_map[order.client_order_id] = venue_order_id_str
            self._venue_order_map[venue_order_id_str] = order.client_order_id

            venue_order_id = VenueOrderId(venue_order_id_str)

            # If already accepted (immediate response), generate accepted event
            status = response.get("status", "")
            if status in ("new", "accepted", "pending_new"):
                self.generate_order_accepted(
                    strategy_id=order.strategy_id,
                    instrument_id=order.instrument_id,
                    client_order_id=order.client_order_id,
                    venue_order_id=venue_order_id,
                    ts_event=self._clock.timestamp_ns(),
                )

            self._log.info(f"Order submitted: {order.client_order_id} -> {venue_order_id}")

        except AlpacaApiError as e:
            self._log.error(f"Order rejected: {e.message}")
            self.generate_order_rejected(
                strategy_id=order.strategy_id,
                instrument_id=order.instrument_id,
                client_order_id=order.client_order_id,
                reason=e.message,
                ts_event=self._clock.timestamp_ns(),
            )
        except Exception as e:
            self._log.error(f"Error submitting order: {e}")
            self.generate_order_rejected(
                strategy_id=order.strategy_id,
                instrument_id=order.instrument_id,
                client_order_id=order.client_order_id,
                reason=str(e),
                ts_event=self._clock.timestamp_ns(),
            )

    async def _submit_order_list(self, command: SubmitOrderList) -> None:
        """Submit a list of orders."""
        order_list = command.order_list
        orders = order_list.orders

        if not orders:
            self._log.warning("Order list is empty, nothing to submit")
            return

        # Alpaca supports bracket orders, but for simplicity we submit individually
        for order in orders:
            single_command = SubmitOrder(
                trader_id=command.trader_id,
                strategy_id=command.strategy_id,
                order=order,
                command_id=UUID4(),
                ts_init=self._clock.timestamp_ns(),
            )
            await self._submit_order(single_command)

    async def _modify_order(self, command: ModifyOrder) -> None:
        """Modify an existing order."""
        client_order_id = command.client_order_id
        venue_order_id_str = self._order_id_map.get(client_order_id)

        if not venue_order_id_str:
            self._log.error(f"Cannot modify order: venue order ID not found for {client_order_id}")
            return

        try:
            qty_str = str(command.quantity) if command.quantity else None
            limit_price_str = str(command.price) if command.price else None
            stop_price_str = str(command.trigger_price) if command.trigger_price else None

            response = await self._http_client.replace_order(
                order_id=venue_order_id_str,
                qty=qty_str,
                limit_price=limit_price_str,
                stop_price=stop_price_str,
            )

            # Update venue order ID mapping if needed
            new_venue_order_id = response.get("id")
            if new_venue_order_id != venue_order_id_str:
                del self._venue_order_map[venue_order_id_str]
                self._order_id_map[client_order_id] = new_venue_order_id
                self._venue_order_map[new_venue_order_id] = client_order_id

            self._log.info(f"Order modified: {client_order_id}")

        except AlpacaApiError as e:
            self._log.error(f"Failed to modify order: {e.message}")
        except Exception as e:
            self._log.error(f"Error modifying order: {e}")

    async def _cancel_order(self, command: CancelOrder) -> None:
        """Cancel an order."""
        client_order_id = command.client_order_id
        venue_order_id_str = self._order_id_map.get(client_order_id)

        if not venue_order_id_str:
            self._log.error(f"Cannot cancel order: venue order ID not found for {client_order_id}")
            return

        try:
            await self._http_client.cancel_order(venue_order_id_str)
            self._log.info(f"Order cancel requested: {client_order_id}")

        except AlpacaApiError as e:
            self._log.error(f"Failed to cancel order: {e.message}")
        except Exception as e:
            self._log.error(f"Error canceling order: {e}")

    async def _cancel_all_orders(self, command: CancelAllOrders) -> None:
        """Cancel all open orders."""
        if command.order_side != OrderSide.NO_ORDER_SIDE:
            self._log.warning(
                f"Alpaca does not support order_side filtering for cancel all orders; "
                f"ignoring order_side={order_side_to_str(command.order_side)}"
            )

        try:
            result = await self._http_client.cancel_all_orders()
            self._log.info(f"Canceled all orders: {len(result or [])} orders")

        except AlpacaApiError as e:
            self._log.error(f"Failed to cancel all orders: {e.message}")
        except Exception as e:
            self._log.error(f"Error canceling all orders: {e}")

    async def _batch_cancel_orders(self, command: BatchCancelOrders) -> None:
        """Batch cancel orders."""
        for cancel in command.cancels:
            await self._cancel_order(
                CancelOrder(
                    trader_id=command.trader_id,
                    strategy_id=cancel.strategy_id,
                    instrument_id=cancel.instrument_id,
                    client_order_id=cancel.client_order_id,
                    venue_order_id=cancel.venue_order_id,
                    command_id=UUID4(),
                    ts_init=self._clock.timestamp_ns(),
                )
            )

    # -------------------------------------------------------------------------
    # Reports
    # -------------------------------------------------------------------------

    async def generate_order_status_report(
        self,
        command: GenerateOrderStatusReport,
    ) -> OrderStatusReport | None:
        """Generate an order status report."""
        client_order_id = command.client_order_id
        venue_order_id = command.venue_order_id

        try:
            if venue_order_id:
                order_data = await self._http_client.get_order_by_id(str(venue_order_id))
            elif client_order_id:
                order_data = await self._http_client.get_order_by_client_id(str(client_order_id))
            else:
                return None

            return self._parse_order_status_report(order_data)

        except AlpacaApiError as e:
            self._log.error(f"Failed to get order status: {e.message}")
            return None

    async def generate_order_status_reports(
        self,
        command: GenerateOrderStatusReports,
    ) -> list[OrderStatusReport]:
        """Generate order status reports for all orders."""
        try:
            orders = await self._http_client.get_orders(status="all")
            reports = []

            for order_data in orders:
                try:
                    report = self._parse_order_status_report(order_data)
                    if report:
                        reports.append(report)
                except Exception as e:
                    self._log.warning(f"Failed to parse order: {e}")

            self._log.info(f"Generated {len(reports)} order status reports")
            return reports

        except AlpacaApiError as e:
            self._log.error(f"Failed to get orders: {e.message}")
            return []

    async def generate_fill_reports(
        self,
        command: GenerateFillReports,
    ) -> list[FillReport]:
        """Generate fill reports."""
        try:
            activities = await self._http_client.get_account_activities(
                activity_types=["FILL"],
            )

            reports = []
            for activity in activities:
                try:
                    report = self._parse_fill_report(activity)
                    if report:
                        reports.append(report)
                except Exception as e:
                    self._log.warning(f"Failed to parse fill: {e}")

            self._log.info(f"Generated {len(reports)} fill reports")
            return reports

        except AlpacaApiError as e:
            self._log.error(f"Failed to get fills: {e.message}")
            return []

    async def generate_position_status_reports(
        self,
        command: GeneratePositionStatusReports,
    ) -> list[PositionStatusReport]:
        """Generate position status reports."""
        try:
            positions = await self._http_client.get_all_positions()
            reports = []

            for position_data in positions:
                try:
                    report = self._parse_position_status_report(position_data)
                    if report:
                        reports.append(report)
                except Exception as e:
                    self._log.warning(f"Failed to parse position: {e}")

            self._log.info(f"Generated {len(reports)} position status reports")
            return reports

        except AlpacaApiError as e:
            self._log.error(f"Failed to get positions: {e.message}")
            return []

    def _parse_order_status_report(self, order_data: dict) -> OrderStatusReport | None:
        """Parse order data into an OrderStatusReport."""
        symbol = order_data.get("symbol")
        if not symbol:
            return None

        instrument_id = InstrumentId.from_str(f"{symbol}.{ALPACA_VENUE}")

        # Parse status
        status_str = order_data.get("status", "")
        order_status = self._parse_order_status(status_str)

        # Parse side
        side_str = order_data.get("side", "buy")
        order_side = OrderSide.BUY if side_str == "buy" else OrderSide.SELL

        # Parse order type
        type_str = order_data.get("type", "market")
        order_type = self._parse_order_type(type_str)

        # Parse time in force
        tif_str = order_data.get("time_in_force", "day")
        time_in_force = self._parse_time_in_force(tif_str)

        # Parse quantities
        qty = Quantity.from_str(str(order_data.get("qty", "0")))
        filled_qty = Quantity.from_str(str(order_data.get("filled_qty", "0")))

        # Parse prices
        limit_price_str = order_data.get("limit_price")
        stop_price_str = order_data.get("stop_price")
        avg_fill_price_str = order_data.get("filled_avg_price")

        # Parse timestamps
        ts_accepted = self._parse_timestamp(order_data.get("created_at", ""))
        ts_last = self._parse_timestamp(order_data.get("updated_at", ""))

        return OrderStatusReport(
            account_id=self.account_id,
            instrument_id=instrument_id,
            venue_order_id=VenueOrderId(order_data.get("id", "")),
            client_order_id=ClientOrderId(order_data.get("client_order_id", "")),
            order_side=order_side,
            order_type=order_type,
            time_in_force=time_in_force,
            order_status=order_status,
            quantity=qty,
            filled_qty=filled_qty,
            price=Price.from_str(limit_price_str) if limit_price_str else None,
            trigger_price=Price.from_str(stop_price_str) if stop_price_str else None,
            avg_px=Decimal(avg_fill_price_str) if avg_fill_price_str else None,
            report_id=UUID4(),
            ts_accepted=ts_accepted,
            ts_last=ts_last,
            ts_init=self._clock.timestamp_ns(),
        )

    def _parse_fill_report(self, activity: dict) -> FillReport | None:
        """Parse activity data into a FillReport."""
        symbol = activity.get("symbol")
        if not symbol:
            return None

        instrument_id = InstrumentId.from_str(f"{symbol}.{ALPACA_VENUE}")

        # Parse side
        side_str = activity.get("side", "buy")
        order_side = OrderSide.BUY if side_str == "buy" else OrderSide.SELL

        return FillReport(
            account_id=self.account_id,
            instrument_id=instrument_id,
            venue_order_id=VenueOrderId(activity.get("order_id", "")),
            client_order_id=ClientOrderId(activity.get("client_order_id", "")),
            trade_id=TradeId(activity.get("id", "")),
            order_side=order_side,
            last_qty=Quantity.from_str(str(activity.get("qty", "0"))),
            last_px=Price.from_str(str(activity.get("price", "0"))),
            commission=Money(Decimal("0"), USD),
            liquidity_side=LiquiditySide.NO_LIQUIDITY_SIDE,
            report_id=UUID4(),
            ts_event=self._parse_timestamp(activity.get("transaction_time", "")),
            ts_init=self._clock.timestamp_ns(),
        )

    def _parse_position_status_report(self, position_data: dict) -> PositionStatusReport | None:
        """Parse position data into a PositionStatusReport."""
        symbol = position_data.get("symbol")
        if not symbol:
            return None

        instrument_id = InstrumentId.from_str(f"{symbol}.{ALPACA_VENUE}")

        qty = Decimal(position_data.get("qty", "0"))
        side_str = position_data.get("side", "long")
        position_side = PositionSide.LONG if side_str == "long" else PositionSide.SHORT

        return PositionStatusReport(
            account_id=self.account_id,
            instrument_id=instrument_id,
            position_side=position_side,
            quantity=Quantity.from_str(str(abs(qty))),
            report_id=UUID4(),
            ts_last=self._clock.timestamp_ns(),
            ts_init=self._clock.timestamp_ns(),
        )

    # -------------------------------------------------------------------------
    # Queries
    # -------------------------------------------------------------------------

    async def _query_order(self, command: QueryOrder) -> None:
        """Query an order status."""
        try:
            report = await self.generate_order_status_report(command)
            if report:
                self._send_order_status_report(report)
        except Exception as e:
            self._log.error(f"Error querying order: {e}")

    async def _query_account(self, command: QueryAccount) -> None:
        """Query account state."""
        try:
            await self._update_account_state()
        except Exception as e:
            self._log.error(f"Error querying account: {e}")

    # -------------------------------------------------------------------------
    # Helpers
    # -------------------------------------------------------------------------

    def _convert_order_side(self, side: OrderSide) -> AlpacaOrderSide:
        """Convert nautilus order side to Alpaca."""
        if side == OrderSide.BUY:
            return AlpacaOrderSide.BUY
        elif side == OrderSide.SELL:
            return AlpacaOrderSide.SELL
        else:
            raise ValueError(f"Unsupported order side: {side}")

    def _convert_order_type(self, order_type: OrderType) -> AlpacaOrderType:
        """Convert nautilus order type to Alpaca."""
        if order_type == OrderType.MARKET:
            return AlpacaOrderType.MARKET
        elif order_type == OrderType.LIMIT:
            return AlpacaOrderType.LIMIT
        elif order_type == OrderType.STOP_MARKET:
            return AlpacaOrderType.STOP
        elif order_type == OrderType.STOP_LIMIT:
            return AlpacaOrderType.STOP_LIMIT
        elif order_type == OrderType.TRAILING_STOP_MARKET:
            return AlpacaOrderType.TRAILING_STOP
        else:
            raise ValueError(f"Unsupported order type: {order_type}")

    def _convert_time_in_force(self, tif: TimeInForce) -> AlpacaTimeInForce:
        """Convert nautilus time in force to Alpaca."""
        if tif == TimeInForce.DAY:
            return AlpacaTimeInForce.DAY
        elif tif == TimeInForce.GTC:
            return AlpacaTimeInForce.GTC
        elif tif == TimeInForce.IOC:
            return AlpacaTimeInForce.IOC
        elif tif == TimeInForce.FOK:
            return AlpacaTimeInForce.FOK
        elif tif == TimeInForce.AT_THE_OPEN:
            return AlpacaTimeInForce.OPG
        elif tif == TimeInForce.AT_THE_CLOSE:
            return AlpacaTimeInForce.CLS
        else:
            return AlpacaTimeInForce.DAY

    def _parse_order_status(self, status: str) -> OrderStatus:
        """Parse Alpaca order status to nautilus."""
        status_map = {
            "new": OrderStatus.ACCEPTED,
            "accepted": OrderStatus.ACCEPTED,
            "pending_new": OrderStatus.SUBMITTED,
            "partially_filled": OrderStatus.PARTIALLY_FILLED,
            "filled": OrderStatus.FILLED,
            "canceled": OrderStatus.CANCELED,
            "expired": OrderStatus.EXPIRED,
            "rejected": OrderStatus.REJECTED,
            "pending_cancel": OrderStatus.PENDING_CANCEL,
            "pending_replace": OrderStatus.PENDING_UPDATE,
            "stopped": OrderStatus.CANCELED,
            "suspended": OrderStatus.CANCELED,
            "held": OrderStatus.ACCEPTED,
        }
        return status_map.get(status.lower(), OrderStatus.INITIALIZED)

    def _parse_order_type(self, order_type: str) -> OrderType:
        """Parse Alpaca order type to nautilus."""
        type_map = {
            "market": OrderType.MARKET,
            "limit": OrderType.LIMIT,
            "stop": OrderType.STOP_MARKET,
            "stop_limit": OrderType.STOP_LIMIT,
            "trailing_stop": OrderType.TRAILING_STOP_MARKET,
        }
        return type_map.get(order_type.lower(), OrderType.MARKET)

    def _parse_time_in_force(self, tif: str) -> TimeInForce:
        """Parse Alpaca time in force to nautilus."""
        tif_map = {
            "day": TimeInForce.DAY,
            "gtc": TimeInForce.GTC,
            "ioc": TimeInForce.IOC,
            "fok": TimeInForce.FOK,
            "opg": TimeInForce.AT_THE_OPEN,
            "cls": TimeInForce.AT_THE_CLOSE,
        }
        return tif_map.get(tif.lower(), TimeInForce.DAY)

    def _parse_timestamp(self, timestamp: str) -> int:
        """Parse ISO timestamp to nanoseconds."""
        if not timestamp:
            return self._clock.timestamp_ns()
        try:
            dt = datetime.fromisoformat(timestamp.replace("Z", "+00:00"))
            return dt_to_unix_nanos(dt)
        except Exception:
            return self._clock.timestamp_ns()
