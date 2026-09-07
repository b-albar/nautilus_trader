# -------------------------------------------------------------------------------------------------
#  Copyright (C) 2015-2026 Nautech Systems Pty Ltd. All rights reserved.
#  https://nautechsystems.io
#
#  Licensed under the GNU Lesser General Public License Version 3.0 (the "License");
# -------------------------------------------------------------------------------------------------

"""Tests for the native read-only Alpaca portfolio Python client."""

import json
import threading
from collections.abc import Generator
from http.server import BaseHTTPRequestHandler
from http.server import ThreadingHTTPServer
from typing import ClassVar
from urllib.parse import parse_qs
from urllib.parse import urlparse

import pytest

from nautilus_trader.adapters.alpaca import AlpacaPortfolioClient


class PortfolioHandler(BaseHTTPRequestHandler):
    """Serve deterministic Alpaca account and position snapshots."""

    calendar_queries: ClassVar[list[dict[str, list[str]]]] = []
    order_queries: ClassVar[list[dict[str, list[str]]]] = []

    def do_GET(self) -> None:
        """Respond to an authenticated portfolio request."""
        assert self.headers["APCA-API-KEY-ID"] == "test-key"
        assert self.headers["APCA-API-SECRET-KEY"] == "test-secret"
        parsed = urlparse(self.path)
        if parsed.path == "/v2/account":
            response: object = {
                "id": "account-id",
                "account_number": "PA123456",
                "status": "ACTIVE",
                "currency": "USD",
                "cash": "12345.670000001",
                "buying_power": "24691.340000002",
                "equity": "13345.670000001",
                "portfolio_value": "13345.670000001",
                "long_market_value": "1000.00",
                "short_market_value": "0",
                "trading_blocked": False,
                "transfers_blocked": False,
                "account_blocked": False,
                "trade_suspended_by_user": False,
                "shorting_enabled": True,
                "multiplier": "2",
                "created_at": "2026-01-01T12:00:00Z",
                "regt_buying_power": "20000.000000001",
                "non_marginable_buying_power": "12000.000000001",
                "initial_margin": "500.000000001",
                "maintenance_margin": "300.000000001",
                "last_equity": "13200.000000001",
                "last_maintenance_margin": "290.000000001",
                "sma": "10.000000001",
                "accrued_fees": "1.230000001",
                "pending_transfer_in": "500.000000001",
                "options_buying_power": "8000.000000001",
                "options_approved_level": 2,
                "options_trading_level": 1,
                "crypto_status": "ACTIVE",
            }
        elif parsed.path == "/v2/positions":
            response = [
                {
                    "asset_id": "asset-id",
                    "symbol": "AAPL",
                    "asset_class": "us_equity",
                    "qty": "0.123456789",
                    "side": "long",
                    "avg_entry_price": "199.000000001",
                    "market_value": "25.000000001",
                    "cost_basis": "24.567901211",
                    "unrealized_pl": "0.432098790",
                    "unrealized_plpc": "0.017586963",
                    "current_price": "202.500000001",
                    "lastday_price": "201.00",
                    "change_today": "0.007462687",
                },
            ]
        elif parsed.path == "/v2/account/portfolio/history":
            query = parse_qs(parsed.query)
            assert query == {
                "period": ["1M"],
                "timeframe": ["1D"],
                "intraday_reporting": ["market_hours"],
                "cashflow_types": ["DIV,FEE"],
            }
            response = {
                "timestamp": [1788177600, 1788264000, 1788350400],
                "equity": ["10000.00", None, "10123.45"],
                "profit_loss": ["0.00", None, "123.45"],
                "profit_loss_pct": ["0.0000", None, "0.0123"],
                "base_value": "10000.00",
                "base_value_asof": "2026-08-31",
                "timeframe": "1D",
            }
        elif parsed.path == "/v2/clock":
            response = {
                "timestamp": "2026-07-03T12:00:00-04:00",
                "is_open": True,
                "next_open": "2026-07-06T09:30:00-04:00",
                "next_close": "2026-07-03T13:00:00-04:00",
            }
        elif parsed.path == "/v2/calendar":
            self.calendar_queries.append(parse_qs(parsed.query))
            response = [
                {
                    "date": "2026-07-03",
                    "open": "09:30",
                    "close": "13:00",
                    "session_open": "0700",
                    "session_close": "1500",
                    "settlement_date": "2026-07-07",
                },
            ]
        elif parsed.path == "/v2/orders":
            query = parse_qs(parsed.query)
            self.order_queries.append(query)
            if "before_order_id" in query:
                response = [order(500)]
            else:
                response = [order(index) for index in range(500)]
        elif parsed.path == "/v2/orders/order-042":
            query = parse_qs(parsed.query)
            assert query == {"nested": ["true"]}
            response = order(42)
        elif parsed.path == "/v2/orders:by_client_order_id":
            query = parse_qs(parsed.query)
            assert query == {"client_order_id": ["client-042"]}
            response = order(42)
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


