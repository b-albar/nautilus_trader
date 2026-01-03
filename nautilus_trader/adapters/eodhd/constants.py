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
EODHD adapter constants.
"""

from typing import Final

from nautilus_trader.model.identifiers import ClientId
from nautilus_trader.model.identifiers import Venue


# Venue and client identifiers
EODHD: Final[str] = "EODHD"
EODHD_CLIENT_ID: Final[ClientId] = ClientId(EODHD)

# Supported venues/exchanges
EODHD_VENUE_US: Final[Venue] = Venue("US")
EODHD_VENUE_FOREX: Final[Venue] = Venue("FOREX")
EODHD_VENUE_CRYPTO: Final[Venue] = Venue("CC")

# API endpoints
EODHD_BASE_URL_HTTP: Final[str] = "https://eodhd.com/api"
EODHD_BASE_URL_WS: Final[str] = "wss://ws.eodhistoricaldata.com/ws"

# WebSocket endpoints by asset type
WS_ENDPOINT_US_TRADES: Final[str] = "/us"
WS_ENDPOINT_US_QUOTES: Final[str] = "/us-quote"
WS_ENDPOINT_FOREX: Final[str] = "/forex"
WS_ENDPOINT_CRYPTO: Final[str] = "/crypto"

# API rate limits
MAX_SYMBOLS_PER_WS_CONNECTION: Final[int] = 50
