# -------------------------------------------------------------------------------------------------
#  Copyright (C) 2015-2026 Nautech Systems Pty Ltd. All rights reserved.
#  https://nautechsystems.io
#
#  Licensed under the GNU Lesser General Public License Version 3.0 (the "License");
# -------------------------------------------------------------------------------------------------

"""Tests for the native read-only Alpaca reference-data Python client."""

import json
import threading
from collections.abc import Generator
from http.server import BaseHTTPRequestHandler
from http.server import ThreadingHTTPServer
from typing import ClassVar
from urllib.parse import parse_qs
from urllib.parse import urlparse

import pytest

from nautilus_trader.adapters.alpaca import AlpacaReferenceDataClient


def asset() -> dict[str, object]:
    """Build a venue-authored fractionable equity definition."""
    return {
        "id": "asset-id",
        "class": "us_equity",
        "exchange": "NASDAQ",
        "symbol": "AAPL",
        "name": "Apple Inc. Common Stock",
        "status": "active",
        "tradable": True,
        "marginable": True,
        "shortable": True,
        "borrow_status": "easy_to_borrow",
        "fractionable": True,
        "cusip": "037833100",
        "maintenance_margin_requirement": "30.000000001",
        "margin_requirement_long": "30.000000001",
        "margin_requirement_short": "35.000000001",
        "min_order_size": "0.000000001",
        "min_trade_increment": "0.000000001",
        "price_increment": "0.0001",
        "attributes": ["fractional_eh_enabled"],
    }


def watchlist() -> dict[str, object]:
    """Build a watchlist with its venue-resolved assets."""
    return {
        "id": "watchlist-id",
        "account_id": "account-id",
        "created_at": "2026-01-01T12:00:00Z",
        "updated_at": "2026-01-02T12:00:00Z",
        "name": "Momentum",
        "assets": [asset()],
    }


def option_contract(symbol: str = "AAPL260116C00200000") -> dict[str, object]:
    """Build one venue-authored OCC option contract."""
    return {
        "id": f"contract-{symbol}",
        "symbol": symbol,
        "name": "AAPL Jan 16 2026 200 Call",
        "status": "active",
        "tradable": True,
        "expiration_date": "2026-01-16",
        "underlying_symbol": "AAPL",
        "underlying_asset_id": "asset-id",
        "type": "call",
        "style": "american",
        "strike_price": "200.000000001",
        "multiplier": "100",
        "size": "100",
        "root_symbol": "AAPL",
        "open_interest": "237",
        "open_interest_date": "2026-01-02",
        "close_price": "12.340000001",
        "close_price_date": "2026-01-02",
        "deliverables": [
            {
                "type": "equity",
                "symbol": "AAPL",
                "asset_id": "asset-id",
                "amount": "100",
                "allocation_percentage": "100",
                "settlement_type": "T+1",
                "settlement_method": "CCC",
                "delayed_settlement": False,
            },
        ],
    }


class ReferenceDataHandler(BaseHTTPRequestHandler):
    """Serve deterministic Alpaca asset responses."""

    queries: ClassVar[list[dict[str, list[str]]]] = []

    def do_GET(self) -> None:
        """Respond to an authenticated asset request."""
        assert self.headers["APCA-API-KEY-ID"] == "test-key"
        assert self.headers["APCA-API-SECRET-KEY"] == "test-secret"
        parsed = urlparse(self.path)
        if parsed.path == "/v2/assets/AAPL":
            response: object = asset()
        elif parsed.path == "/v2/assets":
            self.queries.append(parse_qs(parsed.query))
            response = [asset()]
        elif parsed.path == "/v2/options/contracts/AAPL260116C00200000":
            response = option_contract()
        elif parsed.path == "/v2/options/contracts":
            query = parse_qs(parsed.query)
            self.queries.append(query)
            page = 2 if "page_token" in query else 1
            response = {
                "option_contracts": [option_contract(f"AAPL260116C0020000{page - 1}")],
                "next_page_token": "contracts-2" if page == 1 else None,
            }
        elif parsed.path == "/v2/watchlists":
            response = [watchlist()]
        elif parsed.path == "/v2/watchlists/watchlist-id":
            response = watchlist()
        elif parsed.path == "/v2/watchlists:by_name":
            assert parse_qs(parsed.query) == {"name": ["Momentum"]}
            response = watchlist()
        else:
            self.send_error(404)
            return

        payload = json.dumps(response).encode()
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(payload)))
        self.end_headers()
        self.wfile.write(payload)

    def log_message(self, _format: str, *args: object) -> None:
        """Suppress test-server access logs."""


@pytest.fixture
def reference_data_server() -> Generator[str, None, None]:
    """Run the local reference-data HTTP fixture."""
    ReferenceDataHandler.queries = []
    server = ThreadingHTTPServer(("127.0.0.1", 0), ReferenceDataHandler)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    try:
        yield f"http://127.0.0.1:{server.server_port}"
    finally:
        server.shutdown()
        server.server_close()
        thread.join()


