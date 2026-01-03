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
Alpaca HTTP client for REST API interactions.
"""

from __future__ import annotations

import asyncio
import os
from datetime import datetime
from decimal import Decimal
from typing import Any

import aiohttp

from nautilus_trader.adapters.alpaca.constants import ALPACA_DATA_BASE_URL
from nautilus_trader.adapters.alpaca.constants import ALPACA_LIVE_BASE_URL
from nautilus_trader.adapters.alpaca.constants import ALPACA_PAPER_BASE_URL
from nautilus_trader.adapters.alpaca.enums import AlpacaAssetClass
from nautilus_trader.adapters.alpaca.enums import AlpacaBarTimeframe
from nautilus_trader.adapters.alpaca.enums import AlpacaDataFeed
from nautilus_trader.adapters.alpaca.enums import AlpacaOrderSide
from nautilus_trader.adapters.alpaca.enums import AlpacaOrderType
from nautilus_trader.adapters.alpaca.enums import AlpacaTimeInForce
from nautilus_trader.common.component import Logger
from nautilus_trader.core.datetime import dt_to_unix_nanos
from nautilus_trader.core.datetime import unix_nanos_to_dt
from nautilus_trader.core.uuid import UUID4
from nautilus_trader.model.data import Bar
from nautilus_trader.model.data import BarType
from nautilus_trader.model.data import QuoteTick
from nautilus_trader.model.data import TradeTick
from nautilus_trader.model.enums import AggressorSide
from nautilus_trader.model.enums import OrderSide
from nautilus_trader.model.enums import OrderType
from nautilus_trader.model.enums import TimeInForce
from nautilus_trader.model.identifiers import InstrumentId
from nautilus_trader.model.identifiers import TradeId
from nautilus_trader.model.objects import Price
from nautilus_trader.model.objects import Quantity


class AlpacaHttpClient:
    """
    HTTP client for the Alpaca REST API.

    Supports both Trading API and Market Data API endpoints.

    Parameters
    ----------
    api_key : str, optional
        The Alpaca API key. If None, sources from environment variable.
    api_secret : str, optional
        The Alpaca API secret. If None, sources from environment variable.
    paper : bool, default True
        If True, uses paper trading endpoints.
    base_url_http : str, optional
        Custom base URL for trading API.
    base_url_data : str, optional
        Custom base URL for market data API.
    timeout_secs : int, default 10
        Request timeout in seconds.
    logger : Logger, optional
        The logger instance.

    """

    def __init__(
        self,
        api_key: str | None = None,
        api_secret: str | None = None,
        paper: bool = True,
        base_url_http: str | None = None,
        base_url_data: str | None = None,
        timeout_secs: int = 10,
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
        self._base_url = base_url_http or (ALPACA_PAPER_BASE_URL if paper else ALPACA_LIVE_BASE_URL)
        self._data_url = base_url_data or ALPACA_DATA_BASE_URL
        self._timeout = aiohttp.ClientTimeout(total=timeout_secs)
        self._logger = logger
        self._session: aiohttp.ClientSession | None = None

    @property
    def api_key(self) -> str:
        """Return the API key."""
        return self._api_key

    @property
    def is_paper(self) -> bool:
        """Return True if using paper trading."""
        return self._paper

    def _headers(self) -> dict[str, str]:
        """Return the request headers with authentication."""
        return {
            "APCA-API-KEY-ID": self._api_key,
            "APCA-API-SECRET-KEY": self._api_secret,
            "Content-Type": "application/json",
        }

    async def _ensure_session(self) -> aiohttp.ClientSession:
        """Ensure the HTTP session is created."""
        if self._session is None or self._session.closed:
            self._session = aiohttp.ClientSession(timeout=self._timeout)
        return self._session

    async def close(self) -> None:
        """Close the HTTP session."""
        if self._session and not self._session.closed:
            await self._session.close()
            self._session = None

    async def _request(
        self,
        method: str,
        url: str,
        params: dict | None = None,
        json_data: dict | None = None,
    ) -> dict | list | None:
        """
        Make an HTTP request.

        Parameters
        ----------
        method : str
            HTTP method (GET, POST, PATCH, DELETE).
        url : str
            Full URL for the request.
        params : dict, optional
            Query parameters.
        json_data : dict, optional
            JSON body for POST/PATCH requests.

        Returns
        -------
        dict | list | None
            Response data.

        Raises
        ------
        AlpacaApiError
            If the request fails.

        """
        session = await self._ensure_session()

        try:
            async with session.request(
                method=method,
                url=url,
                headers=self._headers(),
                params=params,
                json=json_data,
            ) as response:
                if response.status == 204:
                    return None

                data = await response.json()

                if response.status >= 400:
                    error_msg = data.get("message", str(data))
                    raise AlpacaApiError(
                        status_code=response.status,
                        message=error_msg,
                        response_data=data,
                    )

                return data

        except aiohttp.ClientError as e:
            raise AlpacaApiError(
                status_code=0,
                message=f"Request failed: {e}",
            ) from e

    # -------------------------------------------------------------------------
    # Trading API - Account
    # -------------------------------------------------------------------------

    async def get_account(self) -> dict:
        """
        Get the current account information.

        Returns
        -------
        dict
            Account data including buying power, portfolio value, etc.

        """
        url = f"{self._base_url}/v2/account"
        return await self._request("GET", url)

    async def get_account_configurations(self) -> dict:
        """
        Get account configuration settings.

        Returns
        -------
        dict
            Account configuration data.

        """
        url = f"{self._base_url}/v2/account/configurations"
        return await self._request("GET", url)

    async def update_account_configurations(self, config: dict) -> dict:
        """
        Update account configuration settings.

        Parameters
        ----------
        config : dict
            Configuration values to update.

        Returns
        -------
        dict
            Updated account configuration.

        """
        url = f"{self._base_url}/v2/account/configurations"
        return await self._request("PATCH", url, json_data=config)

    # -------------------------------------------------------------------------
    # Trading API - Orders
    # -------------------------------------------------------------------------

    async def create_order(
        self,
        symbol: str,
        qty: str | None = None,
        notional: str | None = None,
        side: AlpacaOrderSide = AlpacaOrderSide.BUY,
        order_type: AlpacaOrderType = AlpacaOrderType.MARKET,
        time_in_force: AlpacaTimeInForce = AlpacaTimeInForce.DAY,
        limit_price: str | None = None,
        stop_price: str | None = None,
        trail_price: str | None = None,
        trail_percent: str | None = None,
        extended_hours: bool = False,
        client_order_id: str | None = None,
        order_class: str | None = None,
        take_profit: dict | None = None,
        stop_loss: dict | None = None,
    ) -> dict:
        """
        Create a new order.

        Parameters
        ----------
        symbol : str
            The symbol to trade.
        qty : str, optional
            Number of shares to trade (for share-based orders).
        notional : str, optional
            Dollar amount to trade (for dollar-based orders, market orders only).
        side : AlpacaOrderSide
            Buy or sell.
        order_type : AlpacaOrderType
            Order type.
        time_in_force : AlpacaTimeInForce
            Time in force.
        limit_price : str, optional
            Limit price (required for limit orders).
        stop_price : str, optional
            Stop price (required for stop orders).
        trail_price : str, optional
            Trail amount in dollars for trailing stop.
        trail_percent : str, optional
            Trail amount in percent for trailing stop.
        extended_hours : bool, default False
            If True, allows trading during extended hours.
        client_order_id : str, optional
            Client-specified order ID.
        order_class : str, optional
            Order class (simple, bracket, oco, oto).
        take_profit : dict, optional
            Take profit configuration for bracket orders.
        stop_loss : dict, optional
            Stop loss configuration for bracket orders.

        Returns
        -------
        dict
            Order data.

        """
        url = f"{self._base_url}/v2/orders"

        order_data = {
            "symbol": symbol,
            "side": side.value,
            "type": order_type.value,
            "time_in_force": time_in_force.value,
        }

        if qty is not None:
            order_data["qty"] = qty
        if notional is not None:
            order_data["notional"] = notional
        if limit_price is not None:
            order_data["limit_price"] = limit_price
        if stop_price is not None:
            order_data["stop_price"] = stop_price
        if trail_price is not None:
            order_data["trail_price"] = trail_price
        if trail_percent is not None:
            order_data["trail_percent"] = trail_percent
        if extended_hours:
            order_data["extended_hours"] = extended_hours
        if client_order_id is not None:
            order_data["client_order_id"] = client_order_id
        if order_class is not None:
            order_data["order_class"] = order_class
        if take_profit is not None:
            order_data["take_profit"] = take_profit
        if stop_loss is not None:
            order_data["stop_loss"] = stop_loss

        return await self._request("POST", url, json_data=order_data)

    async def get_orders(
        self,
        status: str = "open",
        limit: int = 50,
        after: datetime | None = None,
        until: datetime | None = None,
        direction: str = "desc",
        nested: bool = False,
        symbols: list[str] | None = None,
        side: str | None = None,
    ) -> list[dict]:
        """
        Get a list of orders.

        Parameters
        ----------
        status : str
            Order status filter (open, closed, all).
        limit : int
            Maximum number of orders to return.
        after : datetime, optional
            Filter for orders after this time.
        until : datetime, optional
            Filter for orders until this time.
        direction : str
            Sort direction (asc, desc).
        nested : bool
            If True, includes nested orders for bracket/OCO orders.
        symbols : list[str], optional
            Filter by symbols.
        side : str, optional
            Filter by side (buy, sell).

        Returns
        -------
        list[dict]
            List of order data.

        """
        url = f"{self._base_url}/v2/orders"
        params = {
            "status": status,
            "limit": limit,
            "direction": direction,
            "nested": str(nested).lower(),
        }

        if after:
            params["after"] = after.isoformat()
        if until:
            params["until"] = until.isoformat()
        if symbols:
            params["symbols"] = ",".join(symbols)
        if side:
            params["side"] = side

        return await self._request("GET", url, params=params)

    async def get_order_by_id(self, order_id: str, nested: bool = False) -> dict:
        """
        Get an order by its ID.

        Parameters
        ----------
        order_id : str
            The order ID.
        nested : bool
            If True, includes nested orders.

        Returns
        -------
        dict
            Order data.

        """
        url = f"{self._base_url}/v2/orders/{order_id}"
        params = {"nested": str(nested).lower()}
        return await self._request("GET", url, params=params)

    async def get_order_by_client_id(self, client_order_id: str) -> dict:
        """
        Get an order by its client order ID.

        Parameters
        ----------
        client_order_id : str
            The client order ID.

        Returns
        -------
        dict
            Order data.

        """
        url = f"{self._base_url}/v2/orders:by_client_order_id"
        params = {"client_order_id": client_order_id}
        return await self._request("GET", url, params=params)

    async def replace_order(
        self,
        order_id: str,
        qty: str | None = None,
        time_in_force: AlpacaTimeInForce | None = None,
        limit_price: str | None = None,
        stop_price: str | None = None,
        trail: str | None = None,
        client_order_id: str | None = None,
    ) -> dict:
        """
        Replace (modify) an existing order.

        Parameters
        ----------
        order_id : str
            The order ID to replace.
        qty : str, optional
            New quantity.
        time_in_force : AlpacaTimeInForce, optional
            New time in force.
        limit_price : str, optional
            New limit price.
        stop_price : str, optional
            New stop price.
        trail : str, optional
            New trail amount.
        client_order_id : str, optional
            New client order ID.

        Returns
        -------
        dict
            Updated order data.

        """
        url = f"{self._base_url}/v2/orders/{order_id}"

        data = {}
        if qty is not None:
            data["qty"] = qty
        if time_in_force is not None:
            data["time_in_force"] = time_in_force.value
        if limit_price is not None:
            data["limit_price"] = limit_price
        if stop_price is not None:
            data["stop_price"] = stop_price
        if trail is not None:
            data["trail"] = trail
        if client_order_id is not None:
            data["client_order_id"] = client_order_id

        return await self._request("PATCH", url, json_data=data)

    async def cancel_order(self, order_id: str) -> None:
        """
        Cancel an order by ID.

        Parameters
        ----------
        order_id : str
            The order ID to cancel.

        """
        url = f"{self._base_url}/v2/orders/{order_id}"
        await self._request("DELETE", url)

    async def cancel_all_orders(self) -> list[dict]:
        """
        Cancel all open orders.

        Returns
        -------
        list[dict]
            List of canceled order data.

        """
        url = f"{self._base_url}/v2/orders"
        return await self._request("DELETE", url)

    # -------------------------------------------------------------------------
    # Trading API - Positions
    # -------------------------------------------------------------------------

    async def get_all_positions(self) -> list[dict]:
        """
        Get all open positions.

        Returns
        -------
        list[dict]
            List of position data.

        """
        url = f"{self._base_url}/v2/positions"
        return await self._request("GET", url)

    async def get_position(self, symbol_or_asset_id: str) -> dict:
        """
        Get a position by symbol or asset ID.

        Parameters
        ----------
        symbol_or_asset_id : str
            Symbol or asset ID.

        Returns
        -------
        dict
            Position data.

        """
        url = f"{self._base_url}/v2/positions/{symbol_or_asset_id}"
        return await self._request("GET", url)

    async def close_position(
        self,
        symbol_or_asset_id: str,
        qty: str | None = None,
        percentage: str | None = None,
    ) -> dict:
        """
        Close a position.

        Parameters
        ----------
        symbol_or_asset_id : str
            Symbol or asset ID.
        qty : str, optional
            Quantity to close (partial close).
        percentage : str, optional
            Percentage to close.

        Returns
        -------
        dict
            Order data for the closing order.

        """
        url = f"{self._base_url}/v2/positions/{symbol_or_asset_id}"
        params = {}
        if qty is not None:
            params["qty"] = qty
        if percentage is not None:
            params["percentage"] = percentage
        return await self._request("DELETE", url, params=params)

    async def close_all_positions(self, cancel_orders: bool = False) -> list[dict]:
        """
        Close all positions.

        Parameters
        ----------
        cancel_orders : bool
            If True, also cancel all open orders.

        Returns
        -------
        list[dict]
            List of closing order data.

        """
        url = f"{self._base_url}/v2/positions"
        params = {"cancel_orders": str(cancel_orders).lower()}
        return await self._request("DELETE", url, params=params)

    # -------------------------------------------------------------------------
    # Trading API - Assets
    # -------------------------------------------------------------------------

    async def get_assets(
        self,
        status: str | None = None,
        asset_class: AlpacaAssetClass | None = None,
        exchange: str | None = None,
    ) -> list[dict]:
        """
        Get a list of assets.

        Parameters
        ----------
        status : str, optional
            Asset status filter (active, inactive).
        asset_class : AlpacaAssetClass, optional
            Asset class filter.
        exchange : str, optional
            Exchange filter.

        Returns
        -------
        list[dict]
            List of asset data.

        """
        url = f"{self._base_url}/v2/assets"
        params = {}
        if status:
            params["status"] = status
        if asset_class:
            params["asset_class"] = asset_class.value
        if exchange:
            params["exchange"] = exchange
        return await self._request("GET", url, params=params)

    async def get_asset(self, symbol_or_asset_id: str) -> dict:
        """
        Get an asset by symbol or asset ID.

        Parameters
        ----------
        symbol_or_asset_id : str
            Symbol or asset ID.

        Returns
        -------
        dict
            Asset data.

        """
        url = f"{self._base_url}/v2/assets/{symbol_or_asset_id}"
        return await self._request("GET", url)

    # -------------------------------------------------------------------------
    # Trading API - Calendar and Clock
    # -------------------------------------------------------------------------

    async def get_clock(self) -> dict:
        """
        Get the current market clock.

        Returns
        -------
        dict
            Market clock data (is_open, next_open, next_close).

        """
        url = f"{self._base_url}/v2/clock"
        return await self._request("GET", url)

    async def get_calendar(
        self,
        start: datetime | None = None,
        end: datetime | None = None,
    ) -> list[dict]:
        """
        Get the market calendar.

        Parameters
        ----------
        start : datetime, optional
            Start date for calendar.
        end : datetime, optional
            End date for calendar.

        Returns
        -------
        list[dict]
            List of calendar days.

        """
        url = f"{self._base_url}/v2/calendar"
        params = {}
        if start:
            params["start"] = start.strftime("%Y-%m-%d")
        if end:
            params["end"] = end.strftime("%Y-%m-%d")
        return await self._request("GET", url, params=params)

    # -------------------------------------------------------------------------
    # Trading API - Account Activities
    # -------------------------------------------------------------------------

    async def get_account_activities(
        self,
        activity_types: list[str] | None = None,
        after: datetime | None = None,
        until: datetime | None = None,
        direction: str = "desc",
        page_size: int = 100,
        page_token: str | None = None,
    ) -> list[dict]:
        """
        Get account activities.

        Parameters
        ----------
        activity_types : list[str], optional
            Filter by activity types (FILL, TRANS, etc.).
        after : datetime, optional
            Filter for activities after this time.
        until : datetime, optional
            Filter for activities until this time.
        direction : str
            Sort direction (asc, desc).
        page_size : int
            Number of activities per page.
        page_token : str, optional
            Pagination token.

        Returns
        -------
        list[dict]
            List of activity data.

        """
        url = f"{self._base_url}/v2/account/activities"
        params = {
            "direction": direction,
            "page_size": page_size,
        }
        if activity_types:
            params["activity_types"] = ",".join(activity_types)
        if after:
            params["after"] = after.isoformat()
        if until:
            params["until"] = until.isoformat()
        if page_token:
            params["page_token"] = page_token
        return await self._request("GET", url, params=params)

    # -------------------------------------------------------------------------
    # Trading API - Portfolio History
    # -------------------------------------------------------------------------

    async def get_portfolio_history(
        self,
        period: str | None = None,
        timeframe: str | None = None,
        intraday_reporting: str | None = None,
        start: datetime | None = None,
        end: datetime | None = None,
        pnl_reset: str | None = None,
    ) -> dict:
        """
        Get portfolio history.

        Parameters
        ----------
        period : str, optional
            Period for history (1D, 1W, 1M, 3M, 1A, all, intraday).
        timeframe : str, optional
            Timeframe for data points (1Min, 5Min, 15Min, 1H, 1D).
        intraday_reporting : str, optional
            How to report intraday (market_hours, extended_hours, continuous).
        start : datetime, optional
            Start date.
        end : datetime, optional
            End date.
        pnl_reset : str, optional
            When to reset PnL (per_day, no_reset).

        Returns
        -------
        dict
            Portfolio history data.

        """
        url = f"{self._base_url}/v2/account/portfolio/history"
        params = {}
        if period:
            params["period"] = period
        if timeframe:
            params["timeframe"] = timeframe
        if intraday_reporting:
            params["intraday_reporting"] = intraday_reporting
        if start:
            params["start"] = start.isoformat()
        if end:
            params["end"] = end.isoformat()
        if pnl_reset:
            params["pnl_reset"] = pnl_reset
        return await self._request("GET", url, params=params)

    # -------------------------------------------------------------------------
    # Market Data API - Stocks
    # -------------------------------------------------------------------------

    async def get_stock_bars(
        self,
        symbol: str,
        timeframe: AlpacaBarTimeframe,
        start: datetime | None = None,
        end: datetime | None = None,
        limit: int | None = None,
        feed: AlpacaDataFeed = AlpacaDataFeed.IEX,
        adjustment: str = "raw",
        sort: str = "asc",
    ) -> dict:
        """
        Get historical stock bars.

        Parameters
        ----------
        symbol : str
            Stock symbol.
        timeframe : AlpacaBarTimeframe
            Bar timeframe.
        start : datetime, optional
            Start time.
        end : datetime, optional
            End time.
        limit : int, optional
            Maximum number of bars.
        feed : AlpacaDataFeed
            Data feed (iex or sip).
        adjustment : str
            Price adjustment (raw, split, dividend, all).
        sort : str
            Sort order (asc, desc).

        Returns
        -------
        dict
            Bar data.

        """
        url = f"{self._data_url}/v2/stocks/{symbol}/bars"
        params = {
            "timeframe": timeframe.value,
            "feed": feed.value,
            "adjustment": adjustment,
            "sort": sort,
        }
        if start:
            params["start"] = start.isoformat()
        if end:
            params["end"] = end.isoformat()
        if limit:
            params["limit"] = limit
        return await self._request("GET", url, params=params)

    async def get_stock_trades(
        self,
        symbol: str,
        start: datetime | None = None,
        end: datetime | None = None,
        limit: int | None = None,
        feed: AlpacaDataFeed = AlpacaDataFeed.IEX,
        sort: str = "asc",
    ) -> dict:
        """
        Get historical stock trades.

        Parameters
        ----------
        symbol : str
            Stock symbol.
        start : datetime, optional
            Start time.
        end : datetime, optional
            End time.
        limit : int, optional
            Maximum number of trades.
        feed : AlpacaDataFeed
            Data feed.
        sort : str
            Sort order.

        Returns
        -------
        dict
            Trade data.

        """
        url = f"{self._data_url}/v2/stocks/{symbol}/trades"
        params = {
            "feed": feed.value,
            "sort": sort,
        }
        if start:
            params["start"] = start.isoformat()
        if end:
            params["end"] = end.isoformat()
        if limit:
            params["limit"] = limit
        return await self._request("GET", url, params=params)

    async def get_stock_quotes(
        self,
        symbol: str,
        start: datetime | None = None,
        end: datetime | None = None,
        limit: int | None = None,
        feed: AlpacaDataFeed = AlpacaDataFeed.IEX,
        sort: str = "asc",
    ) -> dict:
        """
        Get historical stock quotes.

        Parameters
        ----------
        symbol : str
            Stock symbol.
        start : datetime, optional
            Start time.
        end : datetime, optional
            End time.
        limit : int, optional
            Maximum number of quotes.
        feed : AlpacaDataFeed
            Data feed.
        sort : str
            Sort order.

        Returns
        -------
        dict
            Quote data.

        """
        url = f"{self._data_url}/v2/stocks/{symbol}/quotes"
        params = {
            "feed": feed.value,
            "sort": sort,
        }
        if start:
            params["start"] = start.isoformat()
        if end:
            params["end"] = end.isoformat()
        if limit:
            params["limit"] = limit
        return await self._request("GET", url, params=params)

    async def get_latest_stock_trade(
        self,
        symbol: str,
        feed: AlpacaDataFeed = AlpacaDataFeed.IEX,
    ) -> dict:
        """
        Get the latest trade for a stock.

        Parameters
        ----------
        symbol : str
            Stock symbol.
        feed : AlpacaDataFeed
            Data feed.

        Returns
        -------
        dict
            Latest trade data.

        """
        url = f"{self._data_url}/v2/stocks/{symbol}/trades/latest"
        params = {"feed": feed.value}
        return await self._request("GET", url, params=params)

    async def get_latest_stock_quote(
        self,
        symbol: str,
        feed: AlpacaDataFeed = AlpacaDataFeed.IEX,
    ) -> dict:
        """
        Get the latest quote for a stock.

        Parameters
        ----------
        symbol : str
            Stock symbol.
        feed : AlpacaDataFeed
            Data feed.

        Returns
        -------
        dict
            Latest quote data.

        """
        url = f"{self._data_url}/v2/stocks/{symbol}/quotes/latest"
        params = {"feed": feed.value}
        return await self._request("GET", url, params=params)

    async def get_stock_snapshot(
        self,
        symbol: str,
        feed: AlpacaDataFeed = AlpacaDataFeed.IEX,
    ) -> dict:
        """
        Get a snapshot for a stock.

        Parameters
        ----------
        symbol : str
            Stock symbol.
        feed : AlpacaDataFeed
            Data feed.

        Returns
        -------
        dict
            Snapshot data.

        """
        url = f"{self._data_url}/v2/stocks/{symbol}/snapshot"
        params = {"feed": feed.value}
        return await self._request("GET", url, params=params)

    # -------------------------------------------------------------------------
    # Market Data API - Crypto
    # -------------------------------------------------------------------------

    async def get_crypto_bars(
        self,
        symbol: str,
        timeframe: AlpacaBarTimeframe,
        start: datetime | None = None,
        end: datetime | None = None,
        limit: int | None = None,
        sort: str = "asc",
    ) -> dict:
        """
        Get historical crypto bars.

        Parameters
        ----------
        symbol : str
            Crypto symbol (e.g., BTC/USD).
        timeframe : AlpacaBarTimeframe
            Bar timeframe.
        start : datetime, optional
            Start time.
        end : datetime, optional
            End time.
        limit : int, optional
            Maximum number of bars.
        sort : str
            Sort order.

        Returns
        -------
        dict
            Bar data.

        """
        url = f"{self._data_url}/v1beta3/crypto/us/bars"
        params = {
            "symbols": symbol,
            "timeframe": timeframe.value,
            "sort": sort,
        }
        if start:
            params["start"] = start.isoformat()
        if end:
            params["end"] = end.isoformat()
        if limit:
            params["limit"] = limit
        return await self._request("GET", url, params=params)

    async def get_crypto_trades(
        self,
        symbol: str,
        start: datetime | None = None,
        end: datetime | None = None,
        limit: int | None = None,
        sort: str = "asc",
    ) -> dict:
        """
        Get historical crypto trades.

        Parameters
        ----------
        symbol : str
            Crypto symbol.
        start : datetime, optional
            Start time.
        end : datetime, optional
            End time.
        limit : int, optional
            Maximum number of trades.
        sort : str
            Sort order.

        Returns
        -------
        dict
            Trade data.

        """
        url = f"{self._data_url}/v1beta3/crypto/us/trades"
        params = {
            "symbols": symbol,
            "sort": sort,
        }
        if start:
            params["start"] = start.isoformat()
        if end:
            params["end"] = end.isoformat()
        if limit:
            params["limit"] = limit
        return await self._request("GET", url, params=params)

    async def get_crypto_quotes(
        self,
        symbol: str,
        start: datetime | None = None,
        end: datetime | None = None,
        limit: int | None = None,
        sort: str = "asc",
    ) -> dict:
        """
        Get historical crypto quotes.

        Parameters
        ----------
        symbol : str
            Crypto symbol.
        start : datetime, optional
            Start time.
        end : datetime, optional
            End time.
        limit : int, optional
            Maximum number of quotes.
        sort : str
            Sort order.

        Returns
        -------
        dict
            Quote data.

        """
        url = f"{self._data_url}/v1beta3/crypto/us/quotes"
        params = {
            "symbols": symbol,
            "sort": sort,
        }
        if start:
            params["start"] = start.isoformat()
        if end:
            params["end"] = end.isoformat()
        if limit:
            params["limit"] = limit
        return await self._request("GET", url, params=params)

    async def get_latest_crypto_trade(self, symbol: str) -> dict:
        """
        Get the latest trade for a crypto symbol.

        Parameters
        ----------
        symbol : str
            Crypto symbol.

        Returns
        -------
        dict
            Latest trade data.

        """
        url = f"{self._data_url}/v1beta3/crypto/us/latest/trades"
        params = {"symbols": symbol}
        return await self._request("GET", url, params=params)

    async def get_latest_crypto_quote(self, symbol: str) -> dict:
        """
        Get the latest quote for a crypto symbol.

        Parameters
        ----------
        symbol : str
            Crypto symbol.

        Returns
        -------
        dict
            Latest quote data.

        """
        url = f"{self._data_url}/v1beta3/crypto/us/latest/quotes"
        params = {"symbols": symbol}
        return await self._request("GET", url, params=params)

    async def get_crypto_snapshot(self, symbol: str) -> dict:
        """
        Get a snapshot for a crypto symbol.

        Parameters
        ----------
        symbol : str
            Crypto symbol.

        Returns
        -------
        dict
            Snapshot data.

        """
        url = f"{self._data_url}/v1beta3/crypto/us/snapshots"
        params = {"symbols": symbol}
        return await self._request("GET", url, params=params)


class AlpacaApiError(Exception):
    """
    Exception raised for Alpaca API errors.

    Parameters
    ----------
    status_code : int
        HTTP status code.
    message : str
        Error message.
    response_data : dict, optional
        Full response data.

    """

    def __init__(
        self,
        status_code: int,
        message: str,
        response_data: dict | None = None,
    ) -> None:
        self.status_code = status_code
        self.message = message
        self.response_data = response_data
        super().__init__(f"Alpaca API Error ({status_code}): {message}")
