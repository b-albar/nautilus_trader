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
Alpaca data client for market data feeds and requests.
"""

from __future__ import annotations

import asyncio
from datetime import datetime
from decimal import Decimal
from typing import Any

from nautilus_trader.adapters.alpaca.config import AlpacaDataClientConfig
from nautilus_trader.adapters.alpaca.constants import ALPACA_VENUE
from nautilus_trader.adapters.alpaca.enums import AlpacaAssetClass
from nautilus_trader.adapters.alpaca.enums import AlpacaBarTimeframe
from nautilus_trader.adapters.alpaca.enums import AlpacaDataFeed
from nautilus_trader.adapters.alpaca.http import AlpacaHttpClient
from nautilus_trader.adapters.alpaca.providers import AlpacaInstrumentProvider
from nautilus_trader.adapters.alpaca.websocket import AlpacaWebSocketClient
from nautilus_trader.cache.cache import Cache
from nautilus_trader.common.component import LiveClock
from nautilus_trader.common.component import MessageBus
from nautilus_trader.common.enums import LogColor
from nautilus_trader.core.datetime import dt_to_unix_nanos
from nautilus_trader.core.datetime import ensure_pydatetime_utc
from nautilus_trader.core.datetime import unix_nanos_to_dt
from nautilus_trader.data.messages import RequestBars
from nautilus_trader.data.messages import RequestInstrument
from nautilus_trader.data.messages import RequestInstruments
from nautilus_trader.data.messages import RequestQuoteTicks
from nautilus_trader.data.messages import RequestTradeTicks
from nautilus_trader.data.messages import SubscribeBars
from nautilus_trader.data.messages import SubscribeInstrument
from nautilus_trader.data.messages import SubscribeInstruments
from nautilus_trader.data.messages import SubscribeOrderBook
from nautilus_trader.data.messages import SubscribeQuoteTicks
from nautilus_trader.data.messages import SubscribeTradeTicks
from nautilus_trader.data.messages import UnsubscribeBars
from nautilus_trader.data.messages import UnsubscribeInstrument
from nautilus_trader.data.messages import UnsubscribeInstruments
from nautilus_trader.data.messages import UnsubscribeOrderBook
from nautilus_trader.data.messages import UnsubscribeQuoteTicks
from nautilus_trader.data.messages import UnsubscribeTradeTicks
from nautilus_trader.live.data_client import LiveMarketDataClient
from nautilus_trader.model.data import Bar
from nautilus_trader.model.data import BarSpecification
from nautilus_trader.model.data import BarType
from nautilus_trader.model.data import QuoteTick
from nautilus_trader.model.data import TradeTick
from nautilus_trader.model.enums import AggressorSide
from nautilus_trader.model.enums import BarAggregation
from nautilus_trader.model.enums import PriceType
from nautilus_trader.model.identifiers import ClientId
from nautilus_trader.model.identifiers import InstrumentId
from nautilus_trader.model.identifiers import TradeId
from nautilus_trader.model.instruments import CurrencyPair
from nautilus_trader.model.instruments import Equity
from nautilus_trader.model.objects import Price
from nautilus_trader.model.objects import Quantity


class AlpacaDataClient(LiveMarketDataClient):
    """
    Provides a data client for the Alpaca brokerage.

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
    config : AlpacaDataClientConfig
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
        config: AlpacaDataClientConfig,
        name: str | None = None,
    ) -> None:
        super().__init__(
            loop=loop,
            client_id=ClientId(name or ALPACA_VENUE.value),
            venue=ALPACA_VENUE,
            msgbus=msgbus,
            cache=cache,
            clock=clock,
            instrument_provider=instrument_provider,
        )

        self._instrument_provider: AlpacaInstrumentProvider = instrument_provider

        # Configuration
        self._config = config
        self._log.info(f"config.paper={config.paper}", LogColor.BLUE)
        self._log.info(f"config.data_feed={config.data_feed.value}", LogColor.BLUE)
        self._log.info(f"config.http_timeout_secs={config.http_timeout_secs}", LogColor.BLUE)

        # HTTP client
        self._http_client = client

        # WebSocket clients for streaming data
        self._stock_ws_client: AlpacaWebSocketClient | None = None
        self._crypto_ws_client: AlpacaWebSocketClient | None = None
        self._ws_clients: dict[str, AlpacaWebSocketClient] = {}

        # Track subscriptions
        self._subscribed_trades: set[InstrumentId] = set()
        self._subscribed_quotes: set[InstrumentId] = set()
        self._subscribed_bars: dict[BarType, InstrumentId] = {}

    @property
    def instrument_provider(self) -> AlpacaInstrumentProvider:
        """Return the instrument provider."""
        return self._instrument_provider

    async def _connect(self) -> None:
        """Connect to Alpaca data feeds."""
        await self.instrument_provider.initialize()
        self._send_all_instruments_to_data_engine()

        # Initialize WebSocket clients
        self._stock_ws_client = AlpacaWebSocketClient(
            api_key=self._config.api_key,
            api_secret=self._config.api_secret,
            paper=self._config.paper,
            stream_type="stocks",
            data_feed=self._config.data_feed,
            base_url=self._config.base_url_ws,
            logger=self._log,
        )

        self._crypto_ws_client = AlpacaWebSocketClient(
            api_key=self._config.api_key,
            api_secret=self._config.api_secret,
            paper=self._config.paper,
            stream_type="crypto",
            base_url=self._config.base_url_ws,
            logger=self._log,
        )

        self._ws_clients["stocks"] = self._stock_ws_client
        self._ws_clients["crypto"] = self._crypto_ws_client

        self._log.info("Alpaca data client connected", LogColor.GREEN)

    async def _disconnect(self) -> None:
        """Disconnect from Alpaca data feeds."""
        # Close WebSocket connections
        for name, ws_client in self._ws_clients.items():
            if ws_client and not ws_client.is_closed():
                self._log.info(f"Disconnecting {name} WebSocket")
                await ws_client.close()

        self._ws_clients.clear()
        self._stock_ws_client = None
        self._crypto_ws_client = None

        # Close HTTP client
        await self._http_client.close()

        self._log.info("Alpaca data client disconnected", LogColor.GREEN)

    def _send_all_instruments_to_data_engine(self) -> None:
        """Send all loaded instruments to the data engine."""
        for instrument in self.instrument_provider.get_all().values():
            self._handle_data(instrument)

        for currency in self.instrument_provider.currencies().values():
            self._cache.add_currency(currency)

    def _get_ws_client_for_instrument(
        self,
        instrument_id: InstrumentId,
    ) -> AlpacaWebSocketClient:
        """Get the appropriate WebSocket client for an instrument."""
        instrument = self._cache.instrument(instrument_id)
        if instrument is None:
            instrument = self.instrument_provider.find(instrument_id)

        if instrument is None:
            raise ValueError(f"Unknown instrument: {instrument_id}")

        # Determine asset class from instrument type
        if isinstance(instrument, Equity):
            if self._stock_ws_client is None:
                raise RuntimeError("Stock WebSocket client not initialized")
            return self._stock_ws_client
        elif isinstance(instrument, CurrencyPair):
            if self._crypto_ws_client is None:
                raise RuntimeError("Crypto WebSocket client not initialized")
            return self._crypto_ws_client
        else:
            raise ValueError(f"Unsupported instrument type: {type(instrument)}")

    def _handle_ws_message(self, msg: dict) -> None:
        """Handle incoming WebSocket messages."""
        msg_type = msg.get("T")

        try:
            if msg_type == "t":  # Trade
                self._handle_trade_msg(msg)
            elif msg_type == "q":  # Quote
                self._handle_quote_msg(msg)
            elif msg_type == "b":  # Bar
                self._handle_bar_msg(msg)
            elif msg_type == "success":
                self._log.debug(f"WebSocket success: {msg.get('msg')}")
            elif msg_type == "error":
                self._log.error(f"WebSocket error: {msg.get('msg')}")
            elif msg_type == "subscription":
                self._log.debug(f"Subscription confirmed: {msg}")
            else:
                self._log.debug(f"Unhandled message type: {msg_type}")

        except Exception as e:
            self._log.exception(f"Error handling WebSocket message: {e}", e)

    def _handle_trade_msg(self, msg: dict) -> None:
        """Handle trade message."""
        symbol = msg.get("S")
        if not symbol:
            return

        instrument_id = InstrumentId.from_str(f"{symbol}.{ALPACA_VENUE}")
        instrument = self._cache.instrument(instrument_id)
        if instrument is None:
            return

        # Parse timestamp
        timestamp = msg.get("t")
        if timestamp:
            ts_event = self._parse_timestamp(timestamp)
        else:
            ts_event = self._clock.timestamp_ns()

        trade = TradeTick(
            instrument_id=instrument_id,
            price=Price.from_str(str(msg.get("p", 0))),
            size=Quantity.from_str(str(msg.get("s", 0))),
            aggressor_side=AggressorSide.NO_AGGRESSOR,  # Not provided by Alpaca
            trade_id=TradeId(str(msg.get("i", ts_event))),
            ts_event=ts_event,
            ts_init=self._clock.timestamp_ns(),
        )

        self._handle_data(trade)

    def _handle_quote_msg(self, msg: dict) -> None:
        """Handle quote message."""
        symbol = msg.get("S")
        if not symbol:
            return

        instrument_id = InstrumentId.from_str(f"{symbol}.{ALPACA_VENUE}")
        instrument = self._cache.instrument(instrument_id)
        if instrument is None:
            return

        # Parse timestamp
        timestamp = msg.get("t")
        if timestamp:
            ts_event = self._parse_timestamp(timestamp)
        else:
            ts_event = self._clock.timestamp_ns()

        quote = QuoteTick(
            instrument_id=instrument_id,
            bid_price=Price.from_str(str(msg.get("bp", 0))),
            ask_price=Price.from_str(str(msg.get("ap", 0))),
            bid_size=Quantity.from_str(str(msg.get("bs", 0))),
            ask_size=Quantity.from_str(str(msg.get("as", 0))),
            ts_event=ts_event,
            ts_init=self._clock.timestamp_ns(),
        )

        self._handle_data(quote)

    def _handle_bar_msg(self, msg: dict) -> None:
        """Handle bar message."""
        symbol = msg.get("S")
        if not symbol:
            return

        instrument_id = InstrumentId.from_str(f"{symbol}.{ALPACA_VENUE}")
        instrument = self._cache.instrument(instrument_id)
        if instrument is None:
            return

        # Parse timestamp
        timestamp = msg.get("t")
        if timestamp:
            ts_event = self._parse_timestamp(timestamp)
        else:
            ts_event = self._clock.timestamp_ns()

        # Find matching bar type from subscriptions
        bar_type = None
        for bt, iid in self._subscribed_bars.items():
            if iid == instrument_id:
                bar_type = bt
                break

        if bar_type is None:
            return

        bar = Bar(
            bar_type=bar_type,
            open=Price.from_str(str(msg.get("o", 0))),
            high=Price.from_str(str(msg.get("h", 0))),
            low=Price.from_str(str(msg.get("l", 0))),
            close=Price.from_str(str(msg.get("c", 0))),
            volume=Quantity.from_str(str(msg.get("v", 0))),
            ts_event=ts_event,
            ts_init=self._clock.timestamp_ns(),
        )

        self._handle_data(bar)

    def _parse_timestamp(self, timestamp: str) -> int:
        """Parse ISO timestamp to nanoseconds."""
        try:
            dt = datetime.fromisoformat(timestamp.replace("Z", "+00:00"))
            return dt_to_unix_nanos(dt)
        except Exception:
            return self._clock.timestamp_ns()

    # -------------------------------------------------------------------------
    # Subscriptions
    # -------------------------------------------------------------------------

    async def _subscribe_instrument(self, command: SubscribeInstrument) -> None:
        """Subscribe to instrument updates."""
        self._log.info(f"Subscribed to instrument updates for {command.instrument_id}")

    async def _subscribe_instruments(self, command: SubscribeInstruments) -> None:
        """Subscribe to all instruments updates."""
        self._log.info("Subscribed to instruments updates")

    async def _subscribe_trade_ticks(self, command: SubscribeTradeTicks) -> None:
        """Subscribe to trade ticks."""
        instrument_id = command.instrument_id
        symbol = instrument_id.symbol.value

        if instrument_id in self._subscribed_trades:
            self._log.warning(f"Already subscribed to trades for {instrument_id}")
            return

        ws_client = self._get_ws_client_for_instrument(instrument_id)

        # Connect if not connected
        if not ws_client.is_connected:
            await ws_client.connect(self._handle_ws_message)

        await ws_client.subscribe_trades([symbol])
        self._subscribed_trades.add(instrument_id)
        self._log.info(f"Subscribed to trade ticks for {instrument_id}")

    async def _unsubscribe_trade_ticks(self, command: UnsubscribeTradeTicks) -> None:
        """Unsubscribe from trade ticks."""
        instrument_id = command.instrument_id
        symbol = instrument_id.symbol.value

        if instrument_id not in self._subscribed_trades:
            self._log.warning(f"Not subscribed to trades for {instrument_id}")
            return

        ws_client = self._get_ws_client_for_instrument(instrument_id)
        await ws_client.unsubscribe_trades([symbol])
        self._subscribed_trades.discard(instrument_id)
        self._log.info(f"Unsubscribed from trade ticks for {instrument_id}")

    async def _subscribe_quote_ticks(self, command: SubscribeQuoteTicks) -> None:
        """Subscribe to quote ticks."""
        instrument_id = command.instrument_id
        symbol = instrument_id.symbol.value

        if instrument_id in self._subscribed_quotes:
            self._log.warning(f"Already subscribed to quotes for {instrument_id}")
            return

        ws_client = self._get_ws_client_for_instrument(instrument_id)

        # Connect if not connected
        if not ws_client.is_connected:
            await ws_client.connect(self._handle_ws_message)

        await ws_client.subscribe_quotes([symbol])
        self._subscribed_quotes.add(instrument_id)
        self._log.info(f"Subscribed to quote ticks for {instrument_id}")

    async def _unsubscribe_quote_ticks(self, command: UnsubscribeQuoteTicks) -> None:
        """Unsubscribe from quote ticks."""
        instrument_id = command.instrument_id
        symbol = instrument_id.symbol.value

        if instrument_id not in self._subscribed_quotes:
            self._log.warning(f"Not subscribed to quotes for {instrument_id}")
            return

        ws_client = self._get_ws_client_for_instrument(instrument_id)
        await ws_client.unsubscribe_quotes([symbol])
        self._subscribed_quotes.discard(instrument_id)
        self._log.info(f"Unsubscribed from quote ticks for {instrument_id}")

    async def _subscribe_bars(self, command: SubscribeBars) -> None:
        """Subscribe to bars."""
        bar_type = command.bar_type
        instrument_id = bar_type.instrument_id
        symbol = instrument_id.symbol.value

        if bar_type in self._subscribed_bars:
            self._log.warning(f"Already subscribed to bars for {bar_type}")
            return

        ws_client = self._get_ws_client_for_instrument(instrument_id)

        # Connect if not connected
        if not ws_client.is_connected:
            await ws_client.connect(self._handle_ws_message)

        await ws_client.subscribe_bars([symbol])
        self._subscribed_bars[bar_type] = instrument_id
        self._log.info(f"Subscribed to bars for {bar_type}")

    async def _unsubscribe_bars(self, command: UnsubscribeBars) -> None:
        """Unsubscribe from bars."""
        bar_type = command.bar_type
        instrument_id = bar_type.instrument_id
        symbol = instrument_id.symbol.value

        if bar_type not in self._subscribed_bars:
            self._log.warning(f"Not subscribed to bars for {bar_type}")
            return

        ws_client = self._get_ws_client_for_instrument(instrument_id)
        await ws_client.unsubscribe_bars([symbol])
        del self._subscribed_bars[bar_type]
        self._log.info(f"Unsubscribed from bars for {bar_type}")

    async def _subscribe_order_book_deltas(self, command: SubscribeOrderBook) -> None:
        """Subscribe to order book deltas (crypto only)."""
        instrument_id = command.instrument_id
        symbol = instrument_id.symbol.value

        # Only crypto supports orderbook streaming
        instrument = self._cache.instrument(instrument_id)
        if not isinstance(instrument, CurrencyPair):
            self._log.warning(f"Order book streaming only supported for crypto: {instrument_id}")
            return

        ws_client = self._crypto_ws_client
        if ws_client is None:
            raise RuntimeError("Crypto WebSocket client not initialized")

        if not ws_client.is_connected:
            await ws_client.connect(self._handle_ws_message)

        await ws_client.subscribe_orderbooks([symbol])
        self._log.info(f"Subscribed to order book for {instrument_id}")

    async def _subscribe_order_book_snapshots(self, command: SubscribeOrderBook) -> None:
        """Subscribe to order book snapshots."""
        await self._subscribe_order_book_deltas(command)

    async def _unsubscribe_order_book_deltas(self, command: UnsubscribeOrderBook) -> None:
        """Unsubscribe from order book deltas."""
        instrument_id = command.instrument_id
        symbol = instrument_id.symbol.value

        ws_client = self._crypto_ws_client
        if ws_client and not ws_client.is_closed():
            await ws_client.unsubscribe_orderbooks([symbol])
        self._log.info(f"Unsubscribed from order book for {instrument_id}")

    async def _unsubscribe_order_book_snapshots(self, command: UnsubscribeOrderBook) -> None:
        """Unsubscribe from order book snapshots."""
        await self._unsubscribe_order_book_deltas(command)

    async def _unsubscribe_instrument(self, command: UnsubscribeInstrument) -> None:
        """Unsubscribe from instrument updates."""
        self._log.info(f"Unsubscribed from instrument updates for {command.instrument_id}")

    async def _unsubscribe_instruments(self, command: UnsubscribeInstruments) -> None:
        """Unsubscribe from all instruments updates."""
        self._log.info("Unsubscribed from instruments updates")

    # -------------------------------------------------------------------------
    # Requests
    # -------------------------------------------------------------------------

    async def _request_instrument(self, request: RequestInstrument) -> None:
        """Request an instrument."""
        instrument = self.instrument_provider.find(request.instrument_id)
        if instrument:
            self._handle_data(instrument)
            self._log.debug(f"Sent instrument {request.instrument_id}")
        else:
            self._log.error(f"Instrument not found: {request.instrument_id}")

    async def _request_instruments(self, request: RequestInstruments) -> None:
        """Request multiple instruments."""
        instruments = []
        for instrument_id in request.instrument_ids:
            instrument = self.instrument_provider.find(instrument_id)
            if instrument:
                instruments.append(instrument)
                self._handle_data(instrument)
                self._log.debug(f"Sent instrument {instrument_id}")
            else:
                self._log.warning(f"Instrument not found: {instrument_id}")

        if not instruments:
            self._log.warning("No instruments found for request")
        else:
            self._log.info(f"Sent {len(instruments)} instruments")

    async def _request_quote_ticks(self, request: RequestQuoteTicks) -> None:
        """Request historical quote ticks."""
        instrument_id = request.instrument_id
        instrument = self._cache.instrument(instrument_id)

        if instrument is None:
            self._log.error(f"Instrument not found: {instrument_id}")
            return

        symbol = instrument_id.symbol.value
        start = ensure_pydatetime_utc(request.start) if request.start else None
        end = ensure_pydatetime_utc(request.end) if request.end else None

        try:
            if isinstance(instrument, Equity):
                data = await self._http_client.get_stock_quotes(
                    symbol=symbol,
                    start=start,
                    end=end,
                    limit=request.limit,
                    feed=self._config.data_feed,
                )
            else:
                data = await self._http_client.get_crypto_quotes(
                    symbol=symbol,
                    start=start,
                    end=end,
                    limit=request.limit,
                )

            quotes = self._parse_quotes(instrument_id, data)
            self._handle_quote_ticks(
                instrument_id,
                quotes,
                request.id,
                request.start,
                request.end,
                request.params,
            )

        except Exception as e:
            self._log.exception(f"Error requesting quotes for {instrument_id}", e)

    async def _request_trade_ticks(self, request: RequestTradeTicks) -> None:
        """Request historical trade ticks."""
        instrument_id = request.instrument_id
        instrument = self._cache.instrument(instrument_id)

        if instrument is None:
            self._log.error(f"Instrument not found: {instrument_id}")
            return

        symbol = instrument_id.symbol.value
        start = ensure_pydatetime_utc(request.start) if request.start else None
        end = ensure_pydatetime_utc(request.end) if request.end else None

        try:
            if isinstance(instrument, Equity):
                data = await self._http_client.get_stock_trades(
                    symbol=symbol,
                    start=start,
                    end=end,
                    limit=request.limit,
                    feed=self._config.data_feed,
                )
            else:
                data = await self._http_client.get_crypto_trades(
                    symbol=symbol,
                    start=start,
                    end=end,
                    limit=request.limit,
                )

            trades = self._parse_trades(instrument_id, data)
            self._handle_trade_ticks(
                instrument_id,
                trades,
                request.id,
                request.start,
                request.end,
                request.params,
            )

        except Exception as e:
            self._log.exception(f"Error requesting trades for {instrument_id}", e)

    async def _request_bars(self, request: RequestBars) -> None:
        """Request historical bars."""
        bar_type = request.bar_type
        instrument_id = bar_type.instrument_id
        instrument = self._cache.instrument(instrument_id)

        if instrument is None:
            self._log.error(f"Instrument not found: {instrument_id}")
            return

        symbol = instrument_id.symbol.value
        start = ensure_pydatetime_utc(request.start) if request.start else None
        end = ensure_pydatetime_utc(request.end) if request.end else None

        # Convert bar specification to Alpaca timeframe
        timeframe = self._bar_spec_to_timeframe(bar_type.spec)
        if timeframe is None:
            self._log.error(f"Unsupported bar specification: {bar_type.spec}")
            return

        try:
            if isinstance(instrument, Equity):
                data = await self._http_client.get_stock_bars(
                    symbol=symbol,
                    timeframe=timeframe,
                    start=start,
                    end=end,
                    limit=request.limit,
                    feed=self._config.data_feed,
                )
            else:
                data = await self._http_client.get_crypto_bars(
                    symbol=symbol,
                    timeframe=timeframe,
                    start=start,
                    end=end,
                    limit=request.limit,
                )

            bars = self._parse_bars(bar_type, data)
            self._handle_bars(
                bar_type,
                bars,
                request.id,
                request.start,
                request.end,
                request.params,
            )

        except Exception as e:
            self._log.exception(f"Error requesting bars for {bar_type}", e)

    def _bar_spec_to_timeframe(self, spec: BarSpecification) -> AlpacaBarTimeframe | None:
        """Convert bar specification to Alpaca timeframe."""
        if spec.aggregation == BarAggregation.MINUTE:
            if spec.step == 1:
                return AlpacaBarTimeframe.MINUTE_1
            elif spec.step == 5:
                return AlpacaBarTimeframe.MINUTE_5
            elif spec.step == 15:
                return AlpacaBarTimeframe.MINUTE_15
            elif spec.step == 30:
                return AlpacaBarTimeframe.MINUTE_30
        elif spec.aggregation == BarAggregation.HOUR:
            if spec.step == 1:
                return AlpacaBarTimeframe.HOUR_1
            elif spec.step == 4:
                return AlpacaBarTimeframe.HOUR_4
        elif spec.aggregation == BarAggregation.DAY:
            if spec.step == 1:
                return AlpacaBarTimeframe.DAY_1
        elif spec.aggregation == BarAggregation.WEEK:
            if spec.step == 1:
                return AlpacaBarTimeframe.WEEK_1
        elif spec.aggregation == BarAggregation.MONTH:
            if spec.step == 1:
                return AlpacaBarTimeframe.MONTH_1

        return None

    def _parse_quotes(
        self,
        instrument_id: InstrumentId,
        data: dict,
    ) -> list[QuoteTick]:
        """Parse quote data from API response."""
        quotes = []
        symbol = instrument_id.symbol.value

        # Handle both single and multi-symbol responses
        quotes_data = data.get("quotes", data.get(symbol, []))
        if isinstance(quotes_data, dict):
            quotes_data = quotes_data.get(symbol, [])

        for q in quotes_data:
            ts_event = self._parse_timestamp(q.get("t", ""))
            quote = QuoteTick(
                instrument_id=instrument_id,
                bid_price=Price.from_str(str(q.get("bp", 0))),
                ask_price=Price.from_str(str(q.get("ap", 0))),
                bid_size=Quantity.from_str(str(q.get("bs", 0))),
                ask_size=Quantity.from_str(str(q.get("as", 0))),
                ts_event=ts_event,
                ts_init=ts_event,
            )
            quotes.append(quote)

        return quotes

    def _parse_trades(
        self,
        instrument_id: InstrumentId,
        data: dict,
    ) -> list[TradeTick]:
        """Parse trade data from API response."""
        trades = []
        symbol = instrument_id.symbol.value

        # Handle both single and multi-symbol responses
        trades_data = data.get("trades", data.get(symbol, []))
        if isinstance(trades_data, dict):
            trades_data = trades_data.get(symbol, [])

        for t in trades_data:
            ts_event = self._parse_timestamp(t.get("t", ""))
            trade = TradeTick(
                instrument_id=instrument_id,
                price=Price.from_str(str(t.get("p", 0))),
                size=Quantity.from_str(str(t.get("s", 0))),
                aggressor_side=AggressorSide.NO_AGGRESSOR,
                trade_id=TradeId(str(t.get("i", ts_event))),
                ts_event=ts_event,
                ts_init=ts_event,
            )
            trades.append(trade)

        return trades

    def _parse_bars(
        self,
        bar_type: BarType,
        data: dict,
    ) -> list[Bar]:
        """Parse bar data from API response."""
        bars = []
        symbol = bar_type.instrument_id.symbol.value

        # Handle both single and multi-symbol responses
        bars_data = data.get("bars", data.get(symbol, []))
        if isinstance(bars_data, dict):
            bars_data = bars_data.get(symbol, [])

        for b in bars_data:
            ts_event = self._parse_timestamp(b.get("t", ""))
            bar = Bar(
                bar_type=bar_type,
                open=Price.from_str(str(b.get("o", 0))),
                high=Price.from_str(str(b.get("h", 0))),
                low=Price.from_str(str(b.get("l", 0))),
                close=Price.from_str(str(b.get("c", 0))),
                volume=Quantity.from_str(str(b.get("v", 0))),
                ts_event=ts_event,
                ts_init=ts_event,
            )
            bars.append(bar)

        return bars