def order(index: int) -> dict[str, object]:
    """Build an exact fractional order response."""
    return {
        "id": f"order-{index:03}",
        "client_order_id": f"client-{index:03}",
        "symbol": "AAPL",
        "asset_class": "us_equity",
        "qty": "0.123456789",
        "filled_qty": "0.023456789",
        "filled_avg_price": "202.500000001",
        "side": "buy",
        "type": "limit",
        "time_in_force": "day",
        "limit_price": "203.000000001",
        "stop_price": None,
        "status": "partially_filled",
        "extended_hours": False,
        "created_at": "2026-07-03T14:00:00Z",
        "updated_at": "2026-07-03T14:01:00Z",
        "submitted_at": "2026-07-03T14:00:01Z",
        "filled_at": None,
        "canceled_at": None,
        "expired_at": None,
        "failed_at": None,
        "legs": [],
    }


@pytest.fixture
def portfolio_server() -> Generator[str, None, None]:
    """Run the local portfolio HTTP fixture."""
    PortfolioHandler.calendar_queries = []
    PortfolioHandler.order_queries = []
    server = ThreadingHTTPServer(("127.0.0.1", 0), PortfolioHandler)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    try:
        yield f"http://127.0.0.1:{server.server_port}"
    finally:
        server.shutdown()
        server.server_close()
        thread.join()


@pytest.mark.asyncio
async def test_portfolio_snapshots_preserve_exact_decimals(portfolio_server: str) -> None:
    """Return authenticated snapshots without floating-point conversion."""
    client = AlpacaPortfolioClient(
        api_key="test-key",
        api_secret="test-secret",
        base_url=portfolio_server,
    )

    account = await client.get_account()
    positions = await client.get_positions()

    assert account["cash"] == "12345.670000001"
    assert account["buying_power"] == "24691.340000002"
    assert account["pattern_day_trader"] is None
    assert account["accrued_fees"] == "1.230000001"
    assert account["options_trading_level"] == 1
    assert account["crypto_status"] == "ACTIVE"
    assert positions[0]["symbol"] == "AAPL"
    assert positions[0]["qty"] == "0.123456789"
    assert positions[0]["unrealized_pl"] == "0.432098790"


@pytest.mark.asyncio
async def test_market_schedule_exposes_early_close_and_exact_range(portfolio_server: str) -> None:
    """Expose clock boundaries and early-close sessions through the read-only API."""
    client = AlpacaPortfolioClient(
        api_key="test-key",
        api_secret="test-secret",
        base_url=portfolio_server,
    )

    clock = await client.get_market_clock()
    calendar = await client.get_calendar(start="2026-07-03", end="2026-07-03")

    assert clock["is_open"] is True
    assert clock["next_close"] == "2026-07-03T17:00:00Z"
    assert calendar[0]["close"] == "13:00"
    assert calendar[0]["settlement_date"] == "2026-07-07"
    assert PortfolioHandler.calendar_queries == [
        {"start": ["2026-07-03"], "end": ["2026-07-03"]},
    ]


@pytest.mark.asyncio
async def test_portfolio_history_preserves_aligned_exact_performance_series(
    portfolio_server: str,
) -> None:
    """Return chart-ready equity and P&L arrays without float conversion."""
    client = AlpacaPortfolioClient(
        api_key="test-key",
        api_secret="test-secret",
        base_url=portfolio_server,
    )

    history = await client.get_portfolio_history(
        period="1M",
        timeframe="1D",
        cashflow_types="DIV,FEE",
    )

    assert history["timestamp"] == [1788177600, 1788264000, 1788350400]
    assert history["equity"] == ["10000.00", None, "10123.45"]
    assert history["profit_loss"][-1] == "123.45"
    assert history["profit_loss_pct"][-1] == "0.0123"
    assert history["base_value"] == "10000.00"


