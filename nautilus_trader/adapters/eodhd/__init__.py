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
EODHD market data integration adapter.

This subpackage provides a data client factory, instrument provider,
configurations, and utilities for connecting to and interacting with
the EODHD API for historical and real-time market data.

EODHD provides:
- End-of-day (EOD) historical data for stocks, ETFs, and indices
- Intraday historical data (1m, 5m, 1h intervals)
- Real-time WebSocket streaming for US equities, FOREX, and cryptocurrencies

For convenience, the most commonly used symbols are re-exported at the
subpackage's top level, so downstream code can simply import from
``nautilus_trader.adapters.eodhd``.

References
----------
EODHD API Documentation: https://eodhd.com/financial-apis/

"""

from nautilus_trader.adapters.eodhd.config import EodhdDataClientConfig
from nautilus_trader.adapters.eodhd.constants import EODHD
from nautilus_trader.adapters.eodhd.constants import EODHD_CLIENT_ID
from nautilus_trader.adapters.eodhd.constants import EODHD_VENUE_CRYPTO
from nautilus_trader.adapters.eodhd.constants import EODHD_VENUE_FOREX
from nautilus_trader.adapters.eodhd.constants import EODHD_VENUE_US
from nautilus_trader.adapters.eodhd.data import EodhdDataClient
from nautilus_trader.adapters.eodhd.enums import EodhdAssetType
from nautilus_trader.adapters.eodhd.enums import EodhdBarPeriod
from nautilus_trader.adapters.eodhd.enums import EodhdIntradayInterval
from nautilus_trader.adapters.eodhd.factories import EodhdLiveDataClientFactory
from nautilus_trader.adapters.eodhd.factories import get_eodhd_http_client
from nautilus_trader.adapters.eodhd.factories import get_eodhd_instrument_provider
from nautilus_trader.adapters.eodhd.http_client import EodhdHttpClient
from nautilus_trader.adapters.eodhd.loaders import EodhdDataLoader
from nautilus_trader.adapters.eodhd.providers import EodhdInstrumentProvider


__all__ = [
    "EODHD",
    "EODHD_CLIENT_ID",
    "EODHD_VENUE_CRYPTO",
    "EODHD_VENUE_FOREX",
    "EODHD_VENUE_US",
    "EodhdAssetType",
    "EodhdBarPeriod",
    "EodhdDataClient",
    "EodhdDataClientConfig",
    "EodhdDataLoader",
    "EodhdHttpClient",
    "EodhdInstrumentProvider",
    "EodhdIntradayInterval",
    "EodhdLiveDataClientFactory",
    "get_eodhd_http_client",
    "get_eodhd_instrument_provider",
]
