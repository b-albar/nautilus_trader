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
Alpaca data loader for backtesting.

Provides functionality to load historical data from Alpaca API
for use in backtesting with Nautilus Trader.
"""

import asyncio
from datetime import datetime
from datetime import UTC
from decimal import Decimal
import os
from typing import Literal

from nautilus_trader.adapters.alpaca.constants import ALPACA_VENUE
from nautilus_trader.adapters.alpaca.enums import AlpacaBarTimeframe
from nautilus_trader.adapters.alpaca.http import AlpacaHttpClient
from nautilus_trader.model.data import Bar
from nautilus_trader.model.data import BarType
from nautilus_trader.model.data import QuoteTick
from nautilus_trader.model.data import TradeTick
from nautilus_trader.model.enums import BarAggregation
from nautilus_trader.model.enums import AggressorSide
from nautilus_trader.model.identifiers import InstrumentId
from nautilus_trader.model.identifiers import Symbol
from nautilus_trader.model.identifiers import TradeId
from nautilus_trader.model.identifiers import Venue
from nautilus_trader.model.objects import Price
from nautilus_trader.model.objects import Quantity


class AlpacaDataLoader:
    """
    Loads historical data from Alpaca API for backtesting.

    Provides synchronous methods to fetch historical bar, trade, and quote
    data from Alpaca's REST API, suitable for use with BacktestEngine.

    Supports:
    - Bar data (1m, 5m, 15m, 30m, 1h, 4h, 1d, 1w, 1mo)
    - Trade data (tick-level trades)
    - Quote data (NBBO quotes)

    Parameters
    ----------
    api_key : str, optional
        The Alpaca API key.
        If not provided, will use ALPACA_API_KEY environment variable.
    api_secret : str, optional
        The Alpaca API secret.
        If not provided, will use ALPACA_API_SECRET environment variable.
    is_paper : bool, default True
        Whether to use paper trading API.
    is_crypto : bool, default False
        Whether to use crypto data API.

    Examples
    --------
    >>> loader = AlpacaDataLoader(api_key="key", api_secret="secret")
    >>> bars = loader.load_bars("AAPL", start=datetime(2024, 1, 1))
    >>> len(bars)
    250

    """

    def __init__(
        self,
        api_key: str | None = None,
        api_secret: str | None = None,
        is_paper: bool = True,
        is_crypto: bool = False,
    ) -> None:
        self._api_key = api_key or os.environ.get("ALPACA_API_KEY", "")
        self._api_secret = api_secret or os.environ.get("ALPACA_API_SECRET", "")

        if not self._api_key or not self._api_secret:
            raise ValueError(
                "Alpaca API credentials required. Provide via parameters or "
                "ALPACA_API_KEY/ALPACA_API_SECRET environment variables."
            )

        self._http_client = AlpacaHttpClient(
            api_key=self._api_key,
            api_secret=self._api_secret,
            is_paper=is_paper,
        )
        self._is_crypto = is_crypto
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

    def _parse_iso_timestamp(self, timestamp: str) -> int:
        """Parse ISO format timestamp to nanoseconds."""
        timestamp = timestamp.replace("Z", "+00:00")
        dt = datetime.fromisoformat(timestamp)
        if dt.tzinfo is None:
            dt = dt.replace(tzinfo=UTC)
        return int(dt.timestamp() * 1_000_000_000)

    def _timeframe_to_aggregation(
        self, timeframe: AlpacaBarTimeframe
    ) -> tuple[int, BarAggregation]:
        """Convert Alpaca timeframe to Nautilus BarAggregation."""
        mapping = {
            AlpacaBarTimeframe.MINUTE_1: (1, BarAggregation.MINUTE),
            AlpacaBarTimeframe.MINUTE_5: (5, BarAggregation.MINUTE),
            AlpacaBarTimeframe.MINUTE_15: (15, BarAggregation.MINUTE),
            AlpacaBarTimeframe.MINUTE_30: (30, BarAggregation.MINUTE),
            AlpacaBarTimeframe.HOUR_1: (1, BarAggregation.HOUR),
            AlpacaBarTimeframe.HOUR_4: (4, BarAggregation.HOUR),
            AlpacaBarTimeframe.DAY_1: (1, BarAggregation.DAY),
            AlpacaBarTimeframe.WEEK_1: (1, BarAggregation.WEEK),
            AlpacaBarTimeframe.MONTH_1: (1, BarAggregation.MONTH),
        }
        return mapping.get(timeframe, (1, BarAggregation.DAY))

    def load_bars(
        self,
        symbol: str,
        start: datetime,
        end: datetime | None = None,
        timeframe: AlpacaBarTimeframe = AlpacaBarTimeframe.DAY_1,
        bar_type: BarType | None = None,
        price_precision: int = 2,
        size_precision: int = 0,
        limit: int | None = None,
    ) -> list[Bar]:
        """
        Load bar data from Alpaca API.

        Parameters
        ----------
        symbol : str
            The symbol (e.g., "AAPL", "MSFT", "BTC/USD").
        start : datetime
            The start datetime for the data range.
        end : datetime, optional
            The end datetime. Defaults to now.
        timeframe : AlpacaBarTimeframe, default DAY_1
            The bar timeframe.
        bar_type : BarType, optional
            The bar type to use. If not provided, one will be created.
        price_precision : int, default 2
            The price precision for the bars.
        size_precision : int, default 0
            The volume precision for the bars.
        limit : int, optional
            Maximum number of bars to return.

        Returns
        -------
        list[Bar]

        Examples
        --------
        >>> bars = loader.load_bars(
        ...     symbol="AAPL",
        ...     start=datetime(2024, 1, 1),
        ...     end=datetime(2024, 12, 31),
        ...     timeframe=AlpacaBarTimeframe.DAY_1,
        ... )

        """
        return self._run_async(
            self.load_bars_async(
                symbol=symbol,
                start=start,
                end=end,
                timeframe=timeframe,
                bar_type=bar_type,
                price_precision=price_precision,
                size_precision=size_precision,
                limit=limit,
            )
        )

    async def load_bars_async(
        self,
        symbol: str,
        start: datetime,
        end: datetime | None = None,
        timeframe: AlpacaBarTimeframe = AlpacaBarTimeframe.DAY_1,
        bar_type: BarType | None = None,
        price_precision: int = 2,
        size_precision: int = 0,
        limit: int | None = None,
    ) -> list[Bar]:
        """
        Async version of load_bars.

        Parameters
        ----------
        symbol : str
            The symbol.
        start : datetime
            The start datetime.
        end : datetime, optional
            The end datetime.
        timeframe : AlpacaBarTimeframe, default DAY_1
            The bar timeframe.
        bar_type : BarType, optional
            The bar type.
        price_precision : int, default 2
            Price precision.
        size_precision : int, default 0
            Volume precision.
        limit : int, optional
            Max bars to return.

        Returns
        -------
        list[Bar]

        """
        # Create bar type if not provided
        if bar_type is None:
            venue = Venue("ALPACA_CRYPTO") if self._is_crypto else ALPACA_VENUE
            instrument_id = InstrumentId(Symbol(symbol), venue)
            from nautilus_trader.model.data import BarSpecification

            step, agg = self._timeframe_to_aggregation(timeframe)
            bar_spec = BarSpecification(step=step, aggregation=agg, price_type=0)
            bar_type = BarType(instrument_id, bar_spec)

        # Fetch bars from Alpaca API
        if self._is_crypto:
            raw_data = await self._http_client.get_crypto_bars(
                symbol=symbol,
                timeframe=timeframe.value,
                start=start.isoformat(),
                end=end.isoformat() if end else None,
                limit=limit,
            )
        else:
            raw_data = await self._http_client.get_stock_bars(
                symbol=symbol,
                timeframe=timeframe.value,
                start=start.isoformat(),
                end=end.isoformat() if end else None,
                limit=limit,
            )

        # Parse bars
        bars: list[Bar] = []
        bar_list = raw_data.get("bars", raw_data) if isinstance(raw_data, dict) else raw_data

        if bar_list is None:
            return bars

        for row in bar_list:
            ts_str = row.get("t") or row.get("timestamp")
            ts_event = self._parse_iso_timestamp(ts_str)

            bar = Bar(
                bar_type=bar_type,
                open=Price(Decimal(str(row.get("o") or row.get("open"))), price_precision),
                high=Price(Decimal(str(row.get("h") or row.get("high"))), price_precision),
                low=Price(Decimal(str(row.get("l") or row.get("low"))), price_precision),
                close=Price(Decimal(str(row.get("c") or row.get("close"))), price_precision),
                volume=Quantity(
                    Decimal(str(row.get("v") or row.get("volume") or 0)), size_precision
                ),
                ts_event=ts_event,
                ts_init=ts_event,
            )
            bars.append(bar)

        return bars

    def load_trades(
        self,
        symbol: str,
        start: datetime,
        end: datetime | None = None,
        price_precision: int = 2,
        size_precision: int = 0,
        limit: int | None = None,
    ) -> list[TradeTick]:
        """
        Load trade tick data from Alpaca API.

        Parameters
        ----------
        symbol : str
            The symbol.
        start : datetime
            The start datetime.
        end : datetime, optional
            The end datetime.
        price_precision : int, default 2
            Price precision.
        size_precision : int, default 0
            Size precision.
        limit : int, optional
            Max trades to return.

        Returns
        -------
        list[TradeTick]

        """
        return self._run_async(
            self.load_trades_async(
                symbol=symbol,
                start=start,
                end=end,
                price_precision=price_precision,
                size_precision=size_precision,
                limit=limit,
            )
        )

    async def load_trades_async(
        self,
        symbol: str,
        start: datetime,
        end: datetime | None = None,
        price_precision: int = 2,
        size_precision: int = 0,
        limit: int | None = None,
    ) -> list[TradeTick]:
        """
        Async version of load_trades.

        Parameters
        ----------
        symbol : str
            The symbol.
        start : datetime
            The start datetime.
        end : datetime, optional
            The end datetime.
        price_precision : int, default 2
            Price precision.
        size_precision : int, default 0
            Size precision.
        limit : int, optional
            Max trades.

        Returns
        -------
        list[TradeTick]

        """
        venue = Venue("ALPACA_CRYPTO") if self._is_crypto else ALPACA_VENUE
        instrument_id = InstrumentId(Symbol(symbol), venue)

        if self._is_crypto:
            raw_data = await self._http_client.get_crypto_trades(
                symbol=symbol,
                start=start.isoformat(),
                end=end.isoformat() if end else None,
                limit=limit,
            )
        else:
            raw_data = await self._http_client.get_stock_trades(
                symbol=symbol,
                start=start.isoformat(),
                end=end.isoformat() if end else None,
                limit=limit,
            )

        trades: list[TradeTick] = []
        trade_list = raw_data.get("trades", raw_data) if isinstance(raw_data, dict) else raw_data

        if trade_list is None:
            return trades

        for row in trade_list:
            ts_str = row.get("t") or row.get("timestamp")
            ts_event = self._parse_iso_timestamp(ts_str)

            trade = TradeTick(
                instrument_id=instrument_id,
                price=Price(Decimal(str(row.get("p") or row.get("price"))), price_precision),
                size=Quantity(Decimal(str(row.get("s") or row.get("size") or 0)), size_precision),
                aggressor_side=AggressorSide.NO_AGGRESSOR,
                trade_id=TradeId(str(row.get("i") or row.get("id") or ts_event)),
                ts_event=ts_event,
                ts_init=ts_event,
            )
            trades.append(trade)

        return trades

    def load_quotes(
        self,
        symbol: str,
        start: datetime,
        end: datetime | None = None,
        price_precision: int = 2,
        size_precision: int = 0,
        limit: int | None = None,
    ) -> list[QuoteTick]:
        """
        Load quote tick data (NBBO) from Alpaca API.

        Parameters
        ----------
        symbol : str
            The symbol.
        start : datetime
            The start datetime.
        end : datetime, optional
            The end datetime.
        price_precision : int, default 2
            Price precision.
        size_precision : int, default 0
            Size precision.
        limit : int, optional
            Max quotes.

        Returns
        -------
        list[QuoteTick]

        """
        return self._run_async(
            self.load_quotes_async(
                symbol=symbol,
                start=start,
                end=end,
                price_precision=price_precision,
                size_precision=size_precision,
                limit=limit,
            )
        )

    async def load_quotes_async(
        self,
        symbol: str,
        start: datetime,
        end: datetime | None = None,
        price_precision: int = 2,
        size_precision: int = 0,
        limit: int | None = None,
    ) -> list[QuoteTick]:
        """
        Async version of load_quotes.

        Parameters
        ----------
        symbol : str
            The symbol.
        start : datetime
            The start datetime.
        end : datetime, optional
            The end datetime.
        price_precision : int, default 2
            Price precision.
        size_precision : int, default 0
            Size precision.
        limit : int, optional
            Max quotes.

        Returns
        -------
        list[QuoteTick]

        """
        venue = Venue("ALPACA_CRYPTO") if self._is_crypto else ALPACA_VENUE
        instrument_id = InstrumentId(Symbol(symbol), venue)

        if self._is_crypto:
            raw_data = await self._http_client.get_crypto_quotes(
                symbol=symbol,
                start=start.isoformat(),
                end=end.isoformat() if end else None,
                limit=limit,
            )
        else:
            raw_data = await self._http_client.get_stock_quotes(
                symbol=symbol,
                start=start.isoformat(),
                end=end.isoformat() if end else None,
                limit=limit,
            )

        quotes: list[QuoteTick] = []
        quote_list = raw_data.get("quotes", raw_data) if isinstance(raw_data, dict) else raw_data

        if quote_list is None:
            return quotes

        for row in quote_list:
            ts_str = row.get("t") or row.get("timestamp")
            ts_event = self._parse_iso_timestamp(ts_str)

            quote = QuoteTick(
                instrument_id=instrument_id,
                bid_price=Price(
                    Decimal(str(row.get("bp") or row.get("bid_price"))), price_precision
                ),
                ask_price=Price(
                    Decimal(str(row.get("ap") or row.get("ask_price"))), price_precision
                ),
                bid_size=Quantity(
                    Decimal(str(row.get("bs") or row.get("bid_size") or 0)), size_precision
                ),
                ask_size=Quantity(
                    Decimal(str(row.get("as") or row.get("ask_size") or 0)), size_precision
                ),
                ts_event=ts_event,
                ts_init=ts_event,
            )
            quotes.append(quote)

        return quotes

    async def load_multiple_bars_async(
        self,
        symbols: list[str],
        start: datetime,
        end: datetime | None = None,
        timeframe: AlpacaBarTimeframe = AlpacaBarTimeframe.DAY_1,
        price_precision: int = 2,
        size_precision: int = 0,
    ) -> dict[InstrumentId, list[Bar]]:
        """
        Load bars for multiple symbols concurrently.

        Parameters
        ----------
        symbols : list[str]
            List of symbols.
        start : datetime
            The start datetime.
        end : datetime, optional
            The end datetime.
        timeframe : AlpacaBarTimeframe, default DAY_1
            The bar timeframe.
        price_precision : int, default 2
            Price precision.
        size_precision : int, default 0
            Volume precision.

        Returns
        -------
        dict[InstrumentId, list[Bar]]

        """
        tasks = []
        for symbol in symbols:
            tasks.append(
                self.load_bars_async(
                    symbol=symbol,
                    start=start,
                    end=end,
                    timeframe=timeframe,
                    price_precision=price_precision,
                    size_precision=size_precision,
                )
            )

        results = await asyncio.gather(*tasks, return_exceptions=True)

        venue = Venue("ALPACA_CRYPTO") if self._is_crypto else ALPACA_VENUE
        output: dict[InstrumentId, list[Bar]] = {}

        for symbol, result in zip(symbols, results):
            instrument_id = InstrumentId(Symbol(symbol), venue)
            if isinstance(result, Exception):
                output[instrument_id] = []
            else:
                output[instrument_id] = result

        return output

    def close(self) -> None:
        """Close the HTTP client and cleanup resources."""
        if self._loop is not None and not self._loop.is_closed():
            self._loop.run_until_complete(self._http_client.close())
            self._loop.close()
