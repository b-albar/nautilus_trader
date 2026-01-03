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
EODHD adapter enumerations.
"""

from enum import Enum


class EodhdAssetType(Enum):
    """
    Represents an EODHD asset type for WebSocket connections.
    """

    US_EQUITY = "us"
    US_QUOTE = "us-quote"
    FOREX = "forex"
    CRYPTO = "crypto"


class EodhdBarPeriod(Enum):
    """
    Represents an EODHD bar period for historical data.
    """

    DAILY = "d"
    WEEKLY = "w"
    MONTHLY = "m"


class EodhdIntradayInterval(Enum):
    """
    Represents an EODHD intraday interval for historical data.
    """

    MINUTE_1 = "1m"
    MINUTE_5 = "5m"
    HOUR_1 = "1h"


class EodhdMarketStatus(Enum):
    """
    Represents US market status from WebSocket messages.
    """

    OPEN = "open"
    CLOSED = "closed"
    EXTENDED_HOURS = "extended hours"
