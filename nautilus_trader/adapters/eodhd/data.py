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
EODHD live market data client.
"""

import asyncio
import os
from datetime import datetime
from datetime import timezone
from typing import Any

from nautilus_trader.adapters.eodhd.common import get_eodhd_asset_type
from nautilus_trader.adapters.eodhd.common import parse_crypto_msg
from nautilus_trader.adapters.eodhd.common import parse_forex_msg
from nautilus_trader.adapters.eodhd.common import parse_us_quote_msg
from nautilus_trader.adapters.eodhd.common import parse_us_trade_msg
from nautilus_trader.adapters.eodhd.common import to_eodhd_ws_symbol
from nautilus_trader.adapters.eodhd.config import EodhdDataClientConfig
from nautilus_trader.adapters.eodhd.constants import EODHD
from nautilus_trader.adapters.eodhd.constants import EODHD_BASE_URL_HTTP
from nautilus_trader.adapters.eodhd.constants import EODHD_BASE_URL_WS
from nautilus_trader.adapters.eodhd.enums import EodhdAssetType
from nautilus_trader.adapters.eodhd.enums import EodhdBarPeriod
from nautilus_trader.adapters.eodhd.enums import EodhdIntradayInterval
from nautilus_trader.adapters.eodhd.http_client import EodhdHttpClient
from nautilus_trader.adapters.eodhd.providers import EodhdInstrumentProvider
from nautilus_trader.adapters.eodhd.websocket import EodhdWebSocketClient
from nautilus_trader.cache.cache import Cache
from nautilus_trader.common.component import LiveClock
from nautilus_trader.common.component import MessageBus
from nautilus_trader.common.enums import LogColor
from nautilus_trader.data.messages import RequestBars
from nautilus_trader.data.messages import RequestInstrument
from nautilus_trader.data.messages import RequestInstruments
from nautilus_trader.data.messages import RequestQuoteTicks
from nautilus_trader.data.messages import RequestTradeTicks
from nautilus_trader.data.messages import SubscribeBars
from nautilus_trader.data.messages import SubscribeQuoteTicks
from nautilus_trader.data.messages import SubscribeTradeTicks
from nautilus_trader.data.messages import UnsubscribeBars
from nautilus_trader.data.messages import UnsubscribeQuoteTicks
from nautilus_trader.data.messages import UnsubscribeTradeTicks
from nautilus_trader.live.data_client import LiveMarketDataClient
from nautilus_trader.model.data import Bar
from nautilus_trader.model.data import BarType
from nautilus_trader.model.data import QuoteTick
from nautilus_trader.model.data import TradeTick
from nautilus_trader.model.enums import BarAggregation
from nautilus_trader.model.identifiers import ClientId
from nautilus_trader.model.identifiers import InstrumentId
from nautilus_trader.model.instruments import Instrument


class EodhdDataClient(LiveMarketDataClient):
    """
    Provides a data client for the EODHD data provider.

    Supports real-time WebSocket streaming for US equities, FOREX, and crypto,
    as well as historical EOD and intraday data requests.

    Parameters
    ----------
    loop : asyncio.AbstractEventLoop
        The event loop for the client.
    http_client : EodhdHttpClient
        The EODHD HTTP client for historical data.
    msgbus : MessageBus
        The message bus for the client.
    cache : Cache
        The cache for the client.
    clock : LiveClock
        The clock for the client.
    instrument_provider : EodhdInstrumentProvider
        The instrument provider.
    config : EodhdDataClientConfig
        The configuration for the client.
    name : str, optional
        The custom client ID.

    """

    def __init__(
        self,
        loop: asyncio.AbstractEventLoop,
        http_client: EodhdHttpClient,
        msgbus: MessageBus,
        cache: Cache,
        clock: LiveClock,
        instrument_provider: EodhdInstrumentProvider,
        config: EodhdDataClientConfig,
        name: str | None = None,
    ) -> None:
        super().__init__(
            loop=loop,
            client_id=ClientId(name or EODHD),
            venue=None,  # Multi-venue adapter
            msgbus=msgbus,
            cache=cache,
            clock=clock,
            instrument_provider=instrument_provider,
        )

        # Configuration
        self._config = config
        self._http_client = http_client

        # Get API key
        self._api_key = config.api_key or os.environ.get("EODHD_API_KEY", "")
        if not self._api_key:
            self._log.warning("No EODHD API key provided")

        # WebSocket clients by asset type
        self._ws_base_url = config.base_url_ws or EODHD_BASE_URL_WS
        self._ws_clients: dict[EodhdAssetType, EodhdWebSocketClient] = {}

        # Subscription tracking
        self._subscribed_trades: dict[InstrumentId, EodhdAssetType] = {}
        self._subscribed_quotes: dict[InstrumentId, EodhdAssetType] = {}

        # Tasks
        self._update_instruments_interval_mins = config.update_instruments_interval_mins
        self._update_instruments_task: asyncio.Task | None = None
        self._ws_connect_task: asyncio.Task | None = None

        self._log.info(f"{config.update_instruments_interval_mins=}", LogColor.BLUE)

    async def _connect(self) -> None:
        """Connect to EODHD."""
        await self._instrument_provider.initialize()
        self._send_all_instruments_to_data_engine()

        if self._update_instruments_interval_mins:
            self._update_instruments_task = self.create_task(
                self._update_instruments(self._update_instruments_interval_mins),
            )

        # Delay WebSocket connection to allow subscriptions to accumulate
        self._ws_connect_task = self.create_task(self._connect_ws_after_delay())

    async def _disconnect(self) -> None:
        """Disconnect from EODHD."""
        if self._update_instruments_task:
            self._log.debug("Canceling task 'update_instruments'")
            self._update_instruments_task.cancel()
            self._update_instruments_task = None

        if self._ws_connect_task:
            self._log.debug("Canceling task 'connect_ws_after_delay'")
            self._ws_connect_task.cancel()
            self._ws_connect_task = None

        # Close all WebSocket clients
        for asset_type, ws_client in self._ws_clients.items():
            if ws_client.is_connected:
                await ws_client.disconnect()
        self._ws_clients.clear()

        # Close HTTP client
        await self._http_client.close()

        self._subscribed_trades.clear()
        self._subscribed_quotes.clear()

    async def _connect_ws_after_delay(self) -> None:
        """Connect WebSocket clients after initial delay."""
        delay_secs = self._config.ws_connection_delay_secs
        self._log.info(
            f"Awaiting initial WebSocket connection delay ({delay_secs}s)...",
            LogColor.BLUE,
        )
        await asyncio.sleep(delay_secs)

        # Connect WebSocket clients for each asset type with subscriptions
        asset_types_needed = set()
        for instrument_id in self._subscribed_trades:
            asset_types_needed.add(get_eodhd_asset_type(instrument_id))
        for instrument_id in self._subscribed_quotes:
            asset_types_needed.add(get_eodhd_asset_type(instrument_id))

        for asset_type in asset_types_needed:
            await self._ensure_ws_client(asset_type)

    def _send_all_instruments_to_data_engine(self) -> None:
        """Send all loaded instruments to the data engine."""
        for instrument in self._instrument_provider.get_all().values():
            self._handle_data(instrument)

        for currency in self._instrument_provider.currencies().values():
            self._cache.add_currency(currency)

    async def _update_instruments(self, interval_mins: int) -> None:
        """Periodically update instruments."""
        try:
            while True:
                self._log.debug(
                    f"Scheduled task 'update_instruments' to run in {interval_mins} minutes",
                )
                await asyncio.sleep(interval_mins * 60)
                await self._instrument_provider.initialize(reload=True)
                self._send_all_instruments_to_data_engine()
        except asyncio.CancelledError:
            self._log.debug("Canceled task 'update_instruments'")

    async def _ensure_ws_client(self, asset_type: EodhdAssetType) -> EodhdWebSocketClient:
        """Ensure a WebSocket client exists for the asset type."""
        if asset_type not in self._ws_clients:
            ws_client = EodhdWebSocketClient(
                api_key=self._api_key,
                asset_type=asset_type,
                base_url=self._ws_base_url,
                on_message=lambda msg: self._handle_ws_message(msg, asset_type),
                logger=self._log,
            )
            await ws_client.connect()
            self._ws_clients[asset_type] = ws_client

        return self._ws_clients[asset_type]

    def _handle_ws_message(self, msg: dict, asset_type: EodhdAssetType) -> None:
        """Handle incoming WebSocket message."""
        try:
            if asset_type == EodhdAssetType.US_EQUITY:
                trade_tick = parse_us_trade_msg(msg)
                self._handle_data(trade_tick)
            elif asset_type == EodhdAssetType.US_QUOTE:
                quote_tick = parse_us_quote_msg(msg)
                self._handle_data(quote_tick)
            elif asset_type == EodhdAssetType.FOREX:
                quote_tick = parse_forex_msg(msg)
                self._handle_data(quote_tick)
            elif asset_type == EodhdAssetType.CRYPTO:
                trade_tick = parse_crypto_msg(msg)
                self._handle_data(trade_tick)
        except Exception as e:
            self._log.warning(f"Failed to parse WebSocket message: {e}")

    # -- SUBSCRIPTIONS -------------------------------------------------------------------------

    async def _subscribe_trade_ticks(self, command: SubscribeTradeTicks) -> None:
        """Subscribe to trade ticks."""
        instrument_id = command.instrument_id
        asset_type = get_eodhd_asset_type(instrument_id)

        # Trades are from US and Crypto endpoints
        if asset_type == EodhdAssetType.FOREX:
            self._log.warning(f"Trade ticks not available for FOREX: {instrument_id}")
            return

        ws_client = await self._ensure_ws_client(asset_type)
        symbol = to_eodhd_ws_symbol(instrument_id)
        await ws_client.subscribe([symbol])

        self._subscribed_trades[instrument_id] = asset_type
        self._log.info(f"Subscribed to trade ticks: {instrument_id}", LogColor.BLUE)

    async def _subscribe_quote_ticks(self, command: SubscribeQuoteTicks) -> None:
        """Subscribe to quote ticks."""
        instrument_id = command.instrument_id
        asset_type = get_eodhd_asset_type(instrument_id)

        # Quotes come from US_QUOTE and FOREX endpoints
        if asset_type == EodhdAssetType.US_EQUITY:
            asset_type = EodhdAssetType.US_QUOTE
        elif asset_type == EodhdAssetType.CRYPTO:
            self._log.warning(f"Quote ticks not available for crypto: {instrument_id}")
            return

        ws_client = await self._ensure_ws_client(asset_type)
        symbol = to_eodhd_ws_symbol(instrument_id)
        await ws_client.subscribe([symbol])

        self._subscribed_quotes[instrument_id] = asset_type
        self._log.info(f"Subscribed to quote ticks: {instrument_id}", LogColor.BLUE)

    async def _unsubscribe_trade_ticks(self, command: UnsubscribeTradeTicks) -> None:
        """Unsubscribe from trade ticks."""
        instrument_id = command.instrument_id

        if instrument_id not in self._subscribed_trades:
            return

        asset_type = self._subscribed_trades.pop(instrument_id)

        if asset_type in self._ws_clients:
            symbol = to_eodhd_ws_symbol(instrument_id)
            await self._ws_clients[asset_type].unsubscribe([symbol])

        self._log.info(f"Unsubscribed from trade ticks: {instrument_id}", LogColor.BLUE)

    async def _unsubscribe_quote_ticks(self, command: UnsubscribeQuoteTicks) -> None:
        """Unsubscribe from quote ticks."""
        instrument_id = command.instrument_id

        if instrument_id not in self._subscribed_quotes:
            return

        asset_type = self._subscribed_quotes.pop(instrument_id)

        if asset_type in self._ws_clients:
            symbol = to_eodhd_ws_symbol(instrument_id)
            await self._ws_clients[asset_type].unsubscribe([symbol])

        self._log.info(f"Unsubscribed from quote ticks: {instrument_id}", LogColor.BLUE)

    async def _subscribe_bars(self, command: SubscribeBars) -> None:
        """Subscribe to bars - not supported for live streaming."""
        self._log.warning(
            f"Bar subscriptions not supported for EODHD live streaming: {command.bar_type}",
        )

    async def _unsubscribe_bars(self, command: UnsubscribeBars) -> None:
        """Unsubscribe from bars."""
        pass  # Not supported

    # -- REQUESTS -----------------------------------------------------------------------------

    async def _request_instrument(self, request: RequestInstrument) -> None:
        """Request a single instrument."""
        instrument = self._instrument_provider.find(request.instrument_id)

        if instrument is None:
            # Try to load it
            await self._instrument_provider.load_async(request.instrument_id)
            instrument = self._instrument_provider.find(request.instrument_id)

        if instrument is None:
            self._log.error(f"Cannot find instrument for {request.instrument_id}")
            return

        self._handle_instrument(
            instrument,
            request.id,
            request.start,
            request.end,
            request.params,
        )

    async def _request_instruments(self, request: RequestInstruments) -> None:
        """Request instruments for a venue."""
        all_instruments = self._instrument_provider.get_all()
        target_instruments = []

        for instrument in all_instruments.values():
            if instrument.venue == request.venue:
                target_instruments.append(instrument)

        self._handle_instruments(
            request.venue,
            target_instruments,
            request.id,
            request.start,
            request.end,
            request.params,
        )

    async def _request_quote_ticks(self, request: RequestQuoteTicks) -> None:
        """Request historical quote ticks - not supported."""
        self._log.error(
            f"Cannot request historical quotes for {request.instrument_id}: not supported by EODHD",
        )

    async def _request_trade_ticks(self, request: RequestTradeTicks) -> None:
        """Request historical trade ticks - not supported."""
        self._log.error(
            f"Cannot request historical trades for {request.instrument_id}: not supported by EODHD",
        )

    async def _request_bars(self, request: RequestBars) -> None:
        """Request historical bars."""
        instrument = self._cache.instrument(request.bar_type.instrument_id)
        if instrument is None:
            self._log.error(
                f"Cannot request bars: no instrument for {request.bar_type.instrument_id}",
            )
            return

        instrument_id = request.bar_type.instrument_id
        symbol = instrument_id.symbol.value
        exchange = instrument_id.venue.value

        try:
            # Determine if intraday or EOD
            bar_spec = request.bar_type.spec

            if bar_spec.aggregation == BarAggregation.DAY:
                # EOD data
                start_date = request.start.date() if request.start else None
                end_date = request.end.date() if request.end else None

                raw_data = await self._http_client.get_eod_data(
                    symbol=symbol,
                    exchange=exchange,
                    start_date=start_date,
                    end_date=end_date,
                    period=EodhdBarPeriod.DAILY,
                )

                bars = self._http_client.parse_eod_bars(
                    raw_data,
                    request.bar_type,
                    price_precision=instrument.price_precision,
                    size_precision=instrument.size_precision,
                )

            elif bar_spec.aggregation == BarAggregation.MINUTE:
                # Intraday data
                if bar_spec.step == 1:
                    interval = EodhdIntradayInterval.MINUTE_1
                elif bar_spec.step == 5:
                    interval = EodhdIntradayInterval.MINUTE_5
                else:
                    self._log.error(
                        f"Unsupported minute interval: {bar_spec.step} (only 1 and 5 supported)",
                    )
                    return

                start_ts = int(request.start.timestamp()) if request.start else None
                end_ts = int(request.end.timestamp()) if request.end else None

                raw_data = await self._http_client.get_intraday_data(
                    symbol=symbol,
                    exchange=exchange,
                    start_timestamp=start_ts,
                    end_timestamp=end_ts,
                    interval=interval,
                )

                bars = self._http_client.parse_intraday_bars(
                    raw_data,
                    request.bar_type,
                    price_precision=instrument.price_precision,
                    size_precision=instrument.size_precision,
                )

            elif bar_spec.aggregation == BarAggregation.HOUR and bar_spec.step == 1:
                # 1-hour intraday
                start_ts = int(request.start.timestamp()) if request.start else None
                end_ts = int(request.end.timestamp()) if request.end else None

                raw_data = await self._http_client.get_intraday_data(
                    symbol=symbol,
                    exchange=exchange,
                    start_timestamp=start_ts,
                    end_timestamp=end_ts,
                    interval=EodhdIntradayInterval.HOUR_1,
                )

                bars = self._http_client.parse_intraday_bars(
                    raw_data,
                    request.bar_type,
                    price_precision=instrument.price_precision,
                    size_precision=instrument.size_precision,
                )

            else:
                self._log.error(
                    f"Unsupported bar aggregation: {bar_spec.aggregation} "
                    f"(only MINUTE, HOUR, DAY supported)",
                )
                return

            # Apply limit if specified
            if request.limit and len(bars) > request.limit:
                bars = bars[-request.limit :]

            self._handle_bars(
                request.bar_type,
                bars,
                request.id,
                request.start,
                request.end,
                request.params,
            )

            self._log.info(
                f"Received {len(bars)} bars for {request.bar_type}",
                LogColor.BLUE,
            )

        except Exception as e:
            self._log.error(f"Failed to request bars: {e}")
