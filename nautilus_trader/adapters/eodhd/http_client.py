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
EODHD HTTP client for historical data and instrument information.
"""

import asyncio
from datetime import date
from decimal import Decimal
from typing import Any

import aiohttp

from nautilus_trader.adapters.eodhd.constants import EODHD_BASE_URL_HTTP
from nautilus_trader.adapters.eodhd.enums import EodhdBarPeriod
from nautilus_trader.adapters.eodhd.enums import EodhdIntradayInterval
from nautilus_trader.common.component import Logger
from nautilus_trader.model.data import Bar
from nautilus_trader.model.data import BarType
from nautilus_trader.model.enums import BarAggregation
from nautilus_trader.model.objects import Price
from nautilus_trader.model.objects import Quantity


class EodhdHttpClient:
    """
    HTTP client for EODHD REST API.

    Provides methods for fetching historical data, instrument information,
    and exchange data from EODHD.

    Parameters
    ----------
    api_key : str
        The EODHD API key.
    base_url : str, optional
        The base URL for the API. Defaults to EODHD_BASE_URL_HTTP.
    timeout_secs : int, default 60
        The timeout for HTTP requests in seconds.
    logger : Logger, optional
        The logger for the client.

    """

    def __init__(
        self,
        api_key: str,
        base_url: str | None = None,
        timeout_secs: int = 60,
        logger: Logger | None = None,
    ) -> None:
        self._api_key = api_key
        self._base_url = base_url or EODHD_BASE_URL_HTTP
        self._timeout = aiohttp.ClientTimeout(total=timeout_secs)
        self._log = logger
        self._session: aiohttp.ClientSession | None = None

    async def _get_session(self) -> aiohttp.ClientSession:
        """Get or create the aiohttp session."""
        if self._session is None or self._session.closed:
            self._session = aiohttp.ClientSession(timeout=self._timeout)
        return self._session

    async def close(self) -> None:
        """Close the HTTP session."""
        if self._session and not self._session.closed:
            await self._session.close()

    async def _request(
        self,
        endpoint: str,
        params: dict[str, Any] | None = None,
    ) -> Any:
        """
        Make an HTTP GET request to the EODHD API.

        Parameters
        ----------
        endpoint : str
            The API endpoint (e.g., "/eod/AAPL.US").
        params : dict, optional
            Additional query parameters.

        Returns
        -------
        Any
            The JSON response.

        Raises
        ------
        aiohttp.ClientError
            If the request fails.

        """
        session = await self._get_session()

        url = f"{self._base_url}{endpoint}"
        query_params = {"api_token": self._api_key, "fmt": "json"}
        if params:
            query_params.update(params)

        async with session.get(url, params=query_params) as response:
            response.raise_for_status()
            return await response.json()

    async def get_eod_data(
        self,
        symbol: str,
        exchange: str,
        start_date: date | None = None,
        end_date: date | None = None,
        period: EodhdBarPeriod = EodhdBarPeriod.DAILY,
    ) -> list[dict]:
        """
        Get end-of-day historical data for a symbol.

        Parameters
        ----------
        symbol : str
            The symbol (e.g., "AAPL").
        exchange : str
            The exchange code (e.g., "US").
        start_date : date, optional
            The start date for the data range.
        end_date : date, optional
            The end date for the data range.
        period : EodhdBarPeriod, default DAILY
            The bar period (daily, weekly, monthly).

        Returns
        -------
        list[dict]
            List of OHLCV data dictionaries.

        """
        params = {"period": period.value}

        if start_date:
            params["from"] = start_date.isoformat()
        if end_date:
            params["to"] = end_date.isoformat()

        endpoint = f"/eod/{symbol}.{exchange}"
        return await self._request(endpoint, params)

    async def get_intraday_data(
        self,
        symbol: str,
        exchange: str,
        start_timestamp: int | None = None,
        end_timestamp: int | None = None,
        interval: EodhdIntradayInterval = EodhdIntradayInterval.MINUTE_1,
    ) -> list[dict]:
        """
        Get intraday historical data for a symbol.

        Parameters
        ----------
        symbol : str
            The symbol (e.g., "AAPL").
        exchange : str
            The exchange code (e.g., "US").
        start_timestamp : int, optional
            The start Unix timestamp in seconds.
        end_timestamp : int, optional
            The end Unix timestamp in seconds.
        interval : EodhdIntradayInterval, default MINUTE_1
            The intraday interval (1m, 5m, 1h).

        Returns
        -------
        list[dict]
            List of intraday OHLCV data dictionaries.

        """
        params = {"interval": interval.value}

        if start_timestamp:
            params["from"] = str(start_timestamp)
        if end_timestamp:
            params["to"] = str(end_timestamp)

        endpoint = f"/intraday/{symbol}.{exchange}"
        return await self._request(endpoint, params)

    async def get_exchange_symbols(self, exchange: str) -> list[dict]:
        """
        Get list of all symbols for an exchange.

        Parameters
        ----------
        exchange : str
            The exchange code (e.g., "US", "FOREX", "CC").

        Returns
        -------
        list[dict]
            List of symbol information dictionaries.

        """
        endpoint = f"/exchange-symbol-list/{exchange}"
        return await self._request(endpoint)

    async def get_exchanges(self) -> list[dict]:
        """
        Get list of all available exchanges.

        Returns
        -------
        list[dict]
            List of exchange information dictionaries.

        """
        endpoint = "/exchanges-list"
        return await self._request(endpoint)

    def parse_eod_bars(
        self,
        data: list[dict],
        bar_type: BarType,
        price_precision: int = 2,
        size_precision: int = 0,
    ) -> list[Bar]:
        """
        Parse EOD data into Nautilus Bar objects.

        Parameters
        ----------
        data : list[dict]
            The raw EOD data from the API.
        bar_type : BarType
            The bar type for the data.
        price_precision : int, default 2
            The price precision.
        size_precision : int, default 0
            The volume precision.

        Returns
        -------
        list[Bar]
            List of Nautilus Bar objects.

        """
        from datetime import datetime

        bars = []
        for item in data:
            # Parse timestamp
            dt = datetime.fromisoformat(item["date"])
            ts_event = int(dt.timestamp() * 1_000_000_000)

            bar = Bar(
                bar_type=bar_type,
                open=Price(Decimal(str(item["open"])), price_precision),
                high=Price(Decimal(str(item["high"])), price_precision),
                low=Price(Decimal(str(item["low"])), price_precision),
                close=Price(Decimal(str(item["close"])), price_precision),
                volume=Quantity(Decimal(str(item.get("volume", 0))), size_precision),
                ts_event=ts_event,
                ts_init=ts_event,
            )
            bars.append(bar)

        return bars

    def parse_intraday_bars(
        self,
        data: list[dict],
        bar_type: BarType,
        price_precision: int = 2,
        size_precision: int = 0,
    ) -> list[Bar]:
        """
        Parse intraday data into Nautilus Bar objects.

        Parameters
        ----------
        data : list[dict]
            The raw intraday data from the API.
        bar_type : BarType
            The bar type for the data.
        price_precision : int, default 2
            The price precision.
        size_precision : int, default 0
            The volume precision.

        Returns
        -------
        list[Bar]
            List of Nautilus Bar objects.

        """
        bars = []
        for item in data:
            # Intraday data uses Unix timestamps
            ts_event = int(item["timestamp"]) * 1_000_000_000

            bar = Bar(
                bar_type=bar_type,
                open=Price(Decimal(str(item["open"])), price_precision),
                high=Price(Decimal(str(item["high"])), price_precision),
                low=Price(Decimal(str(item["low"])), price_precision),
                close=Price(Decimal(str(item["close"])), price_precision),
                volume=Quantity(Decimal(str(item.get("volume", 0))), size_precision),
                ts_event=ts_event,
                ts_init=ts_event,
            )
            bars.append(bar)

        return bars