@pytest.mark.asyncio
async def test_order_history_paginates_and_preserves_fractional_values(
    portfolio_server: str,
) -> None:
    """Follow order cursors while retaining exact prices and quantities."""
    client = AlpacaPortfolioClient(
        api_key="test-key",
        api_secret="test-secret",
        base_url=portfolio_server,
    )

    orders = await client.get_orders(
        status="all",
        symbols="AAPL",
        after="2026-07-01T00:00:00Z",
        until="2026-07-04T00:00:00Z",
        max_items=501,
    )

    assert len(orders) == 501
    assert orders[0]["qty"] == "0.123456789"
    assert orders[-1]["limit_price"] == "203.000000001"
    assert PortfolioHandler.order_queries[0]["limit"] == ["500"]
    assert PortfolioHandler.order_queries[0]["nested"] == ["true"]
    assert PortfolioHandler.order_queries[1]["limit"] == ["1"]
    assert PortfolioHandler.order_queries[1]["before_order_id"] == ["order-499"]
    assert "after" not in PortfolioHandler.order_queries[1]
    assert "until" not in PortfolioHandler.order_queries[1]


@pytest.mark.asyncio
async def test_order_drill_down_supports_venue_and_client_ids(portfolio_server: str) -> None:
    """Retrieve a precise order directly through either stable identifier."""
    client = AlpacaPortfolioClient(
        api_key="test-key",
        api_secret="test-secret",
        base_url=portfolio_server,
    )

    by_venue = await client.get_order("order-042")
    by_client = await client.get_order_by_client_order_id("client-042")

    assert by_venue["id"] == "order-042"
    assert by_client["client_order_id"] == "client-042"
    assert by_venue["filled_avg_price"] == "202.500000001"


def test_portfolio_client_rejects_zero_timeout() -> None:
    """Reject a zero timeout before making any network request."""
    with pytest.raises(ValueError, match="timeout must be positive"):
        AlpacaPortfolioClient(
            api_key="test-key",
            api_secret="test-secret",
            timeout_secs=0,
        )


def test_calendar_range_validation_fails_before_network() -> None:
    """Reject malformed and descending date ranges synchronously."""
    client = AlpacaPortfolioClient(api_key="test-key", api_secret="test-secret")

    with pytest.raises(ValueError, match="Invalid Alpaca calendar start date"):
        client.get_calendar(start="03/07/2026")
    with pytest.raises(ValueError, match="start must not be later than end"):
        client.get_calendar(start="2026-07-04", end="2026-07-03")


def test_portfolio_history_validation_fails_before_network() -> None:
    """Reject unsupported performance ranges and display modes synchronously."""
    client = AlpacaPortfolioClient(api_key="test-key", api_secret="test-secret")

    with pytest.raises(ValueError, match="period must be a positive number"):
        client.get_portfolio_history(period="0M")
    with pytest.raises(ValueError, match="portfolio timeframe must be"):
        client.get_portfolio_history(timeframe="1Hour")
    with pytest.raises(ValueError, match="intraday_reporting must be"):
        client.get_portfolio_history(intraday_reporting="overnight")
    with pytest.raises(ValueError, match="cashflow_types must be"):
        client.get_portfolio_history(cashflow_types="DIV, FEE")
    with pytest.raises(ValueError, match="only two of period, start, and end"):
        client.get_portfolio_history(
            period="1M",
            start="2026-08-01T00:00:00Z",
            end="2026-09-01T00:00:00Z",
        )


def test_order_query_validation_fails_before_network() -> None:
    """Reject unsafe unbounded or malformed order filters synchronously."""
    client = AlpacaPortfolioClient(api_key="test-key", api_secret="test-secret")

    with pytest.raises(ValueError, match="status must be"):
        client.get_orders(status="pending")
    with pytest.raises(ValueError, match="max_items must be between"):
        client.get_orders(max_items=0)
    with pytest.raises(ValueError, match="comma-separated"):
        client.get_orders(symbols="AAPL, MSFT")
    with pytest.raises(ValueError, match="after timestamp must be earlier"):
        client.get_orders(
            after="2026-07-04T00:00:00Z",
            until="2026-07-03T00:00:00Z",
        )
    with pytest.raises(ValueError, match="order_id must be non-empty"):
        client.get_order("")
    with pytest.raises(ValueError, match="client_order_id must be non-empty"):
        client.get_order_by_client_order_id("  ")
