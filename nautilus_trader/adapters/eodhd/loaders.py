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
EODHD data loader for backtesting.

Provides functionality to load historical data from EODHD API
for use in backtesting with Nautilus Trader.
"""

import asyncio
from datetime import date
from datetime import datetime
import os

from nautilus_trader.adapters.eodhd.constants import EODHD_BASE_URL_HTTP
from nautilus_trader.adapters.eodhd.enums import EodhdBarPeriod
from nautilus_trader.adapters.eodhd.enums import EodhdIntradayInterval
from nautilus_trader.adapters.eodhd.http_client import EodhdHttpClient
from nautilus_trader.model.data import Bar
from nautilus_trader.model.data import BarType
from nautilus_trader.model.enums import BarAggregation
from nautilus_trader.model.enums import PriceType
from nautilus_trader.model.identifiers import InstrumentId
from nautilus_trader.model.identifiers import Symbol
from nautilus_trader.model.identifiers import Venue


class EodhdDataLoader:
    """
    Loads historical data from EODHD API for backtesting.

    Provides synchronous methods to fetch historical bar data from EODHD's
    REST API, suitable for use with BacktestEngine or data catalogs.

    Supports:
    - End-of-day (EOD) bar data (daily, weekly, monthly)
    - Intraday bar data (1m, 5m, 1h)

    Parameters
    ----------
    api_key : str, optional
        The EODHD API key.
        If not provided, will use EODHD_API_KEY environment variable.
    base_url : str, optional
        The base URL for the EODHD API.

    Examples
    --------
    >>> loader = EodhdDataLoader(api_key="your_api_key")
    >>> bars = loader.load_eod_bars("AAPL", "US", start_date=date(2024, 1, 1))
    >>> len(bars)
    250

    """

    def __init__(
        self,
        api_key: str | None = None,
        base_url: str | None = None,
    ) -> None:
        self._api_key = api_key or os.environ.get("EODHD_API_KEY", "")
        if not self._api_key:
            raise ValueError(
                "EODHD API key required. Provide via api_key parameter or EODHD_API_KEY environment variable."
            )
        self._http_client = EodhdHttpClient(
            api_key=self._api_key,
            base_url=base_url or EODHD_BASE_URL_HTTP,
        )
        self._loop: asyncio.AbstractEventLoop | None = None

    def _get_loop(self) -> asyncio.AbstractEventLoop:
        """Get or create event loop for sync operations."""
        try:
            return asyncio.get_running_loop()
        except RuntimeError:
            if self._loop is None or self._loop.is_closed():
                self._loop = asyncio.new_event_loop()
            return self._loop

    def _run_async(self, coro):
        """Run async coroutine synchronously."""
        loop = self._get_loop()
        return loop.run_until_complete(coro)

    def load_eod_bars(
        self,
        symbol: str,
        exchange: str,
        bar_type: BarType | None = None,
        start_date: date | None = None,
        end_date: date | None = None,
        period: EodhdBarPeriod = EodhdBarPeriod.DAILY,
        price_precision: int = 2,
        size_precision: int = 0,
    ) -> list[Bar]:
        """
        Load end-of-day bar data from EODHD API.

        Parameters
        ----------
        symbol : str
            The symbol (e.g., "AAPL", "MSFT").
        exchange : str
            The exchange code (e.g., "US", "LSE", "FOREX").
        bar_type : BarType, optional
            The bar type to use. If not provided, one will be created.
        start_date : date, optional
            The start date for the data range.
        end_date : date, optional
            The end date for the data range.
        period : EodhdBarPeriod, default DAILY
            The bar period (daily, weekly, monthly).
        price_precision : int, default 2
            The price precision for the bars.
        size_precision : int, default 0
            The volume precision for the bars.

        Returns
        -------
        list[Bar]

        Examples
        --------
        >>> bars = loader.load_eod_bars(
        ...     symbol="AAPL",
        ...     exchange="US",
        ...     start_date=date(2024, 1, 1),
        ...     end_date=date(2024, 12, 31),
        ... )

        """
        # Create bar type if not provided
        if bar_type is None:
            instrument_id = InstrumentId(Symbol(symbol), Venue(exchange))
            from nautilus_trader.model.data import BarSpecification

            bar_spec = BarSpecification(
                step=1,
                aggregation=BarAggregation.DAY,
                price_type=PriceType.LAST,
            )
            bar_type = BarType(instrument_id, bar_spec)

        raw_data = self._run_async(
            self._http_client.get_eod_data(
                symbol=symbol,
                exchange=exchange,
                start_date=start_date,
                end_date=end_date,
                period=period,
            )
        )

        return self._http_client.parse_eod_bars(
            raw_data,
            bar_type,
            price_precision=price_precision,
            size_precision=size_precision,
        )

    def load_intraday_bars(
        self,
        symbol: str,
        exchange: str,
        bar_type: BarType | None = None,
        start_timestamp: int | datetime | None = None,
        end_timestamp: int | datetime | None = None,
        interval: EodhdIntradayInterval = EodhdIntradayInterval.MINUTE_1,
        price_precision: int = 2,
        size_precision: int = 0,
    ) -> list[Bar]:
        """
        Load intraday bar data from EODHD API.

        Parameters
        ----------
        symbol : str
            The symbol (e.g., "AAPL", "MSFT").
        exchange : str
            The exchange code (e.g., "US").
        bar_type : BarType, optional
            The bar type to use. If not provided, one will be created.
        start_timestamp : int or datetime, optional
            The start timestamp (Unix seconds or datetime).
        end_timestamp : int or datetime, optional
            The end timestamp (Unix seconds or datetime).
        interval : EodhdIntradayInterval, default MINUTE_1
            The intraday interval (1m, 5m, 1h).
        price_precision : int, default 2
            The price precision for the bars.
        size_precision : int, default 0
            The volume precision for the bars.

        Returns
        -------
        list[Bar]

        Examples
        --------
        >>> from datetime import datetime
        >>> bars = loader.load_intraday_bars(
        ...     symbol="AAPL",
        ...     exchange="US",
        ...     start_timestamp=datetime(2024, 1, 2, 9, 30),
        ...     end_timestamp=datetime(2024, 1, 2, 16, 0),
        ...     interval=EodhdIntradayInterval.MINUTE_5,
        ... )

        """
        # Convert datetime to Unix timestamp if needed
        if isinstance(start_timestamp, datetime):
            start_timestamp = int(start_timestamp.timestamp())
        if isinstance(end_timestamp, datetime):
            end_timestamp = int(end_timestamp.timestamp())

        # Create bar type if not provided
        if bar_type is None:
            instrument_id = InstrumentId(Symbol(symbol), Venue(exchange))
            from nautilus_trader.model.data import BarSpecification

            if interval == EodhdIntradayInterval.MINUTE_1:
                step, agg = 1, BarAggregation.MINUTE
            elif interval == EodhdIntradayInterval.MINUTE_5:
                step, agg = 5, BarAggregation.MINUTE
            else:  # HOUR_1
                step, agg = 1, BarAggregation.HOUR

            bar_spec = BarSpecification(step=step, aggregation=agg, price_type=PriceType.LAST)
            bar_type = BarType(instrument_id, bar_spec)

        raw_data = self._run_async(
            self._http_client.get_intraday_data(
                symbol=symbol,
                exchange=exchange,
                start_timestamp=start_timestamp,
                end_timestamp=end_timestamp,
                interval=interval,
            )
        )

        return self._http_client.parse_intraday_bars(
            raw_data,
            bar_type,
            price_precision=price_precision,
            size_precision=size_precision,
        )

    async def load_eod_bars_async(
        self,
        symbol: str,
        exchange: str,
        bar_type: BarType | None = None,
        start_date: date | None = None,
        end_date: date | None = None,
        period: EodhdBarPeriod = EodhdBarPeriod.DAILY,
        price_precision: int = 2,
        size_precision: int = 0,
    ) -> list[Bar]:
        """
        Async version of load_eod_bars.

        Parameters
        ----------
        symbol : str
            The symbol (e.g., "AAPL", "MSFT").
        exchange : str
            The exchange code (e.g., "US", "LSE", "FOREX").
        bar_type : BarType, optional
            The bar type to use. If not provided, one will be created.
        start_date : date, optional
            The start date for the data range.
        end_date : date, optional
            The end date for the data range.
        period : EodhdBarPeriod, default DAILY
            The bar period (daily, weekly, monthly).
        price_precision : int, default 2
            The price precision for the bars.
        size_precision : int, default 0
            The volume precision for the bars.

        Returns
        -------
        list[Bar]

        """
        # Create bar type if not provided
        if bar_type is None:
            instrument_id = InstrumentId(Symbol(symbol), Venue(exchange))
            from nautilus_trader.model.data import BarSpecification

            bar_spec = BarSpecification(
                step=1,
                aggregation=BarAggregation.DAY,
                price_type=PriceType.LAST,
            )
            bar_type = BarType(instrument_id, bar_spec)

        raw_data = await self._http_client.get_eod_data(
            symbol=symbol,
            exchange=exchange,
            start_date=start_date,
            end_date=end_date,
            period=period,
        )

        return self._http_client.parse_eod_bars(
            raw_data,
            bar_type,
            price_precision=price_precision,
            size_precision=size_precision,
        )

    async def load_intraday_bars_async(
        self,
        symbol: str,
        exchange: str,
        bar_type: BarType | None = None,
        start_timestamp: int | datetime | None = None,
        end_timestamp: int | datetime | None = None,
        interval: EodhdIntradayInterval = EodhdIntradayInterval.MINUTE_1,
        price_precision: int = 2,
        size_precision: int = 0,
    ) -> list[Bar]:
        """
        Async version of load_intraday_bars.

        Parameters
        ----------
        symbol : str
            The symbol (e.g., "AAPL", "MSFT").
        exchange : str
            The exchange code (e.g., "US").
        bar_type : BarType, optional
            The bar type to use. If not provided, one will be created.
        start_timestamp : int or datetime, optional
            The start timestamp.
        end_timestamp : int or datetime, optional
            The end timestamp.
        interval : EodhdIntradayInterval, default MINUTE_1
            The intraday interval.
        price_precision : int, default 2
            The price precision for the bars.
        size_precision : int, default 0
            The volume precision for the bars.

        Returns
        -------
        list[Bar]

        """
        if isinstance(start_timestamp, datetime):
            start_timestamp = int(start_timestamp.timestamp())
        if isinstance(end_timestamp, datetime):
            end_timestamp = int(end_timestamp.timestamp())

        if bar_type is None:
            instrument_id = InstrumentId(Symbol(symbol), Venue(exchange))
            from nautilus_trader.model.data import BarSpecification

            if interval == EodhdIntradayInterval.MINUTE_1:
                step, agg = 1, BarAggregation.MINUTE
            elif interval == EodhdIntradayInterval.MINUTE_5:
                step, agg = 5, BarAggregation.MINUTE
            else:
                step, agg = 1, BarAggregation.HOUR

            bar_spec = BarSpecification(step=step, aggregation=agg, price_type=PriceType.LAST)
            bar_type = BarType(instrument_id, bar_spec)

        raw_data = await self._http_client.get_intraday_data(
            symbol=symbol,
            exchange=exchange,
            start_timestamp=start_timestamp,
            end_timestamp=end_timestamp,
            interval=interval,
        )

        return self._http_client.parse_intraday_bars(
            raw_data,
            bar_type,
            price_precision=price_precision,
            size_precision=size_precision,
        )

    async def load_multiple_eod_bars_async(
        self,
        symbols: list[tuple[str, str]],
        start_date: date | None = None,
        end_date: date | None = None,
        period: EodhdBarPeriod = EodhdBarPeriod.DAILY,
        price_precision: int = 2,
        size_precision: int = 0,
    ) -> dict[InstrumentId, list[Bar]]:
        """
        Load EOD bars for multiple symbols concurrently.

        Parameters
        ----------
        symbols : list[tuple[str, str]]
            List of (symbol, exchange) tuples.
        start_date : date, optional
            The start date for the data range.
        end_date : date, optional
            The end date for the data range.
        period : EodhdBarPeriod, default DAILY
            The bar period.
        price_precision : int, default 2
            The price precision.
        size_precision : int, default 0
            The volume precision.

        Returns
        -------
        dict[InstrumentId, list[Bar]]
            Dictionary mapping instrument IDs to their bars.

        Examples
        --------
        >>> bars = await loader.load_multiple_eod_bars_async(
        ...     symbols=[("AAPL", "US"), ("MSFT", "US"), ("GOOGL", "US")],
        ...     start_date=date(2024, 1, 1),
        ... )

        """
        tasks = []
        for symbol, exchange in symbols:
            tasks.append(
                self.load_eod_bars_async(
                    symbol=symbol,
                    exchange=exchange,
                    start_date=start_date,
                    end_date=end_date,
                    period=period,
                    price_precision=price_precision,
                    size_precision=size_precision,
                )
            )

        results = await asyncio.gather(*tasks, return_exceptions=True)

        output: dict[InstrumentId, list[Bar]] = {}
        for (symbol, exchange), result in zip(symbols, results):
            instrument_id = InstrumentId(Symbol(symbol), Venue(exchange))
            if isinstance(result, Exception):
                # Log or handle error
                output[instrument_id] = []
            else:
                output[instrument_id] = result

        return output

    def close(self) -> None:
        """Close the HTTP client and cleanup resources."""
        if self._loop is not None and not self._loop.is_closed():
            self._loop.run_until_complete(self._http_client.close())
            self._loop.close()
