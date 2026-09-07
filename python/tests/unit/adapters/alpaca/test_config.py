# -------------------------------------------------------------------------------------------------
#  Copyright (C) 2015-2026 Nautech Systems Pty Ltd. All rights reserved.
#  https://nautechsystems.io
#
#  Licensed under the GNU Lesser General Public License Version 3.0 (the "License");
# -------------------------------------------------------------------------------------------------

"""Tests for the native Alpaca adapter configuration surface."""

import pytest

from nautilus_trader.adapters.alpaca import AlpacaDataClientConfig
from nautilus_trader.adapters.alpaca import AlpacaDataFeed
from nautilus_trader.adapters.alpaca import AlpacaExecutionClientConfig
from nautilus_trader.model import InstrumentId


def test_config_repr_is_informative_without_exposing_secrets() -> None:
    """Config representations expose useful settings but never connection secrets."""
    data = AlpacaDataClientConfig(
        api_key="visible-key",
        api_secret="visible-secret",
        base_url_data_http="https://token@data.example",
        proxy_url="https://proxy-secret",
        instrument_ids=[InstrumentId.from_str("AAPL.ALPACA")],
        feed=AlpacaDataFeed.SIP,
    )
    execution = AlpacaExecutionClientConfig(
        api_key="visible-key",
        api_secret="visible-secret",
        base_url_trading_http="https://token@trading.example",
        proxy_url="https://proxy-secret",
        extended_hours=True,
    )

    data_repr = repr(data)
    execution_repr = repr(execution)

    assert "feed=sip" in data_repr
    assert "instrument_count=1" in data_repr
    assert "environment=paper" in execution_repr
    assert "extended_hours=true" in execution_repr
    for secret in ("visible-key", "visible-secret", "token@", "proxy-secret"):
        assert secret not in data_repr
        assert secret not in execution_repr


def test_config_can_be_preflight_validated_without_connecting() -> None:
    """Python callers receive actionable validation errors before node construction."""
    valid = AlpacaDataClientConfig(
        instrument_ids=[InstrumentId.from_str("AAPL.ALPACA")],
    )
    invalid = AlpacaDataClientConfig(
        instrument_ids=[InstrumentId.from_str("AAPL.XNAS")],
    )

    assert valid.validate() is None
    with pytest.raises(ValueError, match=r"venue ALPACA.*AAPL.XNAS"):
        invalid.validate()
