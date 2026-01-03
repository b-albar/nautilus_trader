#!/usr/bin/env python3
# -------------------------------------------------------------------------------------------------
#  Alpaca Quote Data Backtest Example
#
#  This example demonstrates how to run a backtest using historical QUOTE data
#  (NBBO - National Best Bid and Offer) from Alpaca API for AAPL.
#
#  Note: Quote data can be very large - this example uses a short time window.
#
#  Prerequisites:
#  - Set ALPACA_API_KEY and ALPACA_API_SECRET environment variables
#  - Or pass api_key/api_secret directly to AlpacaDataLoader
#
#  Usage:
#  $ export ALPACA_API_KEY=your_api_key
#  $ export ALPACA_API_SECRET=your_api_secret
#  $ python alpaca_quote_ema_cross.py
# -------------------------------------------------------------------------------------------------

import time
from datetime import datetime
from decimal import Decimal
from pathlib import Path

import pandas as pd
from dotenv import load_dotenv

# Load environment variables from .env file
load_dotenv(Path(__file__).parent / ".env")

from nautilus_trader.adapters.alpaca import AlpacaDataLoader
from nautilus_trader.adapters.alpaca.constants import ALPACA_VENUE
from nautilus_trader.backtest.config import BacktestEngineConfig
from nautilus_trader.backtest.engine import BacktestEngine
from nautilus_trader.common.enums import LogColor
from nautilus_trader.config import LoggingConfig
from nautilus_trader.config import RiskEngineConfig
from nautilus_trader.indicators import ExponentialMovingAverage
from nautilus_trader.model.currencies import USD
from nautilus_trader.model.data import QuoteTick
from nautilus_trader.model.enums import AccountType
from nautilus_trader.model.enums import OmsType
from nautilus_trader.model.enums import OrderSide
from nautilus_trader.model.enums import TimeInForce
from nautilus_trader.model.identifiers import InstrumentId
from nautilus_trader.model.identifiers import TraderId
from nautilus_trader.model.instruments import Instrument
from nautilus_trader.model.objects import Money
from nautilus_trader.test_kit.providers import TestInstrumentProvider
from nautilus_trader.trading.config import StrategyConfig
from nautilus_trader.trading.strategy import Strategy


# *** THIS IS A TEST STRATEGY WITH NO ALPHA ADVANTAGE WHATSOEVER. ***
# *** IT IS NOT INTENDED TO BE USED TO TRADE LIVE WITH REAL MONEY. ***


class QuoteEMACrossConfig(StrategyConfig, frozen=True):
    """
    Configuration for ``QuoteEMACross`` strategy.

    This strategy uses quote tick (NBBO) data to calculate EMAs and generate signals.

    Parameters
    ----------
    instrument_id : InstrumentId
        The instrument ID for the strategy.
    trade_size : Decimal, default 10
        The position size per trade.
    fast_ema_period : int, default 50
        The fast EMA period (number of quotes).
    slow_ema_period : int, default 200
        The slow EMA period (number of quotes).
    close_positions_on_stop : bool, default True
        Whether to close positions when strategy stops.

    """

    instrument_id: InstrumentId
    trade_size: Decimal = Decimal(10)
    fast_ema_period: int = 50
    slow_ema_period: int = 200
    close_positions_on_stop: bool = True


class QuoteEMACross(Strategy):
    """
    A quote-based EMA crossover strategy.

    This strategy:
    - Subscribes to quote tick data (NBBO)
    - Calculates EMAs on mid-price (bid + ask) / 2
    - Goes LONG when fast EMA crosses above slow EMA
    - Closes position when fast EMA crosses below slow EMA

    Note: Uses quote counts for EMA periods (not time-based).
    """

    def __init__(self, config: QuoteEMACrossConfig) -> None:
        super().__init__(config)

        self.instrument: Instrument | None = None

        # EMA indicators
        self.fast_ema = ExponentialMovingAverage(config.fast_ema_period)
        self.slow_ema = ExponentialMovingAverage(config.slow_ema_period)

        # State tracking
        self._position_side: str | None = None  # "LONG" or None
        self._quote_count = 0

    def on_start(self) -> None:
        """Actions to be performed on strategy start."""
        self.instrument = self.cache.instrument(self.config.instrument_id)
        if self.instrument is None:
            self.log.error(f"Could not find instrument {self.config.instrument_id}")
            self.stop()
            return

        self.log.info(f"Instrument: {self.instrument.id}")
        self.log.info(f"Fast EMA period: {self.config.fast_ema_period} quotes")
        self.log.info(f"Slow EMA period: {self.config.slow_ema_period} quotes")

        # Subscribe to quote ticks
        self.subscribe_quote_ticks(self.config.instrument_id)
        self.log.info("Subscribed to quote ticks (NBBO)", LogColor.GREEN)

    def on_quote_tick(self, tick: QuoteTick) -> None:
        """Process incoming quote ticks."""
        self._quote_count += 1

        # Calculate mid-price from bid/ask
        mid_price = (float(tick.bid_price) + float(tick.ask_price)) / 2

        # Update EMAs
        self.fast_ema.update_raw(mid_price)
        self.slow_ema.update_raw(mid_price)

        # Need both EMAs to be initialized
        if not self.fast_ema.initialized or not self.slow_ema.initialized:
            if self._quote_count % 100 == 0:
                self.log.debug(
                    f"Warming up EMAs: {self._quote_count} quotes processed, "
                    f"fast={self.fast_ema.count}/{self.config.fast_ema_period}, "
                    f"slow={self.slow_ema.count}/{self.config.slow_ema_period}"
                )
            return

        fast_value = self.fast_ema.value
        slow_value = self.slow_ema.value

        # Check for crossover signals
        if fast_value > slow_value and self._position_side != "LONG":
            self._buy()
        elif fast_value < slow_value and self._position_side == "LONG":
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
            f"BUY signal @ quote {self._quote_count}: fast_ema={self.fast_ema.value:.4f} > slow_ema={self.slow_ema.value:.4f}",
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
            f"SELL signal @ quote {self._quote_count}: fast_ema={self.fast_ema.value:.4f} < slow_ema={self.slow_ema.value:.4f}",
            LogColor.YELLOW,
        )

    def on_stop(self) -> None:
        """Actions to be performed when the strategy is stopped."""
        self.log.info(f"Strategy processed {self._quote_count} quotes")

        # Unsubscribe from data
        self.unsubscribe_quote_ticks(self.config.instrument_id)

        # Close any open position
        if self.config.close_positions_on_stop and self._position_side == "LONG":
            self.log.info("Closing position on strategy stop...")
            self._close_position()


