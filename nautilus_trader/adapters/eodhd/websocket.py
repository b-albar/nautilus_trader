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
EODHD WebSocket client for real-time data streaming.
"""

import asyncio
import json
from typing import Any
from typing import Callable

import aiohttp

from nautilus_trader.adapters.eodhd.constants import EODHD_BASE_URL_WS
from nautilus_trader.adapters.eodhd.enums import EodhdAssetType
from nautilus_trader.common.component import Logger


class EodhdWebSocketClient:
    """
    WebSocket client for EODHD real-time data streaming.

    Provides real-time trade and quote data for US equities, FOREX, and cryptocurrencies.

    Parameters
    ----------
    api_key : str
        The EODHD API key.
    asset_type : EodhdAssetType
        The asset type for this connection (determines endpoint).
    base_url : str, optional
        The base URL for the WebSocket API. Defaults to EODHD_BASE_URL_WS.
    on_message : Callable[[dict], None], optional
        Callback for incoming messages.
    logger : Logger, optional
        The logger for the client.

    """

    def __init__(
        self,
        api_key: str,
        asset_type: EodhdAssetType,
        base_url: str | None = None,
        on_message: Callable[[dict], None] | None = None,
        logger: Logger | None = None,
    ) -> None:
        self._api_key = api_key
        self._asset_type = asset_type
        self._base_url = base_url or EODHD_BASE_URL_WS
        self._on_message = on_message
        self._log = logger

        self._session: aiohttp.ClientSession | None = None
        self._ws: aiohttp.ClientWebSocketResponse | None = None
        self._subscribed_symbols: set[str] = set()
        self._is_connected: bool = False
        self._reconnect_task: asyncio.Task | None = None
        self._receive_task: asyncio.Task | None = None

    @property
    def is_connected(self) -> bool:
        """Return True if WebSocket is connected."""
        return self._is_connected and self._ws is not None and not self._ws.closed

    @property
    def subscribed_symbols(self) -> set[str]:
        """Return the set of subscribed symbols."""
        return self._subscribed_symbols.copy()

    def _get_ws_url(self) -> str:
        """Build the WebSocket URL for the asset type."""
        return f"{self._base_url}/{self._asset_type.value}?api_token={self._api_key}"

    async def connect(self) -> None:
        """
        Establish WebSocket connection.

        Raises
        ------
        aiohttp.ClientError
            If connection fails.

        """
        if self.is_connected:
            return

        if self._session is None or self._session.closed:
            self._session = aiohttp.ClientSession()

        url = self._get_ws_url()

        try:
            self._ws = await self._session.ws_connect(url)
            self._is_connected = True

            if self._log:
                self._log.info(f"Connected to EODHD WebSocket ({self._asset_type.value})")

            # Start receiving messages
            self._receive_task = asyncio.create_task(self._receive_messages())

            # Resubscribe if we had symbols before
            if self._subscribed_symbols:
                await self._send_subscribe(list(self._subscribed_symbols))

        except Exception as e:
            self._is_connected = False
            if self._log:
                self._log.error(f"Failed to connect to EODHD WebSocket: {e}")
            raise

    async def disconnect(self) -> None:
        """Close WebSocket connection."""
        self._is_connected = False

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

        if self._log:
            self._log.info(f"Disconnected from EODHD WebSocket ({self._asset_type.value})")

    async def subscribe(self, symbols: list[str]) -> None:
        """
        Subscribe to real-time data for symbols.

        Parameters
        ----------
        symbols : list[str]
            List of symbols to subscribe to.

        """
        if not symbols:
            return

        self._subscribed_symbols.update(symbols)

        if self.is_connected:
            await self._send_subscribe(symbols)
        else:
            if self._log:
                self._log.warning("WebSocket not connected, symbols queued for subscription")

    async def unsubscribe(self, symbols: list[str]) -> None:
        """
        Unsubscribe from real-time data for symbols.

        Parameters
        ----------
        symbols : list[str]
            List of symbols to unsubscribe from.

        """
        if not symbols:
            return

        self._subscribed_symbols.difference_update(symbols)

        if self.is_connected:
            await self._send_unsubscribe(symbols)

    async def _send_subscribe(self, symbols: list[str]) -> None:
        """Send subscribe message to WebSocket."""
        if not self._ws:
            return

        message = {
            "action": "subscribe",
            "symbols": ",".join(symbols),
        }
        await self._ws.send_str(json.dumps(message))

        if self._log:
            self._log.debug(f"Subscribed to symbols: {symbols}")

    async def _send_unsubscribe(self, symbols: list[str]) -> None:
        """Send unsubscribe message to WebSocket."""
        if not self._ws:
            return

        message = {
            "action": "unsubscribe",
            "symbols": ",".join(symbols),
        }
        await self._ws.send_str(json.dumps(message))

        if self._log:
            self._log.debug(f"Unsubscribed from symbols: {symbols}")

    async def _receive_messages(self) -> None:
        """Receive and process WebSocket messages."""
        if not self._ws:
            return

        try:
            async for msg in self._ws:
                if msg.type == aiohttp.WSMsgType.TEXT:
                    try:
                        data = json.loads(msg.data)
                        if self._on_message:
                            self._on_message(data)
                    except json.JSONDecodeError as e:
                        if self._log:
                            self._log.warning(f"Failed to parse WebSocket message: {e}")

                elif msg.type == aiohttp.WSMsgType.ERROR:
                    if self._log:
                        self._log.error(f"WebSocket error: {self._ws.exception()}")
                    break

                elif msg.type == aiohttp.WSMsgType.CLOSED:
                    if self._log:
                        self._log.warning("WebSocket connection closed")
                    break

        except asyncio.CancelledError:
            pass
        except Exception as e:
            if self._log:
                self._log.error(f"Error receiving WebSocket messages: {e}")

        finally:
            self._is_connected = False
            # Attempt reconnection if we still have subscriptions
            if self._subscribed_symbols:
                self._reconnect_task = asyncio.create_task(self._reconnect())

    async def _reconnect(self, max_retries: int = 5, base_delay: float = 1.0) -> None:
        """
        Attempt to reconnect to WebSocket with exponential backoff.

        Parameters
        ----------
        max_retries : int, default 5
            Maximum number of reconnection attempts.
        base_delay : float, default 1.0
            Base delay in seconds for exponential backoff.

        """
        for attempt in range(max_retries):
            delay = base_delay * (2**attempt)

            if self._log:
                self._log.info(
                    f"Reconnecting in {delay:.1f}s (attempt {attempt + 1}/{max_retries})"
                )

            await asyncio.sleep(delay)

            try:
                await self.connect()
                if self.is_connected:
                    if self._log:
                        self._log.info("Successfully reconnected to EODHD WebSocket")
                    return
            except Exception as e:
                if self._log:
                    self._log.warning(f"Reconnection attempt {attempt + 1} failed: {e}")

        if self._log:
            self._log.error("Max reconnection attempts reached, giving up")
