#!/usr/bin/env python3
# -------------------------------------------------------------------------------------------------
#  EODHD Data + Alpaca Execution Live Trading Example
#
#  This example demonstrates how to run a live EMA crossover strategy using:
#  - EODHD real-time WebSocket streaming for market data
#  - Alpaca for order execution (paper or live)
#
#  IMPORTANT: This is a more advanced setup that uses two different data providers.
#  For the simplest setup, use alpaca_ema_cross_aapl.py which uses only Alpaca.
#
#  Prerequisites:
#  - Set required environment variables (see .env.example)
#
#  Environment Variables:
#  - ALPACA_API_KEY: Your Alpaca API key
#  - ALPACA_API_SECRET: Your Alpaca API secret
#  - ALPACA_ENDPOINT: API endpoint (paper or live URL)
#  - ALPACA_PAPER: "true" for paper trading, "false" for live (default: true)
#  - EODHD_API_KEY: Your EODHD API key
#
#  Usage:
#  $ python eodhd_alpaca_ema_cross.py
# -------------------------------------------------------------------------------------------------

import asyncio
import os
from decimal import Decimal
from pathlib import Path

from dotenv import load_dotenv

# Load environment variables from .env file
load_dotenv(Path(__file__).parent / ".env")

from nautilus_trader.adapters.alpaca import ALPACA
from nautilus_trader.adapters.alpaca import AlpacaDataClientConfig
from nautilus_trader.adapters.alpaca import AlpacaExecClientConfig
from nautilus_trader.adapters.alpaca import AlpacaInstrumentProviderConfig
from nautilus_trader.adapters.alpaca import AlpacaLiveDataClientFactory
from nautilus_trader.adapters.alpaca import AlpacaLiveExecClientFactory
from nautilus_trader.adapters.alpaca.enums import AlpacaAssetClass
from nautilus_trader.adapters.alpaca.enums import AlpacaDataFeed
from nautilus_trader.adapters.eodhd import EODHD
from nautilus_trader.adapters.eodhd import EodhdDataClientConfig
from nautilus_trader.adapters.eodhd import EodhdLiveDataClientFactory
from nautilus_trader.common.enums import LogColor
from nautilus_trader.config import InstrumentProviderConfig
from nautilus_trader.config import LiveExecEngineConfig
from nautilus_trader.config import LoggingConfig
from nautilus_trader.config import RoutingConfig
from nautilus_trader.config import TradingNodeConfig
from nautilus_trader.indicators import ExponentialMovingAverage
from nautilus_trader.live.node import TradingNode
from nautilus_trader.model.data import QuoteTick
from nautilus_trader.model.data import TradeTick
from nautilus_trader.model.enums import OrderSide
from nautilus_trader.model.enums import TimeInForce
from nautilus_trader.model.identifiers import ClientId
from nautilus_trader.model.identifiers import InstrumentId
from nautilus_trader.model.identifiers import TraderId
from nautilus_trader.model.instruments import Instrument
from nautilus_trader.trading.config import StrategyConfig
from nautilus_trader.trading.strategy import Strategy


# *** THIS IS A TEST STRATEGY WITH NO ALPHA ADVANTAGE WHATSOEVER. ***
# *** IT IS NOT INTENDED TO BE USED TO TRADE LIVE WITH REAL MONEY. ***


class EodhdAlpacaEMACrossConfig(StrategyConfig, frozen=True):
    """
    Configuration for the EODHD + Alpaca EMA Cross strategy.

    This strategy uses EODHD for real-time data and Alpaca for execution.
    Since they use different venues, we need to map between them.
    """

    eodhd_instrument_id: InstrumentId  # e.g., AAPL.US (EODHD venue)
    alpaca_instrument_id: InstrumentId  # e.g., AAPL.ALPACA (Alpaca venue)
    eodhd_client_id: ClientId = ClientId(EODHD)
    trade_size: Decimal = Decimal(1)
    fast_ema_period: int = 5
    slow_ema_period: int = 10


