# -------------------------------------------------------------------------------------------------
#  Copyright (C) 2015-2026 Nautech Systems Pty Ltd. All rights reserved.
#  https://nautechsystems.io
#
#  Licensed under the GNU Lesser General Public License Version 3.0 (the "License");
# -------------------------------------------------------------------------------------------------
"""Native Alpaca market-data and trading integration."""

from nautilus_trader._fixup import fixup_module_names
from nautilus_trader._libnautilus.alpaca import *  # noqa: F403


__all__ = [
    "ALPACA",
    "ALPACA_CLIENT_ID",
    "ALPACA_VENUE",
    "AlpacaAccountActivityClient",
    "AlpacaBarAdjustment",
    "AlpacaDataClientConfig",
    "AlpacaDataClientFactory",
    "AlpacaDataEnvironment",
    "AlpacaDataFeed",
    "AlpacaEnvironment",
    "AlpacaExecutionClientConfig",
    "AlpacaExecutionClientFactory",
    "AlpacaHistoricalDataClient",
    "AlpacaPortfolioClient",
    "AlpacaReferenceDataClient",
]

fixup_module_names(globals(), __name__)
del fixup_module_names
