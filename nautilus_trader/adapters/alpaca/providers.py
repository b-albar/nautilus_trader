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
Alpaca instrument provider for loading tradable instruments.
"""

from __future__ import annotations

from collections.abc import Iterable
from decimal import Decimal
from typing import TYPE_CHECKING
from typing import Any

from nautilus_trader.adapters.alpaca.constants import ALPACA_VENUE
from nautilus_trader.adapters.alpaca.enums import AlpacaAssetClass
from nautilus_trader.adapters.alpaca.enums import DEFAULT_ASSET_CLASSES
from nautilus_trader.adapters.alpaca.http import AlpacaHttpClient
from nautilus_trader.common.providers import InstrumentProvider
from nautilus_trader.config import InstrumentProviderConfig
from nautilus_trader.core.correctness import PyCondition
from nautilus_trader.model.currencies import USD
from nautilus_trader.model.currencies import USDT
from nautilus_trader.model.enums import AssetClass
from nautilus_trader.model.identifiers import InstrumentId
from nautilus_trader.model.identifiers import Symbol
from nautilus_trader.model.instruments import CryptoPerpetual
from nautilus_trader.model.instruments import CurrencyPair
from nautilus_trader.model.instruments import Equity
from nautilus_trader.model.instruments import Instrument
from nautilus_trader.model.objects import Currency
from nautilus_trader.model.objects import Money
from nautilus_trader.model.objects import Price
from nautilus_trader.model.objects import Quantity


class AlpacaInstrumentProvider(InstrumentProvider):
    """
    Provides instruments from the Alpaca API.

    Supports US equities, crypto, and options.

    Parameters
    ----------
    client : AlpacaHttpClient
        The Alpaca HTTP client.
    config : InstrumentProviderConfig, optional
        The instrument provider configuration.
    asset_classes : Iterable[AlpacaAssetClass], optional
        Asset classes to load. Default is US_EQUITY and CRYPTO.

    """

    def __init__(
        self,
        client: AlpacaHttpClient,
        config: InstrumentProviderConfig | None = None,
        *,
        asset_classes: Iterable[AlpacaAssetClass] | None = None,
    ) -> None:
        PyCondition.not_none(client, "client")
        super().__init__(config=config or InstrumentProviderConfig())

        self._client = client

        resolved_types = (
            DEFAULT_ASSET_CLASSES
            if asset_classes is None
            else frozenset(AlpacaAssetClass(ac) for ac in asset_classes)
        )
        if not resolved_types:
            raise ValueError("asset_classes must contain at least one entry")

        self._asset_classes = resolved_types
        self._loaded_instruments: dict[InstrumentId, Instrument] = {}

    # -------------------------------------------------------------------------
    # Public helpers
    # -------------------------------------------------------------------------

    @property
    def http_client(self) -> AlpacaHttpClient:
        """Return the HTTP client."""
        return self._client

    # -------------------------------------------------------------------------
    # InstrumentProvider interface
    # -------------------------------------------------------------------------

    async def load_all_async(self, filters: dict | None = None) -> None:
        """
        Load all instruments from Alpaca.

        Parameters
        ----------
        filters : dict, optional
            Filters to apply when loading instruments.

        """
        filters = filters or self._filters

        self._log.info("Loading Alpaca instruments...")

        instruments = await self._load_instruments()

        self._log.info("Applying filters")

        self._reset_caches()

        loaded, skipped = self._ingest_instruments(instruments, filters)

        if not loaded:
            self._log.warning("No Alpaca instruments matched the requested filters")

        if skipped:
            self._log.debug(f"Skipped {skipped} instruments after applying filters")

        self._log.info(f"Loaded {loaded} Alpaca instruments")

    async def _load_instruments(self) -> list[Instrument]:
        """Load instruments from the Alpaca API."""
        instruments = []

        for asset_class in self._asset_classes:
            try:
                assets = await self._client.get_assets(
                    status="active",
                    asset_class=asset_class,
                )

                for asset_data in assets:
                    try:
                        instrument = self._parse_instrument(asset_data, asset_class)
                        if instrument:
                            instruments.append(instrument)
                    except Exception as e:
                        self._log.warning(
                            f"Failed to parse instrument {asset_data.get('symbol')}: {e}"
                        )

            except Exception as e:
                self._log.error(f"Failed to load {asset_class.value} instruments: {e}")

        return instruments

    def _parse_instrument(
        self,
        asset_data: dict,
        asset_class: AlpacaAssetClass,
    ) -> Instrument | None:
        """
        Parse asset data into a nautilus Instrument.

        Parameters
        ----------
        asset_data : dict
            Raw asset data from Alpaca API.
        asset_class : AlpacaAssetClass
            The asset class.

        Returns
        -------
        Instrument | None
            The parsed instrument, or None if not supported.

        """
        symbol_str = asset_data.get("symbol")
        if not symbol_str:
            return None

        # Check if tradable
        if not asset_data.get("tradable", False):
            return None

        instrument_id = InstrumentId(
            symbol=Symbol(symbol_str),
            venue=ALPACA_VENUE,
        )

        if asset_class == AlpacaAssetClass.US_EQUITY:
            return self._parse_equity(instrument_id, asset_data)
        elif asset_class == AlpacaAssetClass.CRYPTO:
            return self._parse_crypto(instrument_id, asset_data)
        else:
            self._log.debug(f"Skipping unsupported asset class: {asset_class}")
            return None

    def _parse_equity(
        self,
        instrument_id: InstrumentId,
        asset_data: dict,
    ) -> Equity:
        """
        Parse equity asset data.

        Parameters
        ----------
        instrument_id : InstrumentId
            The instrument ID.
        asset_data : dict
            Raw asset data.

        Returns
        -------
        Equity
            The equity instrument.

        """
        # Determine if fractional trading is supported
        fractionable = asset_data.get("fractionable", False)
        easy_to_borrow = asset_data.get("easy_to_borrow", False)
        shortable = asset_data.get("shortable", False)
        marginable = asset_data.get("marginable", False)

        # Default price precision for US equities
        price_precision = 2
        price_increment = Price.from_str("0.01")

        # Size precision depends on fractional trading
        if fractionable:
            size_precision = 9  # Fractional shares
            size_increment = Quantity.from_str("0.000000001")
        else:
            size_precision = 0  # Whole shares only
            size_increment = Quantity.from_str("1")

        return Equity(
            instrument_id=instrument_id,
            raw_symbol=Symbol(asset_data.get("symbol")),
            currency=USD,
            price_precision=price_precision,
            price_increment=price_increment,
            lot_size=size_increment,
            isin=None,
            ts_event=0,
            ts_init=0,
            info={
                "asset_id": asset_data.get("id"),
                "exchange": asset_data.get("exchange"),
                "name": asset_data.get("name"),
                "status": asset_data.get("status"),
                "tradable": asset_data.get("tradable"),
                "fractionable": fractionable,
                "shortable": shortable,
                "easy_to_borrow": easy_to_borrow,
                "marginable": marginable,
                "maintenance_margin_requirement": asset_data.get("maintenance_margin_requirement"),
            },
        )

    def _parse_crypto(
        self,
        instrument_id: InstrumentId,
        asset_data: dict,
    ) -> CurrencyPair:
        """
        Parse crypto asset data.

        Parameters
        ----------
        instrument_id : InstrumentId
            The instrument ID.
        asset_data : dict
            Raw asset data.

        Returns
        -------
        CurrencyPair
            The crypto currency pair instrument.

        """
        symbol = asset_data.get("symbol", "")

        # Parse base and quote currencies (e.g., "BTC/USD" -> BTC, USD)
        if "/" in symbol:
            base_str, quote_str = symbol.split("/")
        else:
            # Fallback for symbols without slash
            base_str = symbol[:-3] if len(symbol) > 3 else symbol
            quote_str = symbol[-3:] if len(symbol) > 3 else "USD"

        # Get or create currencies
        base_currency = self._get_or_create_currency(base_str, precision=8)
        quote_currency = self._get_or_create_currency(quote_str, precision=2)

        # Crypto typically has higher precision
        price_precision = asset_data.get("price_precision", 8)
        size_precision = asset_data.get("size_precision", 8)

        min_order_size = asset_data.get("min_order_size", "0.00000001")
        min_trade_amount = asset_data.get("min_trade_amount", "1")

        return CurrencyPair(
            instrument_id=instrument_id,
            raw_symbol=Symbol(symbol),
            base_currency=base_currency,
            quote_currency=quote_currency,
            price_precision=price_precision,
            size_precision=size_precision,
            price_increment=Price(Decimal(10) ** -price_precision, price_precision),
            size_increment=Quantity(Decimal(10) ** -size_precision, size_precision),
            lot_size=None,
            max_quantity=None,
            min_quantity=Quantity.from_str(str(min_order_size)),
            max_notional=None,
            min_notional=Money.from_str(f"{min_trade_amount} {quote_str}"),
            max_price=None,
            min_price=None,
            margin_init=Decimal("0"),
            margin_maint=Decimal("0"),
            maker_fee=Decimal("0.002"),  # Default fee
            taker_fee=Decimal("0.003"),  # Default fee
            ts_event=0,
            ts_init=0,
            info={
                "asset_id": asset_data.get("id"),
                "status": asset_data.get("status"),
                "tradable": asset_data.get("tradable"),
            },
        )

    def _get_or_create_currency(self, code: str, precision: int = 8) -> Currency:
        """
        Get an existing currency or create a new one.

        Parameters
        ----------
        code : str
            Currency code.
        precision : int
            Currency precision.

        Returns
        -------
        Currency
            The currency object.

        """
        # Check common currencies first
        if code == "USD":
            return USD
        elif code == "USDT":
            return USDT

        # Check if already registered
        currency = self._currencies.get(code)
        if currency:
            return currency

        # Create new currency
        currency = Currency(
            code=code,
            precision=precision,
            iso4217=0,  # Not an ISO currency
            name=code,
            currency_type=2,  # Crypto
        )

        self._currencies[code] = currency
        return currency

    def _reset_caches(self) -> None:
        """Reset internal caches."""
        self._instruments.clear()
        self._currencies.clear()
        self._loaded_instruments.clear()

    def _ingest_instruments(
        self,
        instruments: Iterable[Instrument],
        filters: dict | None,
    ) -> tuple[int, int]:
        """
        Ingest instruments into the provider.

        Parameters
        ----------
        instruments : Iterable[Instrument]
            Instruments to ingest.
        filters : dict, optional
            Filters to apply.

        Returns
        -------
        tuple[int, int]
            Count of (loaded, skipped) instruments.

        """
        loaded = 0
        skipped = 0

        for instrument in instruments:
            if not self._accept_instrument(instrument, filters):
                skipped += 1
                continue

            self._loaded_instruments[instrument.id] = instrument
            self.add(instrument)
            loaded += 1

        return loaded, skipped

    def _accept_instrument(
        self,
        instrument: Instrument,
        filters: dict | None,
    ) -> bool:
        """
        Check if an instrument passes the filters.

        Parameters
        ----------
        instrument : Instrument
            Instrument to check.
        filters : dict, optional
            Filters to apply.

        Returns
        -------
        bool
            True if instrument passes filters.

        """
        if not filters:
            return True

        def _normalize(value: Any, *, to_lower: bool = False) -> set[str]:
            if value is None:
                return set()
            if isinstance(value, str):
                values: Iterable[str] = [value]
            else:
                values = value
            return {
                (item.lower() if to_lower else item.upper())
                for item in values
                if isinstance(item, str)
            }

        # Filter by asset type
        asset_types = _normalize(filters.get("asset_types") or filters.get("types"), to_lower=True)
        if asset_types:
            if isinstance(instrument, Equity) and "equity" not in asset_types:
                return False
            if isinstance(instrument, CurrencyPair) and "crypto" not in asset_types:
                return False

        # Filter by symbols
        symbol_value = instrument.id.symbol.value
        symbols = _normalize(filters.get("symbols"))
        if symbols and symbol_value.upper() not in symbols:
            return False

        # Filter by exchange (for equities)
        if isinstance(instrument, Equity):
            exchange = instrument.info.get("exchange", "")
            exchanges = _normalize(filters.get("exchanges"))
            if exchanges and exchange.upper() not in exchanges:
                return False

        return True

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
            List of instrument IDs to load.
        filters : dict, optional
            Additional filters.

        """
        PyCondition.not_none(instrument_ids, "instrument_ids")
        if not instrument_ids:
            self._log.debug("No instrument IDs provided; nothing to load")
            return

        for instrument_id in instrument_ids:
            PyCondition.equal(
                instrument_id.venue,
                ALPACA_VENUE,
                "instrument_id.venue",
                ALPACA_VENUE.value,
            )

        # Load all instruments and filter
        await self.load_all_async(filters)

        missing = [i for i in instrument_ids if i not in self._instruments]
        if missing:
            self._log.warning(
                f"Unable to load {len(missing)} Alpaca instruments: "
                f"{', '.join(i.value for i in missing)}"
            )

    async def load_async(
        self,
        instrument_id: InstrumentId,
        filters: dict | None = None,
    ) -> None:
        """
        Load a single instrument.

        Parameters
        ----------
        instrument_id : InstrumentId
            The instrument ID to load.
        filters : dict, optional
            Additional filters.

        """
        PyCondition.not_none(instrument_id, "instrument_id")
        await self.load_ids_async([instrument_id], filters)
