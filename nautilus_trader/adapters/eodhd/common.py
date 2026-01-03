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
EODHD adapter common utilities and helper functions.
"""

from decimal import Decimal

from nautilus_trader.adapters.eodhd.enums import EodhdAssetType
from nautilus_trader.model.data import QuoteTick
from nautilus_trader.model.data import TradeTick
from nautilus_trader.model.identifiers import InstrumentId
from nautilus_trader.model.identifiers import Symbol
from nautilus_trader.model.identifiers import TradeId
from nautilus_trader.model.identifiers import Venue
from nautilus_trader.model.objects import Price
from nautilus_trader.model.objects import Quantity


def get_eodhd_asset_type(instrument_id: InstrumentId) -> EodhdAssetType:
    """
    Determine the EODHD asset type from an instrument ID.

    Parameters
    ----------
    instrument_id : InstrumentId
        The instrument ID to analyze.

    Returns
    -------
    EodhdAssetType
        The asset type for WebSocket subscription.

    """
    venue = instrument_id.venue.value.upper()

    if venue == "CC":
        return EodhdAssetType.CRYPTO
    elif venue == "FOREX":
        return EodhdAssetType.FOREX
    else:
        return EodhdAssetType.US_EQUITY


def parse_eodhd_symbol(symbol: str, exchange: str) -> InstrumentId:
    """
    Parse an EODHD symbol string into a Nautilus InstrumentId.

    Parameters
    ----------
    symbol : str
        The EODHD symbol (e.g., "AAPL", "EURUSD", "BTC-USD").
    exchange : str
        The EODHD exchange code (e.g., "US", "FOREX", "CC").

    Returns
    -------
    InstrumentId
        The Nautilus instrument ID.

    """
    return InstrumentId(
        symbol=Symbol(symbol),
        venue=Venue(exchange.upper()),
    )


def to_eodhd_symbol(instrument_id: InstrumentId) -> str:
    """
    Convert a Nautilus InstrumentId to an EODHD symbol string.

    Parameters
    ----------
    instrument_id : InstrumentId
        The Nautilus instrument ID.

    Returns
    -------
    str
        The EODHD symbol string (e.g., "AAPL.US", "EURUSD.FOREX").

    """
    return f"{instrument_id.symbol.value}.{instrument_id.venue.value}"


def to_eodhd_ws_symbol(instrument_id: InstrumentId) -> str:
    """
    Convert a Nautilus InstrumentId to an EODHD WebSocket symbol.

    For WebSocket subscriptions, only the symbol is needed without the exchange suffix.

    Parameters
    ----------
    instrument_id : InstrumentId
        The Nautilus instrument ID.

    Returns
    -------
    str
        The EODHD WebSocket symbol string (e.g., "AAPL", "EURUSD", "BTC-USD").

    """
    return instrument_id.symbol.value


def parse_us_trade_msg(
    msg: dict,
    price_precision: int = 2,
    size_precision: int = 0,
) -> TradeTick:
    """
    Parse an EODHD US trade WebSocket message into a TradeTick.

    Message format:
    {
        "s": "AAPL",           // ticker
        "p": 227.31,           // last trade price
        "v": 100,              // trade size (shares)
        "c": 12,               // trade condition code
        "dp": false,           // dark pool
        "ms": "open",          // market status
        "t": 1725198451165     // epoch ms
    }

    Parameters
    ----------
    msg : dict
        The WebSocket message dictionary.
    price_precision : int, default 2
        The price precision for the instrument.
    size_precision : int, default 0
        The size precision for the instrument.

    Returns
    -------
    TradeTick
        The parsed trade tick.

    """
    instrument_id = parse_eodhd_symbol(msg["s"], "US")
    ts_event = msg["t"] * 1_000_000  # Convert ms to ns

    return TradeTick(
        instrument_id=instrument_id,
        price=Price(Decimal(str(msg["p"])), price_precision),
        size=Quantity(Decimal(str(msg["v"])), size_precision),
        aggressor_side=0,  # Unknown from EODHD
        trade_id=TradeId(str(ts_event)),  # Use timestamp as trade ID
        ts_event=ts_event,
        ts_init=ts_event,
    )


def parse_us_quote_msg(
    msg: dict,
    price_precision: int = 2,
    size_precision: int = 0,
) -> QuoteTick:
    """
    Parse an EODHD US quote WebSocket message into a QuoteTick.

    Message format:
    {
        "s": "AAPL",           // ticker
        "ap": 227.33,          // ask price
        "as": 200,             // ask size
        "bp": 227.30,          // bid price
        "bs": 100,             // bid size
        "t": 1725198451165     // epoch ms
    }

    Parameters
    ----------
    msg : dict
        The WebSocket message dictionary.
    price_precision : int, default 2
        The price precision for the instrument.
    size_precision : int, default 0
        The size precision for the instrument.

    Returns
    -------
    QuoteTick
        The parsed quote tick.

    """
    instrument_id = parse_eodhd_symbol(msg["s"], "US")
    ts_event = msg["t"] * 1_000_000  # Convert ms to ns

    return QuoteTick(
        instrument_id=instrument_id,
        bid_price=Price(Decimal(str(msg["bp"])), price_precision),
        ask_price=Price(Decimal(str(msg["ap"])), price_precision),
        bid_size=Quantity(Decimal(str(msg["bs"])), size_precision),
        ask_size=Quantity(Decimal(str(msg["as"])), size_precision),
        ts_event=ts_event,
        ts_init=ts_event,
    )


def parse_forex_msg(
    msg: dict,
    price_precision: int = 5,
    size_precision: int = 0,
) -> QuoteTick:
    """
    Parse an EODHD FOREX WebSocket message into a QuoteTick.

    Message format:
    {
        "s": "EURUSD",         // symbol
        "a": 1.08675,          // ask
        "b": 1.08665,          // bid
        "dc": 0.21,            // daily change, %
        "dd": 0.0023,          // daily difference
        "ppms": false,         // pre/post market status
        "t": 1725198451165     // epoch ms
    }

    Parameters
    ----------
    msg : dict
        The WebSocket message dictionary.
    price_precision : int, default 5
        The price precision for forex pairs.
    size_precision : int, default 0
        The size precision (not provided by EODHD).

    Returns
    -------
    QuoteTick
        The parsed quote tick.

    """
    instrument_id = parse_eodhd_symbol(msg["s"], "FOREX")
    ts_event = int(msg["t"]) * 1_000_000  # Convert ms to ns

    return QuoteTick(
        instrument_id=instrument_id,
        bid_price=Price(Decimal(str(msg["b"])), price_precision),
        ask_price=Price(Decimal(str(msg["a"])), price_precision),
        bid_size=Quantity(Decimal("1"), size_precision),  # Default size
        ask_size=Quantity(Decimal("1"), size_precision),  # Default size
        ts_event=ts_event,
        ts_init=ts_event,
    )


def parse_crypto_msg(
    msg: dict,
    price_precision: int = 2,
    size_precision: int = 8,
) -> TradeTick:
    """
    Parse an EODHD Crypto WebSocket message into a TradeTick.

    Message format:
    {
        "s": "ETH-USD",        // symbol
        "p": 2874.12,          // last price
        "q": 0.145,            // trade quantity
        "dc": -0.54,           // daily change, %
        "dd": -15.61,          // daily difference
        "t": 1725198451165     // epoch ms
    }

    Parameters
    ----------
    msg : dict
        The WebSocket message dictionary.
    price_precision : int, default 2
        The price precision for crypto.
    size_precision : int, default 8
        The size precision for crypto.

    Returns
    -------
    TradeTick
        The parsed trade tick.

    """
    instrument_id = parse_eodhd_symbol(msg["s"], "CC")
    ts_event = int(msg["t"]) * 1_000_000  # Convert ms to ns

    return TradeTick(
        instrument_id=instrument_id,
        price=Price(Decimal(str(msg["p"])), price_precision),
        size=Quantity(Decimal(str(msg["q"])), size_precision),
        aggressor_side=0,  # Unknown from EODHD
        trade_id=TradeId(str(ts_event)),
        ts_event=ts_event,
        ts_init=ts_event,
    )
