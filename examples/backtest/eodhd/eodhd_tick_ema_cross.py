#!/usr/bin/env python3
# -------------------------------------------------------------------------------------------------
#  EODHD Tick Data EMA Cross Backtest Example
#
#  This example demonstrates how to run a simple EMA crossover backtest using
#  historical TICK data from EODHD API for AAPL.US.
#
#  Note: EODHD tick data is only available for US equities.
#
#  Prerequisites:
#  - Set EODHD_API_KEY environment variable with your API key
#  - Or pass api_key directly to EodhdDataLoader
#
#  Usage:
#  $ export EODHD_API_KEY=your_api_key
#  $ python eodhd_tick_ema_cross.py
# -------------------------------------------------------------------------------------------------

import time
from datetime import datetime, timedelta
from decimal import Decimal
from pathlib import Path

import pandas as pd
from dotenv import load_dotenv

# Load environment variables from .env file
load_dotenv(Path(__file__).parent / ".env")

from nautilus_trader.adapters.eodhd import EodhdDataLoader
from nautilus_trader.backtest.config import BacktestEngineConfig
from nautilus_trader.backtest.engine import BacktestEngine
from nautilus_trader.common.enums import LogColor
from nautilus_trader.config import LoggingConfig
from nautilus_trader.config import RiskEngineConfig
from nautilus_trader.indicators import ExponentialMovingAverage
from nautilus_trader.model.currencies import USD
from nautilus_trader.model.data import TradeTick
from nautilus_trader.model.enums import AccountType
from nautilus_trader.model.enums import OmsType
from nautilus_trader.model.enums import OrderSide
from nautilus_trader.model.enums import TimeInForce
from nautilus_trader.model.identifiers import InstrumentId
from nautilus_trader.model.identifiers import TraderId
from nautilus_trader.model.identifiers import Venue
from nautilus_trader.model.instruments import Instrument
from nautilus_trader.model.objects import Money
from nautilus_trader.test_kit.providers import TestInstrumentProvider
from nautilus_trader.trading.config import StrategyConfig
from nautilus_trader.trading.strategy import Strategy


# *** THIS IS A TEST STRATEGY WITH NO ALPHA ADVANTAGE WHATSOEVER. ***
# *** IT IS NOT INTENDED TO BE USED TO TRADE LIVE WITH REAL MONEY. ***


class TickEMACrossConfig(StrategyConfig, frozen=True):
    """
    Configuration for ``TickEMACross`` strategy.

    This strategy uses trade tick data to calculate EMAs and generate signals.

    Parameters
    ----------
    instrument_id : InstrumentId
        The instrument ID for the strategy.
    trade_size : Decimal, default 10
        The position size per trade.
    fast_ema_period : int, default 50
        The fast EMA period (number of ticks).
    slow_ema_period : int, default 200
        The slow EMA period (number of ticks).
    close_positions_on_stop : bool, default True
        Whether to close positions when strategy stops.

    """

    instrument_id: InstrumentId
    trade_size: Decimal = Decimal(10)
    fast_ema_period: int = 50
    slow_ema_period: int = 200
    close_positions_on_stop: bool = True


