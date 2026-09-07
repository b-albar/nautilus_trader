#!/usr/bin/env python3
# -------------------------------------------------------------------------------------------------
#  Copyright (C) 2015-2026 Nautech Systems Pty Ltd. All rights reserved.
#  https://nautechsystems.io
#
#  Licensed under the GNU Lesser General Public License Version 3.0 (the "License");
# -------------------------------------------------------------------------------------------------
"""Stream and request Alpaca equity data without placing orders."""

from nautilus_trader.adapters.alpaca import (
    ALPACA,
    AlpacaDataClientConfig,
    AlpacaDataClientFactory,
    AlpacaDataFeed,
)
from nautilus_trader.common import Environment
from nautilus_trader.live import LiveNode
from nautilus_trader.model import BarType, ClientId, InstrumentId, TraderId
from nautilus_trader.testkit import DataTesterConfig

TRADER_ID = TraderId.from_str("ALPACA-DATA-TESTER-001")
INSTRUMENT_ID = InstrumentId.from_str(f"AAPL.{ALPACA}")
BAR_TYPE = BarType.from_str(f"{INSTRUMENT_ID}-1-MINUTE-LAST-EXTERNAL")


def main() -> None:
    """Run live subscriptions and bounded historical requests."""
    node = (
        LiveNode.builder("ALPACA-DATA-TESTER-001", TRADER_ID, Environment.LIVE)
        .add_data_client(
            None,
            AlpacaDataClientFactory(),
            AlpacaDataClientConfig(
                instrument_ids=[INSTRUMENT_ID],
                feed=AlpacaDataFeed.IEX,
            ),
        )
        .build()
    )
    node.add_builtin_actor(
        "DataTester",
        DataTesterConfig(
            client_id=ClientId.from_str(ALPACA),
            instrument_ids=[INSTRUMENT_ID],
            bar_types=[BAR_TYPE],
            subscribe_quotes=True,
            subscribe_trades=True,
            subscribe_bars=True,
            request_instruments=True,
            request_quotes=True,
            request_trades=True,
            request_bars=True,
            log_data=True,
        ),
    )
    node.run()


if __name__ == "__main__":
    main()
