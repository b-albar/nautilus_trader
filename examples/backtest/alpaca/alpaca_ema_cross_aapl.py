#!/usr/bin/env python3
# -------------------------------------------------------------------------------------------------
#  Alpaca EMA Cross Backtest Example (Bar Data)
#
#  This example demonstrates how to run a simple EMA crossover backtest using
#  historical BAR data from Alpaca API for AAPL.
#
#  Prerequisites:
#  - Set ALPACA_API_KEY and ALPACA_API_SECRET environment variables
#  - Or pass api_key/api_secret directly to AlpacaDataLoader
#
#  Usage:
#  $ export ALPACA_API_KEY=your_api_key
#  $ export ALPACA_API_SECRET=your_api_secret
#  $ python alpaca_ema_cross_aapl.py
# -------------------------------------------------------------------------------------------------

import time
from datetime import datetime, timedelta
from decimal import Decimal
from pathlib import Path

import pandas as pd
from dotenv import load_dotenv

# Load environment variables from .env file
load_dotenv(Path(__file__).parent / ".env")

from nautilus_trader.adapters.alpaca import AlpacaDataLoader
from nautilus_trader.adapters.alpaca.constants import ALPACA_VENUE
from nautilus_trader.adapters.alpaca.enums import AlpacaBarTimeframe
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
from nautilus_trader.model.identifiers import TraderId
from nautilus_trader.model.objects import Money
from nautilus_trader.test_kit.providers import TestInstrumentProvider


if __name__ == "__main__":
    # -------------------------------------------------------------------------
    # Configure the backtest
    # -------------------------------------------------------------------------

    SYMBOL = "AAPL"

    # Define date range (last 90 days for enough EMA warmup data)
    end_time = datetime(2026, 1, 2, 16, 0, 0)  # Market close
    start_time = end_time - timedelta(days=90)

    print(f"Running backtest for {SYMBOL} on Alpaca")
    print(f"Date range: {start_time.date()} to {end_time.date()}")
    print("-" * 60)

    # -------------------------------------------------------------------------
    # Load historical data from Alpaca API
    # -------------------------------------------------------------------------

    print("Loading historical bar data from Alpaca API...")

    # Initialize the data loader (uses ALPACA_API_KEY/ALPACA_API_SECRET env variables)
    loader = AlpacaDataLoader(is_paper=True)

    # Load daily bars
    bars = loader.load_bars(
        symbol=SYMBOL,
        start=start_time,
        end=end_time,
        timeframe=AlpacaBarTimeframe.DAY_1,
        price_precision=2,
        size_precision=0,
    )

    print(f"Loaded {len(bars)} daily bars")

    if not bars:
        print("ERROR: No data loaded. Check your API keys.")
        exit(1)

    # Close the loader
    loader.close()

    # -------------------------------------------------------------------------
    # Configure backtest engine
    # -------------------------------------------------------------------------

    config = BacktestEngineConfig(
        trader_id=TraderId("BACKTESTER-001"),
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
    # Add data to the engine
    # -------------------------------------------------------------------------

    engine.add_data(bars)
    print(f"Added {len(bars)} bars to backtest engine")

    # -------------------------------------------------------------------------
    # Configure the EMA cross strategy
    # -------------------------------------------------------------------------

    bar_type = BarType.from_str(f"{AAPL.id}-1-DAY-LAST-EXTERNAL")

    strategy_config = EMACrossLongOnlyConfig(
        instrument_id=AAPL.id,
        bar_type=bar_type,
        trade_size=Decimal(10),
        fast_ema_period=5,
        slow_ema_period=10,
        request_historical_bars=False,
        close_positions_on_stop=True,
    )

    strategy = EMACrossLongOnly(config=strategy_config)
    engine.add_strategy(strategy=strategy)

    # -------------------------------------------------------------------------
    # Run the backtest
    # -------------------------------------------------------------------------

    print("\nStarting backtest...")
    print("-" * 60)

    start_time_run = time.time()
    engine.run()
    elapsed_time = time.time() - start_time_run

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
