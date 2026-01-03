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
Alpaca WebSocket client for streaming data and trading events.
"""

from __future__ import annotations

import asyncio
import json
import os
from collections.abc import Callable
from datetime import datetime
from typing import Any

import aiohttp

from nautilus_trader.adapters.alpaca.constants import ALPACA_DATA_STREAM_URL
from nautilus_trader.adapters.alpaca.constants import ALPACA_LIVE_STREAM_URL
from nautilus_trader.adapters.alpaca.constants import ALPACA_PAPER_STREAM_URL
from nautilus_trader.adapters.alpaca.enums import AlpacaDataFeed
from nautilus_trader.common.component import Logger


class AlpacaWebSocketClient:
    """
    WebSocket client for Alpaca streaming data.

    Supports both market data streams and trading event streams.

    Parameters
    ----------
    api_key : str, optional
        The Alpaca API key. If None, sources from environment variable.
    api_secret : str, optional
        The Alpaca API secret. If None, sources from environment variable.
    paper : bool, default True
        If True, uses paper trading endpoints for trading streams.
    stream_type : str, default "trading"
        Stream type: "trading", "stocks", or "crypto".
    data_feed : AlpacaDataFeed, default IEX
        Data feed for market data streams.
    base_url : str, optional
        Custom WebSocket URL override.
    logger : Logger, optional
        The logger instance.

    """

    def __init__(
        self,
        api_key: str | None = None,
        api_secret: str | None = None,
        paper: bool = True,
        stream_type: str = "trading",
        data_feed: AlpacaDataFeed = AlpacaDataFeed.IEX,
        base_url: str | None = None,
        logger: Logger | None = None,
    ) -> None:
        # Resolve API credentials
        if paper:
            self._api_key = (
                api_key
                or os.environ.get("ALPACA_PAPER_API_KEY")
                or os.environ.get("ALPACA_API_KEY")
            )
            self._api_secret = (
                api_secret
                or os.environ.get("ALPACA_PAPER_API_SECRET")
                or os.environ.get("ALPACA_API_SECRET")
            )
        else:
            self._api_key = api_key or os.environ.get("ALPACA_API_KEY")
            self._api_secret = api_secret or os.environ.get("ALPACA_API_SECRET")

        if not self._api_key or not self._api_secret:
            raise ValueError(
                "Alpaca API credentials not provided. Set ALPACA_API_KEY and ALPACA_API_SECRET "
                "environment variables or pass them directly."
            )

        self._paper = paper
        self._stream_type = stream_type
        self._data_feed = data_feed
        self._logger = logger

        # Determine WebSocket URL
        if base_url:
            self._url = base_url
        elif stream_type == "trading":
            self._url = ALPACA_PAPER_STREAM_URL if paper else ALPACA_LIVE_STREAM_URL
        else:
            # Market data streams
            feed = data_feed.value
            if stream_type == "stocks":
                self._url = f"{ALPACA_DATA_STREAM_URL}/v2/{feed}"
            elif stream_type == "crypto":
                self._url = f"{ALPACA_DATA_STREAM_URL}/v1beta3/crypto/us"
            else:
                raise ValueError(f"Unknown stream type: {stream_type}")

        self._session: aiohttp.ClientSession | None = None
        self._ws: aiohttp.ClientWebSocketResponse | None = None
        self._handler: Callable[[dict], None] | None = None
        self._receive_task: asyncio.Task | None = None
        self._reconnect_task: asyncio.Task | None = None
        self._is_connected = False
        self._is_authenticated = False
        self._should_reconnect = True
        self._subscriptions: dict[str, set[str]] = {
            "trades": set(),
            "quotes": set(),
            "bars": set(),
            "orderbooks": set(),
        }
        self._reconnect_delay = 1.0
        self._max_reconnect_delay = 60.0

    @property
    def url(self) -> str:
        """Return the WebSocket URL."""
        return self._url

    @property
    def is_connected(self) -> bool:
        """Return True if connected."""
        return self._is_connected

    @property
    def is_authenticated(self) -> bool:
        """Return True if authenticated."""
        return self._is_authenticated

    def is_closed(self) -> bool:
        """Return True if the WebSocket is closed."""
        return self._ws is None or self._ws.closed

    async def connect(
        self,
        handler: Callable[[dict], None],
    ) -> None:
        """
        Connect to the WebSocket and start receiving messages.

        Parameters
        ----------
        handler : Callable[[dict], None]
            Callback function to handle incoming messages.

        """
        self._handler = handler
        self._should_reconnect = True

        await self._connect_internal()

    async def _connect_internal(self) -> None:
        """Internal connection logic."""
        if self._session is None or self._session.closed:
            self._session = aiohttp.ClientSession()

        try:
            if self._logger:
                self._logger.info(f"Connecting to Alpaca WebSocket: {self._url}")

            self._ws = await self._session.ws_connect(self._url)
            self._is_connected = True

            # Authenticate
            await self._authenticate()

            # Start receive loop
            self._receive_task = asyncio.create_task(self._receive_loop())

            # Resubscribe if reconnecting
            await self._resubscribe()

            self._reconnect_delay = 1.0  # Reset delay on successful connection

            if self._logger:
                self._logger.info("Connected and authenticated to Alpaca WebSocket")

        except Exception as e:
            self._is_connected = False
            if self._logger:
                self._logger.error(f"Failed to connect to Alpaca WebSocket: {e}")
            raise

    async def _authenticate(self) -> None:
        """Authenticate with the WebSocket."""
        auth_msg = {
            "action": "auth",
            "key": self._api_key,
            "secret": self._api_secret,
        }

        await self._send(auth_msg)

        # Wait for authentication response
        if self._ws:
            msg = await self._ws.receive()
            if msg.type == aiohttp.WSMsgType.TEXT:
                data = json.loads(msg.data)
                if isinstance(data, list):
                    for item in data:
                        if item.get("T") == "success" and item.get("msg") == "authenticated":
                            self._is_authenticated = True
                            return
                        elif item.get("T") == "error":
                            raise Exception(f"Authentication failed: {item.get('msg')}")

        raise Exception("Authentication failed: no valid response")

    async def _send(self, msg: dict) -> None:
        """Send a message over the WebSocket."""
        if self._ws and not self._ws.closed:
            await self._ws.send_json(msg)

    async def _receive_loop(self) -> None:
        """Main receive loop for WebSocket messages."""
        while self._ws and not self._ws.closed:
            try:
                msg = await self._ws.receive()

                if msg.type == aiohttp.WSMsgType.TEXT:
                    data = json.loads(msg.data)
                    if self._handler:
                        if isinstance(data, list):
                            for item in data:
                                self._handler(item)
                        else:
                            self._handler(data)

                elif msg.type == aiohttp.WSMsgType.CLOSED:
                    if self._logger:
                        self._logger.warning("WebSocket closed")
                    break

                elif msg.type == aiohttp.WSMsgType.ERROR:
                    if self._logger:
                        self._logger.error(f"WebSocket error: {msg.data}")
                    break

            except asyncio.CancelledError:
                break
            except Exception as e:
                if self._logger:
                    self._logger.error(f"Error in receive loop: {e}")
                break

        self._is_connected = False
        self._is_authenticated = False

        # Attempt reconnection if needed
        if self._should_reconnect:
            self._reconnect_task = asyncio.create_task(self._reconnect())

    async def _reconnect(self) -> None:
        """Attempt to reconnect with exponential backoff."""
        while self._should_reconnect:
            if self._logger:
                self._logger.info(f"Attempting reconnection in {self._reconnect_delay}s...")

            await asyncio.sleep(self._reconnect_delay)

            try:
                await self._connect_internal()
                return
            except Exception as e:
                if self._logger:
                    self._logger.error(f"Reconnection failed: {e}")
                self._reconnect_delay = min(
                    self._reconnect_delay * 2,
                    self._max_reconnect_delay,
                )

    async def _resubscribe(self) -> None:
        """Resubscribe to all active subscriptions after reconnection."""
        for sub_type, symbols in self._subscriptions.items():
            if symbols:
                msg = {
                    "action": "subscribe",
                    sub_type: list(symbols),
                }
                await self._send(msg)

    async def close(self) -> None:
        """Close the WebSocket connection."""
        self._should_reconnect = False

        if self._receive_task:
            self._receive_task.cancel()
            try:
                await self._receive_task
            except asyncio.CancelledError:
                pass
            self._receive_task = None

        if self._reconnect_task:
            self._reconnect_task.cancel()
            try:
                await self._reconnect_task
            except asyncio.CancelledError:
                pass
            self._reconnect_task = None

        if self._ws and not self._ws.closed:
            await self._ws.close()
            self._ws = None

        if self._session and not self._session.closed:
            await self._session.close()
            self._session = None

        self._is_connected = False
        self._is_authenticated = False

    # -------------------------------------------------------------------------
    # Subscription methods
    # -------------------------------------------------------------------------

    async def subscribe_trades(self, symbols: list[str]) -> None:
        """
        Subscribe to trade updates.

        Parameters
        ----------
        symbols : list[str]
            List of symbols to subscribe to.

        """
        self._subscriptions["trades"].update(symbols)
        msg = {
            "action": "subscribe",
            "trades": symbols,
        }
        await self._send(msg)

    async def unsubscribe_trades(self, symbols: list[str]) -> None:
        """
        Unsubscribe from trade updates.

        Parameters
        ----------
        symbols : list[str]
            List of symbols to unsubscribe from.

        """
        self._subscriptions["trades"].difference_update(symbols)
        msg = {
            "action": "unsubscribe",
            "trades": symbols,
        }
        await self._send(msg)

    async def subscribe_quotes(self, symbols: list[str]) -> None:
        """
        Subscribe to quote updates.

        Parameters
        ----------
        symbols : list[str]
            List of symbols to subscribe to.

        """
        self._subscriptions["quotes"].update(symbols)
        msg = {
            "action": "subscribe",
            "quotes": symbols,
        }
        await self._send(msg)

    async def unsubscribe_quotes(self, symbols: list[str]) -> None:
        """
        Unsubscribe from quote updates.

        Parameters
        ----------
        symbols : list[str]
            List of symbols to unsubscribe from.

        """
        self._subscriptions["quotes"].difference_update(symbols)
        msg = {
            "action": "unsubscribe",
            "quotes": symbols,
        }
        await self._send(msg)

    async def subscribe_bars(self, symbols: list[str]) -> None:
        """
        Subscribe to bar updates.

        Parameters
        ----------
        symbols : list[str]
            List of symbols to subscribe to.

        """
        self._subscriptions["bars"].update(symbols)
        msg = {
            "action": "subscribe",
            "bars": symbols,
        }
        await self._send(msg)

    async def unsubscribe_bars(self, symbols: list[str]) -> None:
        """
        Unsubscribe from bar updates.

        Parameters
        ----------
        symbols : list[str]
            List of symbols to unsubscribe from.

        """
        self._subscriptions["bars"].difference_update(symbols)
        msg = {
            "action": "unsubscribe",
            "bars": symbols,
        }
        await self._send(msg)

    async def subscribe_orderbooks(self, symbols: list[str]) -> None:
        """
        Subscribe to orderbook updates (crypto only).

        Parameters
        ----------
        symbols : list[str]
            List of symbols to subscribe to.

        """
        self._subscriptions["orderbooks"].update(symbols)
        msg = {
            "action": "subscribe",
            "orderbooks": symbols,
        }
        await self._send(msg)

    async def unsubscribe_orderbooks(self, symbols: list[str]) -> None:
        """
        Unsubscribe from orderbook updates.

        Parameters
        ----------
        symbols : list[str]
            List of symbols to unsubscribe from.

        """
        self._subscriptions["orderbooks"].difference_update(symbols)
        msg = {
            "action": "unsubscribe",
            "orderbooks": symbols,
        }
        await self._send(msg)


class AlpacaTradingWebSocketClient(AlpacaWebSocketClient):
    """
    WebSocket client for Alpaca trading events (orders, fills, account updates).

    Inherits from AlpacaWebSocketClient with trading-specific functionality.

    Parameters
    ----------
    api_key : str, optional
        The Alpaca API key. If None, sources from environment variable.
    api_secret : str, optional
        The Alpaca API secret. If None, sources from environment variable.
    paper : bool, default True
        If True, uses paper trading endpoints.
    base_url : str, optional
        Custom WebSocket URL override.
    logger : Logger, optional
        The logger instance.

    """

    def __init__(
        self,
        api_key: str | None = None,
        api_secret: str | None = None,
        paper: bool = True,
        base_url: str | None = None,
        logger: Logger | None = None,
    ) -> None:
        super().__init__(
            api_key=api_key,
            api_secret=api_secret,
            paper=paper,
            stream_type="trading",
            base_url=base_url,
            logger=logger,
        )
        self._trade_updates_subscribed = False

    async def _authenticate(self) -> None:
        """Authenticate with the trading WebSocket."""
        # Trading stream uses different auth format
        auth_msg = {
            "action": "authenticate",
            "data": {
                "key_id": self._api_key,
                "secret_key": self._api_secret,
            },
        }

        await self._send(auth_msg)

        # Wait for authentication response
        if self._ws:
            msg = await self._ws.receive()
            if msg.type == aiohttp.WSMsgType.TEXT:
                data = json.loads(msg.data)
                if data.get("stream") == "authorization":
                    if data.get("data", {}).get("status") == "authorized":
                        self._is_authenticated = True
                        return
                    else:
                        raise Exception(f"Authentication failed: {data}")

        raise Exception("Authentication failed: no valid response")

    async def subscribe_trade_updates(self) -> None:
        """Subscribe to trade (order) updates."""
        if not self._trade_updates_subscribed:
            msg = {
                "action": "listen",
                "data": {
                    "streams": ["trade_updates"],
                },
            }
            await self._send(msg)
            self._trade_updates_subscribed = True

    async def _resubscribe(self) -> None:
        """Resubscribe after reconnection."""
        if self._trade_updates_subscribed:
            await self.subscribe_trade_updates()
