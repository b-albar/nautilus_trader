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
Alpaca adapter enumerations.
"""

from __future__ import annotations

from enum import Enum
from enum import unique


@unique
class AlpacaAssetClass(str, Enum):
    """
    Alpaca asset class types.
    """

    US_EQUITY = "us_equity"
    CRYPTO = "crypto"
    US_OPTION = "us_option"

    @property
    def is_equity(self) -> bool:
        return self is AlpacaAssetClass.US_EQUITY

    @property
    def is_crypto(self) -> bool:
        return self is AlpacaAssetClass.CRYPTO

    @property
    def is_option(self) -> bool:
        return self is AlpacaAssetClass.US_OPTION


@unique
class AlpacaOrderSide(str, Enum):
    """
    Alpaca order side.
    """

    BUY = "buy"
    SELL = "sell"


@unique
class AlpacaOrderType(str, Enum):
    """
    Alpaca order types.
    """

    MARKET = "market"
    LIMIT = "limit"
    STOP = "stop"
    STOP_LIMIT = "stop_limit"
    TRAILING_STOP = "trailing_stop"


@unique
class AlpacaTimeInForce(str, Enum):
    """
    Alpaca time in force options.
    """

    DAY = "day"
    GTC = "gtc"
    OPG = "opg"  # At the opening
    CLS = "cls"  # At the close
    IOC = "ioc"  # Immediate or cancel
    FOK = "fok"  # Fill or kill


@unique
class AlpacaOrderStatus(str, Enum):
    """
    Alpaca order status.
    """

    NEW = "new"
    PARTIALLY_FILLED = "partially_filled"
    FILLED = "filled"
    DONE_FOR_DAY = "done_for_day"
    CANCELED = "canceled"
    EXPIRED = "expired"
    REPLACED = "replaced"
    PENDING_CANCEL = "pending_cancel"
    PENDING_REPLACE = "pending_replace"
    PENDING_REVIEW = "pending_review"
    ACCEPTED = "accepted"
    PENDING_NEW = "pending_new"
    ACCEPTED_FOR_BIDDING = "accepted_for_bidding"
    STOPPED = "stopped"
    REJECTED = "rejected"
    SUSPENDED = "suspended"
    CALCULATED = "calculated"
    HELD = "held"


@unique
class AlpacaPositionSide(str, Enum):
    """
    Alpaca position side.
    """

    LONG = "long"
    SHORT = "short"


@unique
class AlpacaAccountStatus(str, Enum):
    """
    Alpaca account status.
    """

    ONBOARDING = "ONBOARDING"
    SUBMISSION_FAILED = "SUBMISSION_FAILED"
    SUBMITTED = "SUBMITTED"
    ACCOUNT_UPDATED = "ACCOUNT_UPDATED"
    APPROVAL_PENDING = "APPROVAL_PENDING"
    ACTIVE = "ACTIVE"
    REJECTED = "REJECTED"


@unique
class AlpacaBarTimeframe(str, Enum):
    """
    Alpaca bar timeframes.
    """

    MINUTE_1 = "1Min"
    MINUTE_5 = "5Min"
    MINUTE_15 = "15Min"
    MINUTE_30 = "30Min"
    HOUR_1 = "1Hour"
    HOUR_4 = "4Hour"
    DAY_1 = "1Day"
    WEEK_1 = "1Week"
    MONTH_1 = "1Month"


@unique
class AlpacaDataFeed(str, Enum):
    """
    Alpaca data feed options.
    """

    IEX = "iex"
    SIP = "sip"


DEFAULT_ASSET_CLASSES = frozenset({AlpacaAssetClass.US_EQUITY, AlpacaAssetClass.CRYPTO})