class EodhdAlpacaEMACross(Strategy):
    """
    A dual-data-source EMA cross strategy.

    Uses EODHD for real-time quote data and Alpaca for order execution.
    """

    def __init__(self, config: EodhdAlpacaEMACrossConfig) -> None:
        super().__init__(config)

        self.eodhd_instrument: Instrument | None = None
        self.alpaca_instrument: Instrument | None = None
        self.eodhd_client_id = config.eodhd_client_id

        # EMA indicators
        self.fast_ema = ExponentialMovingAverage(config.fast_ema_period)
        self.slow_ema = ExponentialMovingAverage(config.slow_ema_period)

        # State tracking
        self._last_price: float | None = None
        self._position_side: str | None = None  # "LONG" or None

    def on_start(self) -> None:
        """Actions to be performed on strategy start."""
        # Get EODHD instrument (for data)
        self.eodhd_instrument = self.cache.instrument(self.config.eodhd_instrument_id)
        if self.eodhd_instrument is None:
            self.log.error(
                f"Could not find EODHD instrument for {self.config.eodhd_instrument_id}"
                f"\nPossible instruments: {self.cache.instrument_ids()}"
            )
            self.stop()
            return

        # Get Alpaca instrument (for execution)
        self.alpaca_instrument = self.cache.instrument(self.config.alpaca_instrument_id)
        if self.alpaca_instrument is None:
            self.log.error(
                f"Could not find Alpaca instrument for {self.config.alpaca_instrument_id}"
                f"\nPossible instruments: {self.cache.instrument_ids()}"
            )
            self.stop()
            return

        self.log.info(f"EODHD Instrument: {self.eodhd_instrument.id}")
        self.log.info(f"Alpaca Instrument: {self.alpaca_instrument.id}")

        # Subscribe to EODHD real-time data
        self.subscribe_quote_ticks(
            instrument_id=self.config.eodhd_instrument_id,
            client_id=self.eodhd_client_id,
        )
        self.subscribe_trade_ticks(
            instrument_id=self.config.eodhd_instrument_id,
            client_id=self.eodhd_client_id,
        )

        self.log.info("Strategy started - subscribed to EODHD data", LogColor.GREEN)

    def on_quote_tick(self, tick: QuoteTick) -> None:
        """Process incoming quote ticks from EODHD."""
        # Use mid-price for EMA calculation
        mid_price = (float(tick.bid_price) + float(tick.ask_price)) / 2
        self._update_ema_and_check_signal(mid_price)

    def on_trade_tick(self, tick: TradeTick) -> None:
        """Process incoming trade ticks from EODHD."""
        self._update_ema_and_check_signal(float(tick.price))

    def _update_ema_and_check_signal(self, price: float) -> None:
        """Update EMAs and check for trading signals."""
        self._last_price = price

        # Update EMAs
        self.fast_ema.update_raw(price)
        self.slow_ema.update_raw(price)

        # Need both EMAs to be initialized
        if not self.fast_ema.initialized or not self.slow_ema.initialized:
            self.log.debug(
                f"Warming up EMAs: fast={self.fast_ema.count}/{self.config.fast_ema_period}, "
                f"slow={self.slow_ema.count}/{self.config.slow_ema_period}"
            )
            return

        fast_value = self.fast_ema.value
        slow_value = self.slow_ema.value

        self.log.debug(
            f"Price: {price:.2f}, Fast EMA: {fast_value:.4f}, Slow EMA: {slow_value:.4f}",
        )

        # Check for crossover signals
        if fast_value > slow_value and self._position_side != "LONG":
            # Bullish crossover - go long
            self._buy()
        elif fast_value < slow_value and self._position_side == "LONG":
            # Bearish crossover - close position
            self._close_position()

    def _buy(self) -> None:
        """Execute a buy order via Alpaca."""
        if self.alpaca_instrument is None:
            return

        order = self.order_factory.market(
            instrument_id=self.alpaca_instrument.id,  # Use Alpaca instrument for execution
            order_side=OrderSide.BUY,
            quantity=self.alpaca_instrument.make_qty(self.config.trade_size),
            time_in_force=TimeInForce.DAY,
        )

        self.submit_order(order)
        self._position_side = "LONG"
        self.log.info(
            f"BUY signal: Submitted market order for {self.config.trade_size} shares",
            LogColor.GREEN,
        )

    def _close_position(self) -> None:
        """Close the current position via Alpaca."""
        if self.alpaca_instrument is None:
            return

        order = self.order_factory.market(
            instrument_id=self.alpaca_instrument.id,  # Use Alpaca instrument for execution
            order_side=OrderSide.SELL,
            quantity=self.alpaca_instrument.make_qty(self.config.trade_size),
            time_in_force=TimeInForce.DAY,
        )

        self.submit_order(order)
        self._position_side = None
        self.log.info(
            "SELL signal: Submitted market order to close position",
            LogColor.YELLOW,
        )

    def on_stop(self) -> None:
        """Actions to be performed when the strategy is stopped."""
        # Unsubscribe from data
        self.unsubscribe_quote_ticks(
            instrument_id=self.config.eodhd_instrument_id,
            client_id=self.eodhd_client_id,
        )
        self.unsubscribe_trade_ticks(
            instrument_id=self.config.eodhd_instrument_id,
            client_id=self.eodhd_client_id,
        )

        # Close any open position
        if self._position_side == "LONG":
            self.log.info("Closing position on strategy stop...")
            self._close_position()