class TickEMACross(Strategy):
    """
    A tick-based EMA crossover strategy.

    This strategy:
    - Subscribes to trade tick data
    - Calculates EMAs on tick prices
    - Goes LONG when fast EMA crosses above slow EMA
    - Closes position when fast EMA crosses below slow EMA

    Note: Uses tick counts for EMA periods (not time-based).
    """

    def __init__(self, config: TickEMACrossConfig) -> None:
        super().__init__(config)

        self.instrument: Instrument | None = None

        # EMA indicators
        self.fast_ema = ExponentialMovingAverage(config.fast_ema_period)
        self.slow_ema = ExponentialMovingAverage(config.slow_ema_period)

        # State tracking
        self._position_side: str | None = None  # "LONG" or None
        self._tick_count = 0

    def on_start(self) -> None:
        """Actions to be performed on strategy start."""
        self.instrument = self.cache.instrument(self.config.instrument_id)
        if self.instrument is None:
            self.log.error(f"Could not find instrument {self.config.instrument_id}")
            self.stop()
            return

        self.log.info(f"Instrument: {self.instrument.id}")
        self.log.info(f"Fast EMA period: {self.config.fast_ema_period} ticks")
        self.log.info(f"Slow EMA period: {self.config.slow_ema_period} ticks")

        # Subscribe to trade ticks
        self.subscribe_trade_ticks(self.config.instrument_id)
        self.log.info("Subscribed to trade ticks", LogColor.GREEN)

    def on_trade_tick(self, tick: TradeTick) -> None:
        """Process incoming trade ticks."""
        self._tick_count += 1
        price = float(tick.price)

        # Update EMAs
        self.fast_ema.update_raw(price)
        self.slow_ema.update_raw(price)

        # Need both EMAs to be initialized
        if not self.fast_ema.initialized or not self.slow_ema.initialized:
            if self._tick_count % 100 == 0:  # Log every 100 ticks during warmup
                self.log.debug(
                    f"Warming up EMAs: {self._tick_count} ticks processed, "
                    f"fast={self.fast_ema.count}/{self.config.fast_ema_period}, "
                    f"slow={self.slow_ema.count}/{self.config.slow_ema_period}"
                )
            return

        fast_value = self.fast_ema.value
        slow_value = self.slow_ema.value

        # Check for crossover signals
        if fast_value > slow_value and self._position_side != "LONG":
            # Bullish crossover - go long
            self._buy()
        elif fast_value < slow_value and self._position_side == "LONG":
            # Bearish crossover - close position
            self._close_position()

    def _buy(self) -> None:
        """Execute a buy order."""
        if self.instrument is None:
            return

        order = self.order_factory.market(
            instrument_id=self.instrument.id,
            order_side=OrderSide.BUY,
            quantity=self.instrument.make_qty(self.config.trade_size),
            time_in_force=TimeInForce.GTC,
        )

        self.submit_order(order)
        self._position_side = "LONG"
        self.log.info(
            f"BUY signal @ tick {self._tick_count}: fast_ema={self.fast_ema.value:.4f} > slow_ema={self.slow_ema.value:.4f}",
            LogColor.GREEN,
        )

    def _close_position(self) -> None:
        """Close the current position."""
        if self.instrument is None:
            return

        order = self.order_factory.market(
            instrument_id=self.instrument.id,
            order_side=OrderSide.SELL,
            quantity=self.instrument.make_qty(self.config.trade_size),
            time_in_force=TimeInForce.GTC,
        )

        self.submit_order(order)
        self._position_side = None
        self.log.info(
            f"SELL signal @ tick {self._tick_count}: fast_ema={self.fast_ema.value:.4f} < slow_ema={self.slow_ema.value:.4f}",
            LogColor.YELLOW,
        )

    def on_stop(self) -> None:
        """Actions to be performed when the strategy is stopped."""
        self.log.info(f"Strategy processed {self._tick_count} ticks")

        # Unsubscribe from data
        self.unsubscribe_trade_ticks(self.config.instrument_id)

        # Close any open position
        if self.config.close_positions_on_stop and self._position_side == "LONG":
            self.log.info("Closing position on strategy stop...")
            self._close_position()


