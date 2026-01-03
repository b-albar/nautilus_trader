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
EODHD instrument provider.
"""

from decimal import Decimal

from nautilus_trader.adapters.eodhd.http_client import EodhdHttpClient
from nautilus_trader.common.providers import InstrumentProvider
from nautilus_trader.config import InstrumentProviderConfig
from nautilus_trader.core.correctness import PyCondition
from nautilus_trader.model.currencies import USD
from nautilus_trader.model.currencies import EUR
from nautilus_trader.model.currencies import GBP
from nautilus_trader.model.currencies import JPY
from nautilus_trader.model.currencies import AUD
from nautilus_trader.model.currencies import CAD
from nautilus_trader.model.currencies import CHF
from nautilus_trader.model.currencies import NZD
from nautilus_trader.model.objects import Currency
from nautilus_trader.model.identifiers import InstrumentId
from nautilus_trader.model.identifiers import Symbol
from nautilus_trader.model.identifiers import Venue
from nautilus_trader.model.instruments import CryptoPerpetual
from nautilus_trader.model.instruments import CurrencyPair
from nautilus_trader.model.instruments import Equity
from nautilus_trader.model.objects import Price
from nautilus_trader.model.objects import Quantity


# Common currency mapping
CURRENCY_MAP = {
    "USD": USD,
    "EUR": EUR,
    "GBP": GBP,
    "JPY": JPY,
    "AUD": AUD,
    "CAD": CAD,
    "CHF": CHF,
    "NZD": NZD,
}


class EodhdInstrumentProvider(InstrumentProvider):
    """
    Provides Nautilus instrument definitions from EODHD.

    Parameters
    ----------
    client : EodhdHttpClient
        The EODHD HTTP client.
    config : InstrumentProviderConfig, optional
        The instrument provider configuration.

    """

    def __init__(
        self,
        client: EodhdHttpClient,
        config: InstrumentProviderConfig | None = None,
    ) -> None:
        super().__init__(config=config)
        self._client = client
        self._log_warnings = config.log_warnings if config else True

    async def load_all_async(self, filters: dict | None = None) -> None:
        """
        Load all instruments for specified exchanges.

        Parameters
        ----------
        filters : dict, optional
            Must contain 'exchanges' key with list of exchange codes.
            Example: {'exchanges': ['US', 'FOREX', 'CC']}

        Raises
        ------
        ValueError
            If 'exchanges' filter is not provided.

        """
        if filters is None or not filters.get("exchanges"):
            raise ValueError(
                "`filters` with an 'exchanges' key must be provided to load instruments",
            )

        exchanges = filters["exchanges"]

        for exchange in exchanges:
            self._log.info(f"Loading instruments for exchange: {exchange}")

            try:
                symbols_data = await self._client.get_exchange_symbols(exchange)

                for symbol_info in symbols_data:
                    try:
                        instrument = self._parse_symbol_to_instrument(symbol_info, exchange)
                        if instrument:
                            self.add(instrument=instrument)
                    except Exception as e:
                        if self._log_warnings:
                            self._log.warning(
                                f"Failed to parse instrument {symbol_info.get('Code', 'unknown')}: {e}"
                            )

                self._log.info(f"Loaded {len(symbols_data)} instruments for {exchange}")

            except Exception as e:
                self._log.error(f"Failed to load instruments for {exchange}: {e}")

    async def load_ids_async(
        self,
        instrument_ids: list[InstrumentId],
        filters: dict | None = None,
    ) -> None:
        """
        Load specific instruments by their IDs.

        Parameters
        ----------
        instrument_ids : list[InstrumentId]
            The instrument IDs to load.
        filters : dict, optional
            Additional filters (not currently used).

        """
        if not instrument_ids:
            self._log.warning("No instrument IDs given for loading")
            return

        # Group instruments by exchange
        by_exchange: dict[str, list[str]] = {}
        for instrument_id in instrument_ids:
            exchange = instrument_id.venue.value
            symbol = instrument_id.symbol.value
            if exchange not in by_exchange:
                by_exchange[exchange] = []
            by_exchange[exchange].append(symbol)

        # Load each exchange's symbols
        for exchange, symbols in by_exchange.items():
            try:
                all_symbols = await self._client.get_exchange_symbols(exchange)

                # Filter to requested symbols
                for symbol_info in all_symbols:
                    if symbol_info.get("Code") in symbols:
                        try:
                            instrument = self._parse_symbol_to_instrument(symbol_info, exchange)
                            if instrument:
                                self.add(instrument=instrument)
                        except Exception as e:
                            if self._log_warnings:
                                self._log.warning(
                                    f"Failed to parse instrument {symbol_info.get('Code')}: {e}"
                                )

            except Exception as e:
                self._log.error(f"Failed to load instruments for {exchange}: {e}")

    async def load_async(
        self,
        instrument_id: InstrumentId,
        filters: dict | None = None,
    ) -> None:
        """
        Load a single instrument by its ID.

        Parameters
        ----------
        instrument_id : InstrumentId
            The instrument ID to load.
        filters : dict, optional
            Additional filters (not currently used).

        """
        PyCondition.not_none(instrument_id, "instrument_id")
        await self.load_ids_async([instrument_id], filters)

    def _parse_symbol_to_instrument(
        self,
        symbol_info: dict,
        exchange: str,
    ) -> Equity | CurrencyPair | CryptoPerpetual | None:
        """
        Parse EODHD symbol information into a Nautilus instrument.

        Parameters
        ----------
        symbol_info : dict
            The symbol information from EODHD.
        exchange : str
            The exchange code.

        Returns
        -------
        Instrument or None
            The parsed instrument, or None if parsing fails.

        """
        code = symbol_info.get("Code", "")
        name = symbol_info.get("Name", code)

        instrument_id = InstrumentId(
            symbol=Symbol(code),
            venue=Venue(exchange),
        )

        ts_now = self._clock.timestamp_ns() if hasattr(self, "_clock") else 0

        if exchange == "FOREX":
            return self._create_forex_instrument(instrument_id, code, name, ts_now)
        elif exchange == "CC":
            return self._create_crypto_instrument(instrument_id, code, name, symbol_info, ts_now)
        else:
            return self._create_equity_instrument(instrument_id, code, name, symbol_info, ts_now)

    def _create_equity_instrument(
        self,
        instrument_id: InstrumentId,
        code: str,
        name: str,
        symbol_info: dict,
        ts_now: int,
    ) -> Equity:
        """Create an Equity instrument."""
        return Equity(
            instrument_id=instrument_id,
            raw_symbol=Symbol(code),
            currency=USD,
            price_precision=2,
            price_increment=Price(Decimal("0.01"), 2),
            lot_size=Quantity(Decimal("1"), 0),
            isin=symbol_info.get("ISIN"),
            ts_event=ts_now,
            ts_init=ts_now,
        )

    def _create_forex_instrument(
        self,
        instrument_id: InstrumentId,
        code: str,
        name: str,
        ts_now: int,
    ) -> CurrencyPair | None:
        """Create a CurrencyPair instrument for FOREX."""
        # Parse currency pair (e.g., EURUSD -> EUR/USD)
        if len(code) != 6:
            return None

        base_code = code[:3]
        quote_code = code[3:]

        base_currency = CURRENCY_MAP.get(base_code)
        quote_currency = CURRENCY_MAP.get(quote_code)

        if not base_currency or not quote_currency:
            # Create custom currencies
            if not base_currency:
                base_currency = Currency(
                    code=base_code,
                    precision=2,
                    iso4217=0,
                    name=base_code,
                    currency_type=1,  # FIAT
                )
            if not quote_currency:
                quote_currency = Currency(
                    code=quote_code,
                    precision=2,
                    iso4217=0,
                    name=quote_code,
                    currency_type=1,  # FIAT
                )

        # Determine price precision based on quote currency
        price_precision = 5 if quote_code != "JPY" else 3

        return CurrencyPair(
            instrument_id=instrument_id,
            raw_symbol=Symbol(code),
            base_currency=base_currency,
            quote_currency=quote_currency,
            price_precision=price_precision,
            size_precision=0,
            price_increment=Price(Decimal(f"0.{'0' * (price_precision - 1)}1"), price_precision),
            size_increment=Quantity(Decimal("1"), 0),
            lot_size=Quantity(Decimal("1000"), 0),
            max_quantity=None,
            min_quantity=Quantity(Decimal("1"), 0),
            max_price=None,
            min_price=None,
            margin_init=Decimal("0.02"),
            margin_maint=Decimal("0.01"),
            maker_fee=Decimal("0"),
            taker_fee=Decimal("0"),
            ts_event=ts_now,
            ts_init=ts_now,
        )

    def _create_crypto_instrument(
        self,
        instrument_id: InstrumentId,
        code: str,
        name: str,
        symbol_info: dict,
        ts_now: int,
    ) -> CryptoPerpetual | None:
        """Create a CryptoPerpetual instrument for crypto."""
        # Parse crypto pair (e.g., BTC-USD -> BTC/USD)
        if "-" not in code:
            return None

        parts = code.split("-")
        if len(parts) != 2:
            return None

        base_code = parts[0]
        quote_code = parts[1]

        quote_currency = CURRENCY_MAP.get(quote_code)
        if not quote_currency:
            quote_currency = Currency(
                code=quote_code,
                precision=2,
                iso4217=0,
                name=quote_code,
                currency_type=1,
            )

        base_currency = Currency(
            code=base_code,
            precision=8,
            iso4217=0,
            name=base_code,
            currency_type=2,  # CRYPTO
        )

        return CryptoPerpetual(
            instrument_id=instrument_id,
            raw_symbol=Symbol(code),
            base_currency=base_currency,
            quote_currency=quote_currency,
            settlement_currency=quote_currency,
            is_inverse=False,
            price_precision=2,
            size_precision=8,
            price_increment=Price(Decimal("0.01"), 2),
            size_increment=Quantity(Decimal("0.00000001"), 8),
            lot_size=Quantity(Decimal("0.00000001"), 8),
            max_quantity=None,
            min_quantity=Quantity(Decimal("0.00000001"), 8),
            max_notional=None,
            min_notional=None,
            max_price=None,
            min_price=None,
            margin_init=Decimal("0.05"),
            margin_maint=Decimal("0.025"),
            maker_fee=Decimal("0.001"),
            taker_fee=Decimal("0.001"),
            ts_event=ts_now,
            ts_init=ts_now,
        )
