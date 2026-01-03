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
Alpaca parsing utilities for converting between Alpaca and nautilus types.
"""

from __future__ import annotations

from datetime import datetime
from decimal import Decimal
from typing import Any

from nautilus_trader.adapters.alpaca.constants import ALPACA_VENUE
from nautilus_trader.adapters.alpaca.enums import AlpacaOrderSide
from nautilus_trader.adapters.alpaca.enums import AlpacaOrderStatus
from nautilus_trader.adapters.alpaca.enums import AlpacaOrderType
from nautilus_trader.adapters.alpaca.enums import AlpacaTimeInForce
from nautilus_trader.core.datetime import dt_to_unix_nanos
from nautilus_trader.model.enums import OrderSide
from nautilus_trader.model.enums import OrderStatus
from nautilus_trader.model.enums import OrderType
from nautilus_trader.model.enums import TimeInForce
from nautilus_trader.model.identifiers import InstrumentId
from nautilus_trader.model.identifiers import Symbol


# -------------------------------------------------------------------------
# Alpaca -> Nautilus conversions
# -------------------------------------------------------------------------


def parse_alpaca_order_side(side: str) -> OrderSide:
    """
    Parse Alpaca order side string to nautilus OrderSide.

    Parameters
    ----------
    side : str
        The Alpaca order side ("buy" or "sell").

    Returns
    -------
    OrderSide
        The nautilus order side.

    """
    return OrderSide.BUY if side.lower() == "buy" else OrderSide.SELL


def parse_alpaca_order_status(status: str) -> OrderStatus:
    """
    Parse Alpaca order status string to nautilus OrderStatus.

    Parameters
    ----------
    status : str
        The Alpaca order status.

    Returns
    -------
    OrderStatus
        The nautilus order status.

    """
    status_map = {
        "new": OrderStatus.ACCEPTED,
        "accepted": OrderStatus.ACCEPTED,
        "pending_new": OrderStatus.SUBMITTED,
        "partially_filled": OrderStatus.PARTIALLY_FILLED,
        "filled": OrderStatus.FILLED,
        "canceled": OrderStatus.CANCELED,
        "expired": OrderStatus.EXPIRED,
        "rejected": OrderStatus.REJECTED,
        "pending_cancel": OrderStatus.PENDING_CANCEL,
        "pending_replace": OrderStatus.PENDING_UPDATE,
        "stopped": OrderStatus.CANCELED,
        "suspended": OrderStatus.CANCELED,
        "held": OrderStatus.ACCEPTED,
        "done_for_day": OrderStatus.CANCELED,
        "replaced": OrderStatus.ACCEPTED,
        "accepted_for_bidding": OrderStatus.ACCEPTED,
        "calculated": OrderStatus.ACCEPTED,
    }
    return status_map.get(status.lower(), OrderStatus.INITIALIZED)


def parse_alpaca_order_type(order_type: str) -> OrderType:
    """
    Parse Alpaca order type string to nautilus OrderType.

    Parameters
    ----------
    order_type : str
        The Alpaca order type.

    Returns
    -------
    OrderType
        The nautilus order type.

    """
    type_map = {
        "market": OrderType.MARKET,
        "limit": OrderType.LIMIT,
        "stop": OrderType.STOP_MARKET,
        "stop_limit": OrderType.STOP_LIMIT,
        "trailing_stop": OrderType.TRAILING_STOP_MARKET,
    }
    return type_map.get(order_type.lower(), OrderType.MARKET)


def parse_alpaca_time_in_force(tif: str) -> TimeInForce:
    """
    Parse Alpaca time in force string to nautilus TimeInForce.

    Parameters
    ----------
    tif : str
        The Alpaca time in force.

    Returns
    -------
    TimeInForce
        The nautilus time in force.

    """
    tif_map = {
        "day": TimeInForce.DAY,
        "gtc": TimeInForce.GTC,
        "ioc": TimeInForce.IOC,
        "fok": TimeInForce.FOK,
        "opg": TimeInForce.AT_THE_OPEN,
        "cls": TimeInForce.AT_THE_CLOSE,
    }
    return tif_map.get(tif.lower(), TimeInForce.DAY)


def parse_alpaca_timestamp(timestamp: str, fallback_ns: int = 0) -> int:
    """
    Parse Alpaca ISO timestamp string to nanoseconds since Unix epoch.

    Parameters
    ----------
    timestamp : str
        The ISO format timestamp string.
    fallback_ns : int, default 0
        Fallback value if parsing fails.

    Returns
    -------
    int
        Timestamp in nanoseconds since Unix epoch.

    """
    if not timestamp:
        return fallback_ns
    try:
        # Handle various ISO formats
        ts = timestamp.replace("Z", "+00:00")
        dt = datetime.fromisoformat(ts)
        return dt_to_unix_nanos(dt)
    except Exception:
        return fallback_ns


def parse_alpaca_symbol(symbol: str) -> InstrumentId:
    """
    Parse Alpaca symbol to nautilus InstrumentId.

    Parameters
    ----------
    symbol : str
        The Alpaca symbol.

    Returns
    -------
    InstrumentId
        The nautilus instrument ID.

    """
    return InstrumentId(
        symbol=Symbol(symbol),
        venue=ALPACA_VENUE,
    )


# -------------------------------------------------------------------------
# Nautilus -> Alpaca conversions
# -------------------------------------------------------------------------


def convert_order_side_to_alpaca(side: OrderSide) -> AlpacaOrderSide:
    """
    Convert nautilus OrderSide to Alpaca order side.

    Parameters
    ----------
    side : OrderSide
        The nautilus order side.

    Returns
    -------
    AlpacaOrderSide
        The Alpaca order side.

    Raises
    ------
    ValueError
        If the order side is not supported.

    """
    if side == OrderSide.BUY:
        return AlpacaOrderSide.BUY
    elif side == OrderSide.SELL:
        return AlpacaOrderSide.SELL
    else:
        raise ValueError(f"Unsupported order side: {side}")


def convert_order_type_to_alpaca(order_type: OrderType) -> AlpacaOrderType:
    """
    Convert nautilus OrderType to Alpaca order type.

    Parameters
    ----------
    order_type : OrderType
        The nautilus order type.

    Returns
    -------
    AlpacaOrderType
        The Alpaca order type.

    Raises
    ------
    ValueError
        If the order type is not supported.

    """
    type_map = {
        OrderType.MARKET: AlpacaOrderType.MARKET,
        OrderType.LIMIT: AlpacaOrderType.LIMIT,
        OrderType.STOP_MARKET: AlpacaOrderType.STOP,
        OrderType.STOP_LIMIT: AlpacaOrderType.STOP_LIMIT,
        OrderType.TRAILING_STOP_MARKET: AlpacaOrderType.TRAILING_STOP,
    }

    result = type_map.get(order_type)
    if result is None:
        raise ValueError(f"Unsupported order type: {order_type}")
    return result


def convert_time_in_force_to_alpaca(tif: TimeInForce) -> AlpacaTimeInForce:
    """
    Convert nautilus TimeInForce to Alpaca time in force.

    Parameters
    ----------
    tif : TimeInForce
        The nautilus time in force.

    Returns
    -------
    AlpacaTimeInForce
        The Alpaca time in force.

    """
    tif_map = {
        TimeInForce.DAY: AlpacaTimeInForce.DAY,
        TimeInForce.GTC: AlpacaTimeInForce.GTC,
        TimeInForce.IOC: AlpacaTimeInForce.IOC,
        TimeInForce.FOK: AlpacaTimeInForce.FOK,
        TimeInForce.AT_THE_OPEN: AlpacaTimeInForce.OPG,
        TimeInForce.AT_THE_CLOSE: AlpacaTimeInForce.CLS,
    }
    return tif_map.get(tif, AlpacaTimeInForce.DAY)


# -------------------------------------------------------------------------
# Data parsing utilities
# -------------------------------------------------------------------------


def parse_decimal(value: Any, default: Decimal = Decimal("0")) -> Decimal:
    """
    Parse a value to Decimal.

    Parameters
    ----------
    value : Any
        The value to parse.
    default : Decimal
        Default value if parsing fails.

    Returns
    -------
    Decimal
        The parsed decimal value.

    """
    if value is None:
        return default
    try:
        return Decimal(str(value))
    except Exception:
        return default


def parse_float(value: Any, default: float = 0.0) -> float:
    """
    Parse a value to float.

    Parameters
    ----------
    value : Any
        The value to parse.
    default : float
        Default value if parsing fails.

    Returns
    -------
    float
        The parsed float value.

    """
    if value is None:
        return default
    try:
        return float(value)
    except Exception:
        return default


def parse_int(value: Any, default: int = 0) -> int:
    """
    Parse a value to int.

    Parameters
    ----------
    value : Any
        The value to parse.
    default : int
        Default value if parsing fails.

    Returns
    -------
    int
        The parsed int value.

    """
    if value is None:
        return default
    try:
        return int(value)
    except Exception:
        return default
