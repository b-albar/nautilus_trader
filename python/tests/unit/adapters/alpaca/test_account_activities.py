# -------------------------------------------------------------------------------------------------
#  Copyright (C) 2015-2026 Nautech Systems Pty Ltd. All rights reserved.
#  https://nautechsystems.io
#
#  Licensed under the GNU Lesser General Public License Version 3.0 (the "License");
# -------------------------------------------------------------------------------------------------

"""Tests for the native Alpaca account-activity Python client."""

import json
import threading
from collections.abc import Generator
from http.server import BaseHTTPRequestHandler
from http.server import ThreadingHTTPServer
from typing import ClassVar
from urllib.parse import parse_qs
from urllib.parse import urlparse

import pytest

from nautilus_trader.adapters.alpaca import AlpacaAccountActivityClient


class ActivityHandler(BaseHTTPRequestHandler):
    """Serve deterministic cursor-paginated Alpaca activity responses."""

    requests: ClassVar[list[dict[str, list[str]]]] = []

    def do_GET(self) -> None:
        """Respond to an authenticated account-activity request."""
        parsed = urlparse(self.path)
        assert parsed.path in {"/v2/account/activities/FEE", "/v2/account/activities/FILL"}
        assert self.headers["APCA-API-KEY-ID"] == "test-key"
        assert self.headers["APCA-API-SECRET-KEY"] == "test-secret"
        query = parse_qs(parsed.query)
        self.requests.append(query)
        is_fill = parsed.path.endswith("/FILL")
        if "page_token" in query:
            assert query["page_token"] == ["fee-099"]
            activities = [fill(100) if is_fill else activity(100)]
        else:
            activities = [
                fill(index) if is_fill else activity(index)
                for index in range(100)
            ]
        payload = json.dumps(activities).encode()
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(payload)))
        self.end_headers()
        self.wfile.write(payload)

    def log_message(self, _format: str, *args: object) -> None:
        """Suppress test-server access logs."""


def activity(index: int) -> dict[str, str]:
    """Build a precise regulatory-fee activity payload."""
    return {
        "activity_type": "FEE",
        "activity_sub_type": "REG",
        "id": f"fee-{index:03}",
        "date": "2026-01-16",
        "net_amount": "-0.0001",
    }


def fill(index: int) -> dict[str, str]:
    """Build a precise partial-fill activity payload."""
    return {
        "activity_type": "FILL",
        "id": f"fee-{index:03}",
        "order_id": "venue-order-id",
        "symbol": "AAPL",
        "side": "buy",
        "qty": "0.000000001",
        "price": "202.123456789",
        "cum_qty": f"0.000000{index + 1:03}",
        "leaves_qty": "0.999999999",
        "type": "partial_fill",
        "transaction_time": "2026-01-16T14:30:00.123456789Z",
    }


@pytest.fixture
def activity_server() -> Generator[str, None, None]:
    """Run the local account-activity HTTP fixture."""
    ActivityHandler.requests = []
    server = ThreadingHTTPServer(("127.0.0.1", 0), ActivityHandler)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    try:
        yield f"http://127.0.0.1:{server.server_port}"
    finally:
        server.shutdown()
        server.server_close()
        thread.join()


@pytest.mark.asyncio
async def test_get_all_activities_follows_native_cursor_and_preserves_decimals(
    activity_server: str,
) -> None:
    """Follow native cursors through the public async Python API."""
    client = AlpacaAccountActivityClient(
        api_key="test-key",
        api_secret="test-secret",
        base_url=activity_server,
    )

    activities = await client.get_all_activities("FEE", max_items=101)

    assert len(activities) == 101
    assert activities[0]["id"] == "fee-000"
    assert activities[-1]["id"] == "fee-100"
    assert activities[-1]["net_amount"] == "-0.0001"
    assert ActivityHandler.requests[0]["page_size"] == ["100"]
    assert ActivityHandler.requests[1]["page_size"] == ["1"]


def test_activity_range_and_bounds_fail_before_network() -> None:
    """Reject invalid bounds synchronously without issuing requests."""
    client = AlpacaAccountActivityClient(api_key="test-key", api_secret="test-secret")

    with pytest.raises(ValueError, match="between 1 and 100000"):
        client.get_all_activities("FEE", max_items=0)
    with pytest.raises(ValueError, match="earlier than end"):
        client.get_all_activities(
            "FEE",
            start="2026-02-01T00:00:00Z",
            end="2026-01-01T00:00:00Z",
        )


@pytest.mark.asyncio
async def test_get_fills_paginates_and_preserves_execution_precision(
    activity_server: str,
) -> None:
    """Return every partial execution with exact venue quantities and prices."""
    client = AlpacaAccountActivityClient(
        api_key="test-key",
        api_secret="test-secret",
        base_url=activity_server,
    )

    fills = await client.get_fills(order_id="venue-order-id", max_items=101)

    assert len(fills) == 101
    assert fills[0]["qty"] == "0.000000001"
    assert fills[-1]["price"] == "202.123456789"
    assert fills[-1]["fill_type"] == "partial_fill"
    assert ActivityHandler.requests[0]["page_size"] == ["100"]
    assert ActivityHandler.requests[1]["page_size"] == ["1"]


def test_fill_validation_fails_before_network() -> None:
    """Reject invalid fill ranges and bounds synchronously."""
    client = AlpacaAccountActivityClient(api_key="test-key", api_secret="test-secret")

    with pytest.raises(ValueError, match="fill max_items must be between"):
        client.get_fills(max_items=0)
    with pytest.raises(ValueError, match="fill order_id must be non-empty"):
        client.get_fills(order_id=" ")
    with pytest.raises(ValueError, match="fill start must be earlier"):
        client.get_fills(
            start="2026-02-01T00:00:00Z",
            end="2026-01-01T00:00:00Z",
        )
