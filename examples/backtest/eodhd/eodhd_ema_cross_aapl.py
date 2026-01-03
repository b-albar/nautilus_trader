#!/usr/bin/env python3
# -------------------------------------------------------------------------------------------------
#  EODHD EMA Cross Backtest Example
#
#  This example demonstrates how to run a simple EMA crossover backtest using
#  historical data from EODHD API for AAPL.US over the last month.
#
#  Prerequisites:
#  - Set EODHD_API_KEY environment variable with your API key
#  - Or pass api_key directly to EodhdDataLoader
#
#  Usage:
#  $ export EODHD_API_KEY=your_api_key
#  $ python eodhd_ema_cross_aapl.py
# -------------------------------------------------------------------------------------------------

import time
from datetime import date, timedelta
from decimal import Decimal
from pathlib import Path

import pandas as pd
from dotenv import load_dotenv

# Load environment variables from .env file
load_dotenv(Path(__file__).parent / ".env")

from nautilus_trader.adapters.eodhd import EodhdDataLoader
from nautilus_trader.backtest.config import BacktestEngineConfig
from nautilus_trader.backtest.engine import BacktestEngine
from nautilus_trader.config import LoggingConfig
from nautilus_trader.config import RiskEngineConfig
from nautilus_trader.examples.strategies.ema_cross_long_only import EMACrossLongOnly
from nautilus_trader.examples.strategies.ema_cross_long_only import EMACrossLongOnlyConfig
from nautilus_trader.model.currencies import USD
from nautilus_trader.model.data import BarType
from nautilus_trader.model.enums import AccountType
from nautilus_trader.model.enums import OmsType
from nautilus_trader.model.identifiers import InstrumentId
from nautilus_trader.model.identifiers import Symbol
from nautilus_trader.model.identifiers import TraderId
from nautilus_trader.model.identifiers import Venue
from nautilus_trader.model.objects import Money
from nautilus_trader.test_kit.providers import TestInstrumentProvider


if __name__ == "__main__":
    # -------------------------------------------------------------------------
    # Configure the backtest
    # -------------------------------------------------------------------------

    # Define symbol and venue
    SYMBOL = "AAPL"
    EXCHANGE = "US"

    # Define date range (last 90 days for enough EMA warmup data)
    end_date = date(2026, 1, 2)  # Today (based on current time)
    start_date = end_date - timedelta(days=90)  # 90 days for EMA warmup + signals

    print(f"Running backtest for {SYMBOL}.{EXCHANGE}")
    print(f"Date range: {start_date} to {end_date}")
    print("-" * 60)

    # -------------------------------------------------------------------------
    # Load historical data from EODHD API
    # -------------------------------------------------------------------------

    print("Loading historical data from EODHD API...")

    # Initialize the data loader (uses EODHD_API_KEY env variable by default)
    loader = EodhdDataLoader()

    # Load daily EOD bars for the last month
    bars = loader.load_eod_bars(
        symbol=SYMBOL,
        exchange=EXCHANGE,
        start_date=start_date,
        end_date=end_date,
        price_precision=2,
        size_precision=0,
    )

    print(f"Loaded {len(bars)} bars")

    if not bars:
        print("ERROR: No data loaded. Check your API key and symbol.")
        exit(1)

    # Close the loader
    loader.close()

    # -------------------------------------------------------------------------
    # Configure backtest engine
    # -------------------------------------------------------------------------

    config = BacktestEngineConfig(
        trader_id=TraderId("BACKTESTER-001"),
        logging=LoggingConfig(log_level="INFO"),
        risk_engine=RiskEngineConfig(bypass=True),  # Bypass for simple backtests
    )

    # Build the backtest engine
    engine = BacktestEngine(config=config)

    # -------------------------------------------------------------------------
    # Add venue and instrument
    # -------------------------------------------------------------------------

    # Create venue (US market)
    US_VENUE = Venue(EXCHANGE)

    engine.add_venue(
        venue=US_VENUE,
        oms_type=OmsType.NETTING,
        account_type=AccountType.CASH,
        base_currency=USD,
        starting_balances=[Money(100_000.0, USD)],  # $100k starting capital
    )

    # Create instrument for AAPL
    AAPL = TestInstrumentProvider.equity(symbol=SYMBOL, venue=EXCHANGE)
    engine.add_instrument(AAPL)

    # -------------------------------------------------------------------------
    # Add data to the engine
    # -------------------------------------------------------------------------

    engine.add_data(bars)

    print(f"Added {len(bars)} bars to backtest engine")

    # -------------------------------------------------------------------------
    # Configure the EMA cross strategy
    # -------------------------------------------------------------------------

    # Define bar type for the strategy
    bar_type = BarType.from_str(f"{AAPL.id}-1-DAY-LAST-EXTERNAL")

    strategy_config = EMACrossLongOnlyConfig(
        instrument_id=AAPL.id,
        bar_type=bar_type,
        trade_size=Decimal(10),  # Trade 10 shares at a time
        fast_ema_period=5,  # 5-day fast EMA (shorter for daily bars)
        slow_ema_period=10,  # 10-day slow EMA
        request_historical_bars=False,  # Data already loaded
        close_positions_on_stop=True,
    )

    # Instantiate and add the strategy
    strategy = EMACrossLongOnly(config=strategy_config)
    engine.add_strategy(strategy=strategy)

    # -------------------------------------------------------------------------
    # Run the backtest
    # -------------------------------------------------------------------------

    print("\nStarting backtest...")
    print("-" * 60)

    start_time = time.time()
    engine.run()
    elapsed_time = time.time() - start_time

    print("-" * 60)
    print(f"Backtest completed in {elapsed_time:.2f} seconds")

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
