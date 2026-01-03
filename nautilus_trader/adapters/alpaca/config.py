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
Alpaca adapter configuration classes.
"""

from __future__ import annotations

from nautilus_trader.adapters.alpaca.constants import ALPACA_VENUE
from nautilus_trader.adapters.alpaca.enums import AlpacaAssetClass
from nautilus_trader.adapters.alpaca.enums import AlpacaDataFeed
from nautilus_trader.common.config import PositiveInt
from nautilus_trader.config import InstrumentProviderConfig
from nautilus_trader.config import LiveDataClientConfig
from nautilus_trader.config import LiveExecClientConfig
from nautilus_trader.model.identifiers import Venue


class AlpacaInstrumentProviderConfig(InstrumentProviderConfig, frozen=True):
    """
    Configuration for ``AlpacaInstrumentProvider`` instances.

    Parameters
    ----------
    load_all : bool, default False
        If all venue instruments should be loaded on start.
    load_ids : frozenset[InstrumentId], optional
        The list of instrument IDs to be loaded on start (if `load_all` is False).
    filters : frozendict or dict[str, Any], optional
        The venue specific instrument loading filters to apply.
    filter_callable : str, optional
        A fully qualified path to a callable that takes a single argument, `instrument`,
        and returns a bool indicating whether the instrument should be loaded.
    log_warnings : bool, default True
        If parser warnings should be logged.
    asset_classes : frozenset[AlpacaAssetClass], optional
        The asset classes to load. Default is US_EQUITY and CRYPTO.

    """

    def __eq__(self, other: object) -> bool:
        if other is None:
            return False
        if not isinstance(other, AlpacaInstrumentProviderConfig):
            return False
        return (
            self.load_all == other.load_all
            and self.load_ids == other.load_ids
            and self.filters == other.filters
            and self.asset_classes == other.asset_classes
        )

    def __hash__(self) -> int:
        filters = frozenset(self.filters.items()) if self.filters else None
        return hash((self.load_all, self.load_ids, filters, self.asset_classes))

    asset_classes: frozenset[AlpacaAssetClass] | None = None


class AlpacaDataClientConfig(LiveDataClientConfig, frozen=True):
    """
    Configuration for ``AlpacaDataClient`` instances.

    Parameters
    ----------
    venue : Venue, default ALPACA_VENUE
        The venue for the client.
    api_key : str, optional
        The Alpaca API public key.
        If ``None`` then will source the `ALPACA_API_KEY` or `ALPACA_PAPER_API_KEY`
        environment variable (depending on the `paper` setting).
    api_secret : str, optional
        The Alpaca API secret key.
        If ``None`` then will source the `ALPACA_API_SECRET` or `ALPACA_PAPER_API_SECRET`
        environment variable (depending on the `paper` setting).
    paper : bool, default True
        If the client is connecting to the Alpaca paper trading API.
    data_feed : AlpacaDataFeed, default IEX
        The data feed to use for market data. IEX is free, SIP requires subscription.
    base_url_http : str, optional
        The HTTP client custom endpoint override.
    base_url_ws : str, optional
        The WebSocket client custom endpoint override.
    base_url_data : str, optional
        The market data API custom endpoint override.
    http_timeout_secs : PositiveInt, default 10
        The timeout (seconds) for HTTP requests.
    update_instruments_interval_mins : PositiveInt | None, default 60
        The interval (minutes) between reloading instruments from the venue.
    use_extended_hours : bool, default False
        If extended hours data should be included for equities.

    """

    venue: Venue = ALPACA_VENUE
    api_key: str | None = None
    api_secret: str | None = None
    paper: bool = True
    data_feed: AlpacaDataFeed = AlpacaDataFeed.IEX
    base_url_http: str | None = None
    base_url_ws: str | None = None
    base_url_data: str | None = None
    http_timeout_secs: PositiveInt = 10
    update_instruments_interval_mins: PositiveInt | None = 60
    use_extended_hours: bool = False


class AlpacaExecClientConfig(LiveExecClientConfig, frozen=True):
    """
    Configuration for ``AlpacaExecutionClient`` instances.

    Parameters
    ----------
    venue : Venue, default ALPACA_VENUE
        The venue for the client.
    api_key : str, optional
        The Alpaca API public key.
        If ``None`` then will source the `ALPACA_API_KEY` or `ALPACA_PAPER_API_KEY`
        environment variable (depending on the `paper` setting).
    api_secret : str, optional
        The Alpaca API secret key.
        If ``None`` then will source the `ALPACA_API_SECRET` or `ALPACA_PAPER_API_SECRET`
        environment variable (depending on the `paper` setting).
    paper : bool, default True
        If the client is connecting to the Alpaca paper trading API.
    base_url_http : str, optional
        The HTTP client custom endpoint override.
    base_url_ws : str, optional
        The WebSocket client custom endpoint override.
    http_timeout_secs : PositiveInt, default 10
        The timeout (seconds) for HTTP requests.
    max_retries : PositiveInt | None, optional
        The maximum number of times a submit, cancel or modify order request will be retried.
    retry_delay_initial_ms : PositiveInt | None, optional
        The initial delay (milliseconds) between retries.
    retry_delay_max_ms : PositiveInt | None, optional
        The maximum delay (milliseconds) between retries.
    use_reduce_only : bool, default True
        If the `reduce_only` execution instruction on orders is sent through to the exchange.
    treat_day_as_gtc : bool, default False
        If DAY time in force should be treated as GTC for internal management.
        Alpaca cancels DAY orders at market close.
    fractional_qty_enabled : bool, default False
        If fractional quantity trading is enabled. Note: Only market orders support fractional.

    Warnings
    --------
    A short `retry_delay` with frequent retries may result in rate limiting.

    """

    venue: Venue = ALPACA_VENUE
    api_key: str | None = None
    api_secret: str | None = None
    paper: bool = True
    base_url_http: str | None = None
    base_url_ws: str | None = None
    http_timeout_secs: PositiveInt = 10
    max_retries: PositiveInt | None = None
    retry_delay_initial_ms: PositiveInt | None = None
    retry_delay_max_ms: PositiveInt | None = None
    use_reduce_only: bool = True
    treat_day_as_gtc: bool = False
    fractional_qty_enabled: bool = False