if __name__ == "__main__":
    # -------------------------------------------------------------------------
    # Configure the backtest
    # -------------------------------------------------------------------------

    SYMBOL = "AAPL"

    # Define time range (using a 30-minute window for quote data as it's very large)
    end_time = datetime(2026, 1, 3, 10, 0, 0)  # 10:00 AM
    start_time = datetime(2026, 1, 3, 9, 30, 0)  # 9:30 AM (market open)

    print(f"Running quote-based backtest for {SYMBOL} on Alpaca")
    print(f"Time range: {start_time} to {end_time}")
    print("-" * 60)

    # -------------------------------------------------------------------------
    # Load historical quote data from Alpaca API
    # -------------------------------------------------------------------------

    print("Loading historical quote tick data from Alpaca API...")
    print("(This may take a moment for quote data...)")

    # Initialize the data loader
    loader = AlpacaDataLoader(is_paper=True)

    # Load quote ticks (NBBO)
    quotes = loader.load_quotes(
        symbol=SYMBOL,
        start=start_time,
        end=end_time,
        price_precision=2,
        size_precision=0,
    )

    print(f"Loaded {len(quotes)} quote ticks")

    if not quotes:
        print("ERROR: No quote data loaded. Check your API keys and ensure market was open.")
        exit(1)

    # Show sample of data
    print(f"\nFirst quote: {quotes[0]}")
    print(f"Last quote:  {quotes[-1]}")

    # Close the loader
    loader.close()

    # -------------------------------------------------------------------------
    # Configure backtest engine
    # -------------------------------------------------------------------------

    config = BacktestEngineConfig(
        trader_id=TraderId("QUOTE-BACKTESTER-001"),
        logging=LoggingConfig(log_level="INFO"),
        risk_engine=RiskEngineConfig(bypass=True),
    )

    engine = BacktestEngine(config=config)

    # -------------------------------------------------------------------------
    # Add venue and instrument
    # -------------------------------------------------------------------------

    engine.add_venue(
        venue=ALPACA_VENUE,
        oms_type=OmsType.NETTING,
        account_type=AccountType.CASH,
        base_currency=USD,
        starting_balances=[Money(100_000.0, USD)],
    )

    # Create instrument for AAPL
    AAPL = TestInstrumentProvider.equity(symbol=SYMBOL, venue="ALPACA")
    engine.add_instrument(AAPL)

    # -------------------------------------------------------------------------
    # Add quote data to the engine
    # -------------------------------------------------------------------------

    engine.add_data(quotes)
    print(f"Added {len(quotes)} quote ticks to backtest engine")

    # -------------------------------------------------------------------------
    # Configure the quote-based EMA cross strategy
    # -------------------------------------------------------------------------

    strategy_config = QuoteEMACrossConfig(
        instrument_id=AAPL.id,
        trade_size=Decimal(10),
        fast_ema_period=50,
        slow_ema_period=200,
        close_positions_on_stop=True,
    )

    strategy = QuoteEMACross(config=strategy_config)
    engine.add_strategy(strategy=strategy)

    # -------------------------------------------------------------------------
    # Run the backtest
    # -------------------------------------------------------------------------

    print("\nStarting quote-based backtest...")
    print("-" * 60)

    start_time_run = time.time()
    engine.run()
    elapsed_time = time.time() - start_time_run

    print("-" * 60)
    print(f"Backtest completed in {elapsed_time:.2f} seconds")
    print(
        f"Processed {len(quotes)} quotes ({len(quotes) / max(elapsed_time, 0.001):.0f} quotes/sec)"
    )

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
        print(engine.trader.generate_account_report(ALPACA_VENUE))

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
