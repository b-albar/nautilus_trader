#!/usr/bin/env python3
# -------------------------------------------------------------------------------------------------
#  Alpaca Live Trading with EMA Cross Strategy
#
#  This example demonstrates how to run a live EMA crossover strategy using
#  Alpaca for execution with real-time market data. Supports both paper and
#  live trading modes.
#
#  Prerequisites:
#  - Set required environment variables (see .env.example)
#
#  Environment Variables:
#  - ALPACA_API_KEY: Your Alpaca API key
#  - ALPACA_API_SECRET: Your Alpaca API secret
#  - ALPACA_ENDPOINT: API endpoint (paper or live URL)
#  - ALPACA_PAPER: "true" for paper trading, "false" for live (default: true)
#  - ALPACA_DATA_FEED: "IEX" (free) or "SIP" (paid) (default: IEX)
#
#  Usage:
#  $ python alpaca_ema_cross_aapl.py
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
from nautilus_trader.config import LiveExecEngineConfig
from nautilus_trader.config import LoggingConfig
from nautilus_trader.config import RoutingConfig
from nautilus_trader.config import TradingNodeConfig
from nautilus_trader.examples.strategies.ema_cross_long_only import EMACrossLongOnly
from nautilus_trader.examples.strategies.ema_cross_long_only import EMACrossLongOnlyConfig
from nautilus_trader.live.node import TradingNode
from nautilus_trader.model.data import BarType
from nautilus_trader.model.identifiers import InstrumentId
from nautilus_trader.model.identifiers import TraderId


# *** THIS IS A TEST STRATEGY WITH NO ALPHA ADVANTAGE WHATSOEVER. ***
# *** IT IS NOT INTENDED TO BE USED TO TRADE LIVE WITH REAL MONEY. ***


def get_config_from_env():
    """
    Load Alpaca configuration from environment variables.

    Returns
    -------
    dict
        Configuration dictionary with Alpaca settings.
    """
    # Get API credentials
    api_key = os.environ.get("ALPACA_API_KEY")
    api_secret = os.environ.get("ALPACA_API_SECRET")

    # Get endpoint (optional - will use default based on paper flag if not set)
    endpoint = os.environ.get("ALPACA_ENDPOINT")

    # Determine if paper trading (default: True for safety)
    paper_env = os.environ.get("ALPACA_PAPER", "true").lower()
    paper = paper_env in ("true", "1", "yes")

    # Get data feed (IEX is free, SIP requires subscription)
    data_feed_env = os.environ.get("ALPACA_DATA_FEED", "IEX").upper()
    data_feed = AlpacaDataFeed.SIP if data_feed_env == "SIP" else AlpacaDataFeed.IEX

    return {
        "api_key": api_key,
        "api_secret": api_secret,
        "endpoint": endpoint,
        "paper": paper,
        "data_feed": data_feed,
    }


async def main():
    """
    Run a live EMA cross strategy on Alpaca.

    This example uses:
    - Alpaca for execution (paper or live trading based on config)
    - Alpaca data feed for real-time market data
    - EMACrossLongOnly strategy with 5/10 bar EMA crossover
    """
    # -------------------------------------------------------------------------
    # Load configuration from environment
    # -------------------------------------------------------------------------

    config = get_config_from_env()

    SYMBOL = "AAPL"
    TRADING_MODE = "PAPER" if config["paper"] else "LIVE"

    # Validate API credentials
    if not config["api_key"] or not config["api_secret"]:
        print("ERROR: ALPACA_API_KEY and ALPACA_API_SECRET must be set")
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
    # Configure instrument provider
    # -------------------------------------------------------------------------

    instrument_provider_config = AlpacaInstrumentProviderConfig(
        load_ids=frozenset([f"{SYMBOL}.ALPACA"]),
        asset_classes=frozenset([AlpacaAssetClass.US_EQUITY]),
    )

    # -------------------------------------------------------------------------
    # Configure the trading node
    # -------------------------------------------------------------------------

    config_node = TradingNodeConfig(
        trader_id=TraderId(f"TRADER-{TRADING_MODE}-001"),
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
            ALPACA: AlpacaDataClientConfig(
                api_key=config["api_key"],
                api_secret=config["api_secret"],
                paper=config["paper"],
                base_url_http=config["endpoint"],  # Custom endpoint if provided
                data_feed=config["data_feed"],
                instrument_provider=instrument_provider_config,
                use_extended_hours=False,
            ),
        },
        exec_clients={
            ALPACA: AlpacaExecClientConfig(
                api_key=config["api_key"],
                api_secret=config["api_secret"],
                paper=config["paper"],
                base_url_http=config["endpoint"],  # Custom endpoint if provided
                instrument_provider=instrument_provider_config,
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
    # Configure the EMA cross strategy
    # -------------------------------------------------------------------------

    instrument_id = InstrumentId.from_str(f"{SYMBOL}.ALPACA")
    bar_type = BarType.from_str(f"{instrument_id}-1-MINUTE-LAST-EXTERNAL")

    strategy_config = EMACrossLongOnlyConfig(
        instrument_id=instrument_id,
        bar_type=bar_type,
        trade_size=Decimal(1),  # Trade 1 share at a time
        fast_ema_period=5,
        slow_ema_period=10,
        request_historical_bars=True,
        close_positions_on_stop=True,
    )

    strategy = EMACrossLongOnly(config=strategy_config)
    node.trader.add_strategy(strategy=strategy)

    # -------------------------------------------------------------------------
    # Register client factories and build the node
    # -------------------------------------------------------------------------

    node.add_data_client_factory(ALPACA, AlpacaLiveDataClientFactory)
    node.add_exec_client_factory(ALPACA, AlpacaLiveExecClientFactory)
    node.build()

    # -------------------------------------------------------------------------
    # Run the trading node
    # -------------------------------------------------------------------------

    print("=" * 60)
    print(f"ALPACA {TRADING_MODE} TRADING - EMA CROSS STRATEGY")
    print("=" * 60)
    print(f"Symbol: {SYMBOL}")
    print(f"Mode: {TRADING_MODE}")
    print(f"Data Feed: {config['data_feed'].name}")
    print(f"Endpoint: {config['endpoint'] or 'Default'}")
    print(f"Bar Type: {bar_type}")
    print(f"Fast EMA Period: {strategy_config.fast_ema_period}")
    print(f"Slow EMA Period: {strategy_config.slow_ema_period}")
    print(f"Trade Size: {strategy_config.trade_size} shares")
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
