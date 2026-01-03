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
Alpaca brokerage integration adapter.

This subpackage provides an instrument provider, data and execution clients,
configurations, and constants for connecting to and interacting with Alpaca's API.

Alpaca is a commission-free trading platform offering:
- US Equity trading (stocks)
- Cryptocurrency trading
- Options trading
- Fractional shares
- Paper trading for testing

For convenience, the most commonly used symbols are re-exported at the
subpackage's top level, so downstream code can simply import from
``nautilus_trader.adapters.alpaca``.

Example
-------
>>> from nautilus_trader.adapters.alpaca import (
...     ALPACA_VENUE,
...     AlpacaDataClientConfig,
...     AlpacaExecClientConfig,
...     AlpacaLiveDataClientFactory,
...     AlpacaLiveExecClientFactory,
... )

"""

from nautilus_trader.adapters.alpaca.config import AlpacaDataClientConfig
from nautilus_trader.adapters.alpaca.config import AlpacaExecClientConfig
from nautilus_trader.adapters.alpaca.config import AlpacaInstrumentProviderConfig
from nautilus_trader.adapters.alpaca.constants import ALPACA
from nautilus_trader.adapters.alpaca.constants import ALPACA_CLIENT_ID
from nautilus_trader.adapters.alpaca.constants import ALPACA_VENUE
from nautilus_trader.adapters.alpaca.data import AlpacaDataClient
from nautilus_trader.adapters.alpaca.enums import AlpacaAssetClass
from nautilus_trader.adapters.alpaca.enums import AlpacaBarTimeframe
from nautilus_trader.adapters.alpaca.enums import AlpacaDataFeed
from nautilus_trader.adapters.alpaca.enums import AlpacaOrderSide
from nautilus_trader.adapters.alpaca.enums import AlpacaOrderStatus
from nautilus_trader.adapters.alpaca.enums import AlpacaOrderType
from nautilus_trader.adapters.alpaca.enums import AlpacaTimeInForce
from nautilus_trader.adapters.alpaca.execution import AlpacaExecutionClient
from nautilus_trader.adapters.alpaca.factories import AlpacaLiveDataClientFactory
from nautilus_trader.adapters.alpaca.factories import AlpacaLiveExecClientFactory
from nautilus_trader.adapters.alpaca.factories import create_alpaca_clients
from nautilus_trader.adapters.alpaca.http import AlpacaApiError
from nautilus_trader.adapters.alpaca.http import AlpacaHttpClient
from nautilus_trader.adapters.alpaca.loaders import AlpacaDataLoader
from nautilus_trader.adapters.alpaca.providers import AlpacaInstrumentProvider
from nautilus_trader.adapters.alpaca.websocket import AlpacaTradingWebSocketClient
from nautilus_trader.adapters.alpaca.websocket import AlpacaWebSocketClient


__all__ = [
    # Constants
    "ALPACA",
    "ALPACA_CLIENT_ID",
    "ALPACA_VENUE",
    # Enums
    "AlpacaAssetClass",
    "AlpacaBarTimeframe",
    "AlpacaDataFeed",
    "AlpacaOrderSide",
    "AlpacaOrderStatus",
    "AlpacaOrderType",
    "AlpacaTimeInForce",
    # Config
    "AlpacaDataClientConfig",
    "AlpacaExecClientConfig",
    "AlpacaInstrumentProviderConfig",
    # Providers
    "AlpacaInstrumentProvider",
    # Clients
    "AlpacaDataClient",
    "AlpacaDataLoader",
    "AlpacaExecutionClient",
    "AlpacaHttpClient",
    "AlpacaWebSocketClient",
    "AlpacaTradingWebSocketClient",
    # Factories
    "AlpacaLiveDataClientFactory",
    "AlpacaLiveExecClientFactory",
    "create_alpaca_clients",
    # Exceptions
    "AlpacaApiError",
]