@pytest.mark.asyncio
async def test_asset_lookup_preserves_venue_capabilities_and_increments(
    reference_data_server: str,
) -> None:
    """Expose exact instrument metadata through the public Python client."""
    client = AlpacaReferenceDataClient(
        api_key="test-key",
        api_secret="test-secret",
        base_url=reference_data_server,
    )

    result = await client.get_asset("AAPL")

    assert result["symbol"] == "AAPL"
    assert result["fractionable"] is True
    assert result["borrow_status"] == "easy_to_borrow"
    assert result["easy_to_borrow"] is None
    assert result["cusip"] == "037833100"
    assert result["margin_requirement_long"] == "30.000000001"
    assert result["margin_requirement_short"] == "35.000000001"
    assert result["min_trade_increment"] == "0.000000001"
    assert result["price_increment"] == "0.0001"


@pytest.mark.asyncio
async def test_asset_discovery_sends_explicit_native_filters(reference_data_server: str) -> None:
    """Filter the venue master without loading a Nautilus live node."""
    client = AlpacaReferenceDataClient(
        api_key="test-key",
        api_secret="test-secret",
        base_url=reference_data_server,
    )

    results = await client.get_assets(
        status="active",
        exchange="NASDAQ",
        attributes=["has_options", "overnight_tradable"],
    )

    assert results[0]["name"] == "Apple Inc. Common Stock"
    assert ReferenceDataHandler.queries == [
        {
            "status": ["active"],
            "asset_class": ["us_equity"],
            "exchange": ["NASDAQ"],
            "attributes": ["has_options,overnight_tradable"],
        },
    ]


@pytest.mark.asyncio
async def test_option_contracts_are_filterable_paginated_and_exact(
    reference_data_server: str,
) -> None:
    """Expose contract terms and non-standard deliverables for option research."""
    client = AlpacaReferenceDataClient(
        api_key="test-key",
        api_secret="test-secret",
        base_url=reference_data_server,
    )

    contract = await client.get_option_contract("AAPL260116C00200000")
    contracts = await client.get_option_contracts(
        underlying_symbols=["AAPL"],
        option_type="call",
        expiration_date_gte="2026-01-01",
        expiration_date_lte="2026-01-31",
        strike_price_gte="190.000000001",
        strike_price_lte="210.000000001",
        show_deliverables=True,
        ppind=True,
        max_items=2,
    )

    assert contract["strike_price"] == "200.000000001"
    assert contract["option_type"] == "call"
    assert contract["deliverables"][0]["settlement_type"] == "T+1"
    assert len(contracts) == 2
    queries = ReferenceDataHandler.queries
    assert queries[0]["underlying_symbols"] == ["AAPL"]
    assert queries[0]["show_deliverables"] == ["true"]
    assert queries[0]["strike_price_gte"] == ["190.000000001"]
    assert queries[1]["page_token"] == ["contracts-2"]
    assert queries[1]["limit"] == ["1"]


@pytest.mark.asyncio
async def test_watchlist_reads_resolve_saved_research_universes(reference_data_server: str) -> None:
    """Read saved universes by list, stable ID, and ergonomic display name."""
    client = AlpacaReferenceDataClient(
        api_key="test-key",
        api_secret="test-secret",
        base_url=reference_data_server,
    )

    watchlists = await client.get_watchlists()
    by_id = await client.get_watchlist("watchlist-id")
    by_name = await client.get_watchlist_by_name("Momentum")

    assert watchlists[0]["name"] == "Momentum"
    assert by_id["assets"][0]["symbol"] == "AAPL"
    assert by_name["assets"][0]["min_trade_increment"] == "0.000000001"


def test_reference_data_validation_fails_before_network() -> None:
    """Reject malformed lookup values synchronously."""
    client = AlpacaReferenceDataClient(api_key="test-key", api_secret="test-secret")

    with pytest.raises(ValueError, match="symbol_or_asset_id"):
        client.get_asset("AAPL/USD")
    with pytest.raises(ValueError, match="asset status"):
        client.get_assets(status="delisted")
    with pytest.raises(ValueError, match="exchange must be non-empty"):
        client.get_assets(exchange=" ")
    with pytest.raises(ValueError, match="at least one value"):
        client.get_assets(attributes=[])
    with pytest.raises(ValueError, match="Unsupported Alpaca asset attribute"):
        client.get_assets(attributes=["quantum_ready"])
    with pytest.raises(ValueError, match="must not contain duplicates"):
        client.get_assets(attributes=["has_options", "has_options"])
    with pytest.raises(ValueError, match="watchlist_id must be non-empty"):
        client.get_watchlist("bad/id")
    with pytest.raises(ValueError, match="watchlist name must be non-empty"):
        client.get_watchlist_by_name(" ")
    with pytest.raises(ValueError, match="option type must be"):
        client.get_option_contracts(option_type="straddle")
    with pytest.raises(ValueError, match="minimum strike must not exceed"):
        client.get_option_contracts(strike_price_gte="210", strike_price_lte="200")