if __name__ == "__main__":
    # -------------------------------------------------------------------------
    # Configure the backtest
    # -------------------------------------------------------------------------

    SYMBOL = "AAPL"
    EXCHANGE = "US"

    # Define date range
    end_time = datetime(2024, 1, 2, 10, 30, 0)  # 10:30 AM
    start_time = datetime(2024, 1, 2, 9, 30, 0)  # 9:30 AM (market open)

    print(f"Running tick-based backtest for {SYMBOL}.{EXCHANGE}")
    print(f"Time range: {start_time} to {end_time}")
    print("-" * 60)

    # -------------------------------------------------------------------------
    # Load historical tick data from EODHD API
    # -------------------------------------------------------------------------

    print("Loading historical trade tick data from EODHD API...")
    print("(This may take a moment for tick data...)")

    # Initialize the data loader (uses EODHD_API_KEY env variable by default)
    loader = EodhdDataLoader()

    # Load trade ticks for a 1-hour window
    ticks = loader.load_ticks(
        symbol=SYMBOL,
        start_timestamp=start_time,
        end_timestamp=end_time,
        price_precision=2,
        size_precision=0,
    )

    print(f"Loaded {len(ticks)} trade ticks")

    if not ticks:
        print("ERROR: No tick data loaded. Check your API key and ensure market was open.")
        print("Note: EODHD tick data requires a paid subscription for most symbols.")
        print("      Demo API key only works for AAPL, TSLA, VTI, AMZN tickers.")
        exit(1)

    # Show sample of data
    print(f"\nFirst tick: {ticks[0]}")
    print(f"Last tick:  {ticks[-1]}")

    # Close the loader
    loader.close()

    # -------------------------------------------------------------------------
    # Configure backtest engine
    # -------------------------------------------------------------------------

    config = BacktestEngineConfig(
        trader_id=TraderId("TICK-BACKTESTER-001"),
        logging=LoggingConfig(log_level="INFO"),
        risk_engine=RiskEngineConfig(bypass=True),
    )

    engine = BacktestEngine(config=config)

    # -------------------------------------------------------------------------
    # Add venue and instrument
    # -------------------------------------------------------------------------

    US_VENUE = Venue(EXCHANGE)

    engine.add_venue(
        venue=US_VENUE,
        oms_type=OmsType.NETTING,
        account_type=AccountType.CASH,
        base_currency=USD,
        starting_balances=[Money(100_000.0, USD)],
    )

    # Create instrument for AAPL
    AAPL = TestInstrumentProvider.equity(symbol=SYMBOL, venue=EXCHANGE)
    engine.add_instrument(AAPL)

    # -------------------------------------------------------------------------
    # Add tick data to the engine
    # -------------------------------------------------------------------------

    engine.add_data(ticks)
    print(f"Added {len(ticks)} trade ticks to backtest engine")

    # -------------------------------------------------------------------------
    # Configure the tick-based EMA cross strategy
    # -------------------------------------------------------------------------

    strategy_config = TickEMACrossConfig(
        instrument_id=AAPL.id,
        trade_size=Decimal(10),  # Trade 10 shares at a time
        fast_ema_period=50,  # 50-tick fast EMA
        slow_ema_period=200,  # 200-tick slow EMA
        close_positions_on_stop=True,
    )

    strategy = TickEMACross(config=strategy_config)
    engine.add_strategy(strategy=strategy)

    # -------------------------------------------------------------------------
    # Run the backtest
    # -------------------------------------------------------------------------

    print("\nStarting tick-based backtest...")
    print("-" * 60)

    start_time_run = time.time()
    engine.run()
    elapsed_time = time.time() - start_time_run

    print("-" * 60)
    print(f"Backtest completed in {elapsed_time:.2f} seconds")
    print(f"Processed {len(ticks)} ticks ({len(ticks) / max(elapsed_time, 0.001):.0f} ticks/sec)")

    # -------------------------------------------------------------------------
    # Generate reports
    # -------------------------------------------------------------------------

    print("\n" + "=" * 60)
    print("BACKTEST RESULTS")
    print("=" * 60)

    with pd.option_context(
        "display.max_rows",
        100,
        "display.max_columns",
        None,
        "display.width",
        300,
    ):
        print("\n--- Account Report ---")
        print(engine.trader.generate_account_report(US_VENUE))

        print("\n--- Order Fills Report ---")
        print(engine.trader.generate_order_fills_report())

        print("\n--- Positions Report ---")
        print(engine.trader.generate_positions_report())

    # -------------------------------------------------------------------------
    # Cleanup
    # -------------------------------------------------------------------------

    engine.reset()
    engine.dispose()

    print("\nBacktest complete!")