def get_config_from_env():
    """
    Load configuration from environment variables.

    Returns
    -------
    dict
        Configuration dictionary with Alpaca and EODHD settings.
    """
    # Get Alpaca API credentials
    alpaca_api_key = os.environ.get("ALPACA_API_KEY")
    alpaca_api_secret = os.environ.get("ALPACA_API_SECRET")
    alpaca_endpoint = os.environ.get("ALPACA_ENDPOINT")

    # Determine if paper trading (default: True for safety)
    paper_env = os.environ.get("ALPACA_PAPER", "true").lower()
    paper = paper_env in ("true", "1", "yes")

    # Get EODHD API key
    eodhd_api_key = os.environ.get("EODHD_API_KEY")

    return {
        "alpaca_api_key": alpaca_api_key,
        "alpaca_api_secret": alpaca_api_secret,
        "alpaca_endpoint": alpaca_endpoint,
        "paper": paper,
        "eodhd_api_key": eodhd_api_key,
    }


async def main():
    """
    Run a live EMA cross strategy using EODHD data and Alpaca execution.
    """
    # -------------------------------------------------------------------------
    # Load configuration from environment
    # -------------------------------------------------------------------------

    config = get_config_from_env()

    SYMBOL = "AAPL"
    TRADING_MODE = "PAPER" if config["paper"] else "LIVE"

    # Validate API credentials
    if not config["alpaca_api_key"] or not config["alpaca_api_secret"]:
        print("ERROR: ALPACA_API_KEY and ALPACA_API_SECRET must be set")
        print("Please configure your .env file (see .env.example)")
        return

    if not config["eodhd_api_key"]:
        print("ERROR: EODHD_API_KEY must be set")
        print("Please configure your .env file (see .env.example)")
        return

    # Safety warning for live trading
    if not config["paper"]:
        print("=" * 60)
        print("⚠️  WARNING: LIVE TRADING MODE ENABLED ⚠️")
        print("=" * 60)
        print("This will execute REAL trades with REAL money!")
        print("Press Ctrl+C within 5 seconds to abort...")
        print("=" * 60)
        try:
            await asyncio.sleep(5)
        except KeyboardInterrupt:
            print("\nAborted.")
            return

    # -------------------------------------------------------------------------
    # Define instrument IDs for both providers
    # -------------------------------------------------------------------------

    eodhd_instrument_id = InstrumentId.from_str(f"{SYMBOL}.US")  # EODHD venue
    alpaca_instrument_id = InstrumentId.from_str(f"{SYMBOL}.ALPACA")  # Alpaca venue

    # EODHD instrument provider - load US equities
    eodhd_instrument_provider_config = InstrumentProviderConfig(
        load_ids=frozenset([str(eodhd_instrument_id)]),
    )

    # Alpaca instrument provider - load only AAPL
    alpaca_instrument_provider_config = AlpacaInstrumentProviderConfig(
        load_ids=frozenset([str(alpaca_instrument_id)]),
        asset_classes=frozenset([AlpacaAssetClass.US_EQUITY]),
    )

    # -------------------------------------------------------------------------
    # Configure the trading node with both data sources
    # -------------------------------------------------------------------------

    config_node = TradingNodeConfig(
        trader_id=TraderId(f"DUAL-{TRADING_MODE}-001"),
        logging=LoggingConfig(
            log_level="INFO",
            log_colors=True,
            use_pyo3=True,
        ),
        exec_engine=LiveExecEngineConfig(
            reconciliation=True,
            reconciliation_lookback_mins=60,
            filter_position_reports=False,
        ),
        data_clients={
            # EODHD for real-time WebSocket data
            EODHD: EodhdDataClientConfig(
                api_key=config["eodhd_api_key"],
                instrument_provider=eodhd_instrument_provider_config,
                subscribe_trade_ticks=True,
                subscribe_quote_ticks=True,
            ),
            # Alpaca also for data (needed for instrument resolution)
            ALPACA: AlpacaDataClientConfig(
                api_key=config["alpaca_api_key"],
                api_secret=config["alpaca_api_secret"],
                paper=config["paper"],
                base_url_http=config["alpaca_endpoint"],
                data_feed=AlpacaDataFeed.IEX,
                instrument_provider=alpaca_instrument_provider_config,
            ),
        },
        exec_clients={
            # Alpaca for execution
            ALPACA: AlpacaExecClientConfig(
                api_key=config["alpaca_api_key"],
                api_secret=config["alpaca_api_secret"],
                paper=config["paper"],
                base_url_http=config["alpaca_endpoint"],
                instrument_provider=alpaca_instrument_provider_config,
                routing=RoutingConfig(
                    default=True,
                ),
            ),
        },
        timeout_connection=30.0,
        timeout_reconciliation=10.0,
        timeout_portfolio=10.0,
        timeout_disconnection=10.0,
        timeout_post_stop=5.0,
    )

    # -------------------------------------------------------------------------
    # Instantiate the trading node
    # -------------------------------------------------------------------------

    node = TradingNode(config=config_node)

    # -------------------------------------------------------------------------
    # Configure the strategy
    # -------------------------------------------------------------------------

    strategy_config = EodhdAlpacaEMACrossConfig(
        eodhd_instrument_id=eodhd_instrument_id,
        alpaca_instrument_id=alpaca_instrument_id,
        trade_size=Decimal(1),  # 1 share
        fast_ema_period=5,
        slow_ema_period=10,
    )

    strategy = EodhdAlpacaEMACross(config=strategy_config)
    node.trader.add_strategy(strategy=strategy)

    # -------------------------------------------------------------------------
    # Register client factories and build the node
    # -------------------------------------------------------------------------

    node.add_data_client_factory(EODHD, EodhdLiveDataClientFactory)
    node.add_data_client_factory(ALPACA, AlpacaLiveDataClientFactory)
    node.add_exec_client_factory(ALPACA, AlpacaLiveExecClientFactory)
    node.build()

    # -------------------------------------------------------------------------
    # Run the trading node
    # -------------------------------------------------------------------------

    print("=" * 60)
    print(f"EODHD DATA + ALPACA {TRADING_MODE} - EMA CROSS STRATEGY")
    print("=" * 60)
    print(f"Symbol: {SYMBOL}")
    print(f"Mode: {TRADING_MODE}")
    print(f"EODHD Instrument: {eodhd_instrument_id}")
    print(f"Alpaca Instrument: {alpaca_instrument_id}")
    print(f"Alpaca Endpoint: {config['alpaca_endpoint'] or 'Default'}")
    print(f"Fast EMA Period: {strategy_config.fast_ema_period}")
    print(f"Slow EMA Period: {strategy_config.slow_ema_period}")
    print(f"Trade Size: {strategy_config.trade_size} shares")
    print("-" * 60)
    print("Data Source: EODHD WebSocket (real-time)")
    print(f"Execution: Alpaca {TRADING_MODE} Trading")
    print("-" * 60)
    print("Press Ctrl+C to stop the trading node...")
    print("-" * 60)

    try:
        await node.run_async()
    except KeyboardInterrupt:
        print("\nShutting down gracefully...")
    finally:
        await node.stop_async()
        await asyncio.sleep(1)
        node.dispose()
        print("Trading node disposed.")


if __name__ == "__main__":
    asyncio.run(main())
