# -------------------------------------------------------------------------------------------------
#  Copyright (C) 2015-2026 Nautech Systems Pty Ltd. All rights reserved.
#  https://nautechsystems.io
#
#  Licensed under the GNU Lesser General Public License Version 3.0 (the "License");
# -------------------------------------------------------------------------------------------------

"""Tests for the native direct Alpaca historical-data Python client."""

import json
import threading
from collections.abc import Generator
from http.server import BaseHTTPRequestHandler
from http.server import ThreadingHTTPServer
from typing import ClassVar
from urllib.parse import parse_qs
from urllib.parse import urlparse

import pytest

from nautilus_trader.adapters.alpaca import AlpacaDataFeed
from nautilus_trader.adapters.alpaca import AlpacaHistoricalDataClient


class HistoricalDataHandler(BaseHTTPRequestHandler):
    """Serve cursor-paginated Alpaca market data."""

    queries: ClassVar[dict[str, list[dict[str, list[str]]]]] = {}

    def do_GET(self) -> None:
        """Respond to an authenticated historical-data request."""
        assert self.headers["APCA-API-KEY-ID"] == "test-key"
        assert self.headers["APCA-API-SECRET-KEY"] == "test-secret"
        parsed = urlparse(self.path)
        query = parse_qs(parsed.query)
        self.queries.setdefault(parsed.path, []).append(query)

        screen = discovery_response(parsed.path, query)
        if screen is not None:
            response = screen
        elif parsed.path == "/v2/stocks/AAPL/snapshot":
            response: object = {
                "latestTrade": {
                    "i": 43,
                    "p": "200.123456789",
                    "s": "0.123456789",
                    "t": "2026-01-02T14:31:00.123456789Z",
                },
                "latestQuote": {
                    "bp": "200.100000001",
                    "bs": "1.500000001",
                    "ap": "200.200000001",
                    "as": "2.500000001",
                    "t": "2026-01-02T14:31:00.123456789Z",
                },
                "minuteBar": bar("2026-01-02T14:31:00Z"),
                "dailyBar": bar("2026-01-02T05:00:00Z"),
                "prevDailyBar": bar("2025-12-31T05:00:00Z"),
            }
        elif parsed.path == "/v2/stocks/snapshots":
            response = {
                symbol: {
                    "latestTrade": {
                        "i": index,
                        "p": "200.123456789",
                        "s": "0.123456789",
                        "t": "2026-01-02T14:31:00.123456789Z",
                    },
                    "latestQuote": None,
                    "minuteBar": bar("2026-01-02T14:31:00Z"),
                    "dailyBar": None,
                    "prevDailyBar": None,
                }
                for index, symbol in enumerate(query["symbols"][0].split(","), start=1)
            }
        elif parsed.path == "/v2/stocks/bars":
            page = 2 if "page_token" in query else 1
            requested_symbols = query["symbols"][0].split(",")
            response_symbol = requested_symbols[min(page - 1, len(requested_symbols) - 1)]
            response = {
                "bars": {
                    response_symbol: [
                        {
                            "t": f"2026-01-02T14:3{page}:00Z",
                            "o": "200.000000001",
                            "h": "201.000000001",
                            "l": "199.999999999",
                            "c": "200.500000001",
                            "v": "1234.000000001",
                            "n": page,
                            "vw": "200.250000001",
                        },
                    ],
                },
                "next_page_token": "bars-2" if page == 1 else None,
            }
        elif parsed.path == "/v2/stocks/trades":
            response = {
                "trades": {
                    symbol: [
                        {
                            "i": 42 + index,
                            "p": "200.123456789",
                            "s": "0.123456789",
                            "t": "2026-01-02T14:30:00.123456789Z",
                            "x": "V",
                            "c": ["@", "I"],
                            "z": "C",
                        },
                    ]
                    for index, symbol in enumerate(query["symbols"][0].split(","))
                },
                "next_page_token": None,
            }
        elif parsed.path == "/v2/stocks/quotes":
            response = {
                "quotes": {
                    symbol: [
                        {
                            "bx": "V",
                            "bp": "200.100000001",
                            "bs": "1.500000001",
                            "ax": "Q",
                            "ap": "200.200000001",
                            "as": "2.500000001",
                            "t": "2026-01-02T14:30:00.123456789Z",
                            "c": ["R"],
                            "z": "C",
                        },
                    ]
                    for symbol in query["symbols"][0].split(",")
                },
                "next_page_token": None,
            }
        elif parsed.path == "/v2/stocks/auctions":
            page = 2 if "page_token" in query else 1
            requested_symbols = query["symbols"][0].split(",")
            response_symbol = requested_symbols[min(page - 1, len(requested_symbols) - 1)]
            response = {
                "auctions": {
                    response_symbol: [
                        {
                            "d": f"2026-01-0{page}",
                            "o": [
                                {
                                    "t": f"2026-01-0{page}T14:30:00.123456789Z",
                                    "x": "Q",
                                    "p": "200.123456789",
                                    "s": 1234,
                                    "c": "O",
                                },
                            ],
                            "c": [
                                {
                                    "t": f"2026-01-0{page}T21:00:00.123456789Z",
                                    "x": "Q",
                                    "p": "201.123456789",
                                    "c": "6",
                                },
                            ],
                        },
                    ],
                },
                "currency": "USD",
                "next_page_token": "auctions-2" if page == 1 else None,
            }
        elif parsed.path == "/v1beta1/news":
            page = 2 if "page_token" in query else 1
            response = {
                "news": [news_article(page)],
                "next_page_token": "news-2" if page == 1 else None,
            }
        elif parsed.path == "/v1/corporate-actions":
            page = 2 if "page_token" in query else 1
            response = {
                "cash_dividends" if page == 1 else "forward_splits": [
                    corporate_action(page),
                ],
                "next_page_token": "actions-2" if page == 1 else None,
            }
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


def bar(timestamp: str) -> dict[str, object]:
    """Build one exact Alpaca bar payload."""
    return {
        "t": timestamp,
        "o": "200.000000001",
        "h": "201.000000001",
        "l": "199.999999999",
        "c": "200.500000001",
        "v": "1234.000000001",
        "n": 42,
        "vw": "200.250000001",
    }


def option_discovery_response(
    path: str,
    query: dict[str, list[str]],
) -> dict[str, object] | None:
    """Build option-chain and historical option responses."""
    if path == "/v1beta1/options/snapshots":
        page = 2 if "page_token" in query else 1
        symbol = query["symbols"][0].split(",")[page - 1]
        return {
            "snapshots": {
                symbol: {
                    "latestTrade": {
                        "t": "2026-01-02T15:03:44.56339456Z",
                        "x": "B",
                        "p": "12.340000001",
                        "s": page,
                        "c": "I",
                    },
                    "greeks": {
                        "delta": "0.7521304109871954",
                        "gamma": "0.06241426404871288",
                        "rho": "0.009910739032549095",
                        "theta": "-0.2847623059595503",
                        "vega": "0.047540520834498785",
                    },
                    "impliedVolatility": "0.3372405712050441",
                },
            },
            "next_page_token": "selected-options-2" if page == 1 else None,
        }
    if path == "/v1beta1/options/snapshots/AAPL":
        page = 2 if "page_token" in query else 1
        return {
            "snapshots": {
                f"AAPL260116C0020000{page - 1}": {
                    "latestTrade": {
                        "t": "2026-01-02T15:03:44.56339456Z",
                        "x": "B",
                        "p": "12.340000001",
                        "s": 1,
                        "c": "I",
                    },
                    "latestQuote": {
                        "t": "2026-01-02T15:03:45.123456789Z",
                        "bx": "C",
                        "bp": "12.330000001",
                        "bs": 16,
                        "ap": "12.350000001",
                        "as": 91,
                        "ax": "B",
                        "c": "A",
                    },
                    "greeks": {
                        "delta": "0.7521304109871954",
                        "gamma": "0.06241426404871288",
                        "rho": "0.009910739032549095",
                        "theta": "-0.2847623059595503",
                        "vega": "0.047540520834498785",
                    },
                    "impliedVolatility": "0.3372405712050441",
                },
            },
            "next_page_token": "options-2" if page == 1 else None,
        }
    if path == "/v1beta1/options/bars":
        page = 2 if "page_token" in query else 1
        symbol = query["symbols"][0].split(",")[page - 1]
        return {
            "bars": {symbol: [bar(f"2026-01-02T14:3{page}:00Z")]},
            "currency": "USD",
            "next_page_token": "option-bars-2" if page == 1 else None,
        }
    if path == "/v1beta1/options/trades":
        page = 2 if "page_token" in query else 1
        symbol = query["symbols"][0].split(",")[page - 1]
        return {
            "trades": {
                symbol: [
                    {
                        "t": f"2026-01-02T14:3{page}:00.123456789Z",
                        "x": "B",
                        "p": "12.340000001",
                        "s": page,
                        "c": "I",
                    },
                ],
            },
            "currency": "USD",
            "next_page_token": "option-trades-2" if page == 1 else None,
        }
    return None


def discovery_response(
    path: str,
    query: dict[str, list[str]],
) -> dict[str, object] | None:
    """Build current-market discovery and metadata responses."""
    option_response = option_discovery_response(path, query)
    if option_response is not None:
        return option_response
    if path == "/v1beta1/screener/stocks/most-actives":
        return {
            "most_actives": [
                {"symbol": "AAPL", "volume": 122709184, "trade_count": 639626},
                {"symbol": "MSFT", "volume": 98765432, "trade_count": 432100},
            ],
            "last_updated": "2026-01-02T15:00:00.123456789Z",
        }
    if path == "/v1beta1/screener/stocks/movers":
        return {
            "gainers": [
                {"symbol": "GAIN", "percent_change": 145.56, "change": 2.46, "price": 4.15},
            ],
            "losers": [
                {"symbol": "LOSS", "percent_change": -42.5, "change": -1.7, "price": 2.3},
            ],
            "market_type": "stocks",
            "last_updated": "2026-01-02T15:00:00.123456789Z",
        }
    if path == "/v2/stocks/meta/exchanges":
        return {"N": "New York Stock Exchange", "V": "IEX"}
    if path.startswith("/v2/stocks/meta/conditions/"):
        return {
            "@": "Regular Sale" if query["tape"] == ["C"] else "Regular Condition",
            "I": "Odd Lot Trade",
        }
    return None


def news_article(index: int) -> dict[str, object]:
    """Build one historical news article with optional full content."""
    return {
        "id": index,
        "headline": f"Apple event {index}",
        "author": "Research Desk",
        "created_at": f"2026-01-0{index}T12:00:00Z",
        "updated_at": f"2026-01-0{index}T12:05:00Z",
        "summary": "A deterministic event-study fixture.",
        "content": "Full article body for sentiment analysis.",
        "url": f"https://example.test/news/{index}",
        "images": [{"size": "large", "url": "https://example.test/image.jpg"}],
        "symbols": ["AAPL"],
        "source": "benzinga",
    }


def corporate_action(index: int) -> dict[str, object]:
    """Build one structured corporate-action event with exact decimal fields."""
    return {
        "id": f"action-{index}",
        "process_date": f"2026-01-0{index}",
        "symbol": "AAPL",
        "cusip": "037833100",
        "rate": "0.2400000001" if index == 1 else "4.0",
        "ex_date": f"2026-01-0{index + 1}",
    }


@pytest.fixture
def historical_data_server() -> Generator[str, None, None]:
    """Run the local historical-data HTTP fixture."""
    HistoricalDataHandler.queries = {}
    server = ThreadingHTTPServer(("127.0.0.1", 0), HistoricalDataHandler)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    try:
        yield f"http://127.0.0.1:{server.server_port}"
    finally:
        server.shutdown()
        server.server_close()
        thread.join()


@pytest.mark.asyncio
async def test_historical_bars_paginate_with_friendly_exact_fields(
    historical_data_server: str,
) -> None:
    """Follow cursors while exposing descriptive field names and exact decimals."""
    client = AlpacaHistoricalDataClient(
        api_key="test-key",
        api_secret="test-secret",
        base_url=historical_data_server,
    )

    bars = await client.get_bars(
        "AAPL",
        "1Min",
        "2026-01-02T14:30:00Z",
        "2026-01-02T15:00:00Z",
        max_items=2,
    )

    assert len(bars) == 2
    assert bars[0]["open"] == "200.000000001"
    assert bars[1]["trade_count"] == 2
    queries = HistoricalDataHandler.queries["/v2/stocks/bars"]
    assert queries[0]["limit"] == ["2"]
    assert queries[0]["adjustment"] == ["raw"]
    assert queries[1]["limit"] == ["1"]
    assert queries[1]["page_token"] == ["bars-2"]


@pytest.mark.asyncio
async def test_historical_ticks_preserve_prices_sizes_and_lot_units(
    historical_data_server: str,
) -> None:
    """Return exact trades and explicitly named quote lot sizes."""
    client = AlpacaHistoricalDataClient(
        api_key="test-key",
        api_secret="test-secret",
        base_url=historical_data_server,
    )
    bounds = ("2026-01-02T14:30:00Z", "2026-01-02T15:00:00Z")

    trades = await client.get_trades("AAPL", *bounds)
    quotes = await client.get_quotes("AAPL", *bounds)

    assert trades[0]["price"] == "200.123456789"
    assert trades[0]["size"] == "0.123456789"
    assert trades[0]["exchange"] == "V"
    assert trades[0]["conditions"] == ["@", "I"]
    assert trades[0]["tape"] == "C"
    assert quotes[0]["bid_size_lots"] == "1.500000001"
    assert quotes[0]["ask_price"] == "200.200000001"
    assert quotes[0]["bid_exchange"] == "V"
    assert quotes[0]["ask_exchange"] == "Q"
    assert quotes[0]["conditions"] == ["R"]


@pytest.mark.asyncio
async def test_multi_symbol_history_returns_grouped_exact_data(
    historical_data_server: str,
) -> None:
    """Load a research universe without serial per-symbol requests."""
    client = AlpacaHistoricalDataClient(
        api_key="test-key",
        api_secret="test-secret",
        base_url=historical_data_server,
    )
    bounds = ("2026-01-02T14:30:00Z", "2026-01-02T15:00:00Z")

    bars = await client.get_bars_multi(
        ["AAPL", "MSFT"],
        "1Min",
        *bounds,
        max_items=2,
        asof="2026-01-02",
    )
    trades = await client.get_trades_multi(["AAPL", "MSFT"], *bounds, max_items=2)
    quotes = await client.get_quotes_multi(["AAPL", "MSFT"], *bounds, max_items=2)

    assert list(bars) == ["AAPL", "MSFT"]
    assert bars["MSFT"][0]["close"] == "200.500000001"
    assert trades["AAPL"][0]["price"] == "200.123456789"
    assert trades["MSFT"][0]["trade_id"] == 43
    assert quotes["MSFT"][0]["ask_size_lots"] == "2.500000001"
    assert HistoricalDataHandler.queries["/v2/stocks/bars"][0]["symbols"] == ["AAPL,MSFT"]
    assert HistoricalDataHandler.queries["/v2/stocks/bars"][0]["asof"] == ["2026-01-02"]
    assert HistoricalDataHandler.queries["/v2/stocks/bars"][1]["page_token"] == ["bars-2"]
    assert HistoricalDataHandler.queries["/v2/stocks/bars"][1]["asof"] == ["2026-01-02"]
    assert HistoricalDataHandler.queries["/v2/stocks/trades"][0]["limit"] == ["2"]


@pytest.mark.asyncio
async def test_auctions_preserve_open_close_semantics_and_precision(
    historical_data_server: str,
) -> None:
    """Load SIP auction prints for execution and market-on-close research."""
    client = AlpacaHistoricalDataClient(
        api_key="test-key",
        api_secret="test-secret",
        base_url=historical_data_server,
    )
    auctions = await client.get_auctions(
        ["AAPL", "MSFT"],
        "2026-01-01T00:00:00Z",
        "2026-01-03T00:00:00Z",
        max_items=2,
        asof="2026-01-03",
    )

    assert auctions["AAPL"][0]["opening"][0]["price"] == "200.123456789"
    assert auctions["AAPL"][0]["opening"][0]["size"] == "1234"
    assert auctions["MSFT"][0]["closing"][0]["size"] is None
    assert auctions["MSFT"][0]["closing"][0]["condition"] == "6"
    queries = HistoricalDataHandler.queries["/v2/stocks/auctions"]
    assert queries[0]["feed"] == ["sip"]
    assert queries[0]["limit"] == ["2"]
    assert queries[1]["page_token"] == ["auctions-2"]
    assert queries[1]["asof"] == ["2026-01-03"]


@pytest.mark.asyncio
async def test_snapshot_combines_latest_market_context(historical_data_server: str) -> None:
    """Return one ergonomic exact snapshot for instrument panels and assistant tools."""
    client = AlpacaHistoricalDataClient(
        api_key="test-key",
        api_secret="test-secret",
        base_url=historical_data_server,
    )

    snapshot = await client.get_snapshot("AAPL", feed=AlpacaDataFeed.IEX)

    assert snapshot["latest_trade"]["price"] == "200.123456789"
    assert snapshot["latest_quote"]["bid_size_lots"] == "1.500000001"
    assert snapshot["minute_bar"]["close"] == "200.500000001"
    assert snapshot["daily_bar"]["volume"] == "1234.000000001"
    assert snapshot["previous_daily_bar"]["trade_count"] == 42
    assert HistoricalDataHandler.queries["/v2/stocks/AAPL/snapshot"] == [
        {"feed": ["iex"]},
    ]


@pytest.mark.asyncio
async def test_snapshots_batch_market_context_for_research_screens(
    historical_data_server: str,
) -> None:
    """Retrieve a symbol universe in one authenticated market-data request."""
    client = AlpacaHistoricalDataClient(
        api_key="test-key",
        api_secret="test-secret",
        base_url=historical_data_server,
    )

    snapshots = await client.get_snapshots(["AAPL", "MSFT"], feed=AlpacaDataFeed.SIP)

    assert list(snapshots) == ["AAPL", "MSFT"]
    assert snapshots["AAPL"]["latest_trade"]["price"] == "200.123456789"
    assert snapshots["MSFT"]["minute_bar"]["trade_count"] == 42
    assert HistoricalDataHandler.queries["/v2/stocks/snapshots"] == [
        {"symbols": ["AAPL,MSFT"], "feed": ["sip"]},
    ]


@pytest.mark.asyncio
async def test_option_chain_paginates_with_exact_greeks_and_quotes(
    historical_data_server: str,
) -> None:
    """Return a bounded option chain suitable for volatility research and assistant tools."""
    client = AlpacaHistoricalDataClient(
        api_key="test-key",
        api_secret="test-secret",
        base_url=historical_data_server,
    )

    chain = await client.get_option_chain(
        "AAPL",
        option_type="call",
        strike_price_gte="190.000000001",
        expiration_date_lte="2026-01-31",
        updated_since="2026-01-02T14:00:00Z",
        max_items=2,
    )

    assert list(chain) == ["AAPL260116C00200000", "AAPL260116C00200001"]
    assert chain["AAPL260116C00200000"]["greeks"]["delta"] == "0.7521304109871954"
    assert chain["AAPL260116C00200001"]["latest_quote"]["ask_price"] == "12.350000001"
    assert chain["AAPL260116C00200000"]["implied_volatility"] == "0.3372405712050441"
    queries = HistoricalDataHandler.queries["/v1beta1/options/snapshots/AAPL"]
    assert queries[0]["feed"] == ["indicative"]
    assert queries[0]["limit"] == ["2"]
    assert queries[1]["page_token"] == ["options-2"]
    assert queries[1]["limit"] == ["1"]


@pytest.mark.asyncio
async def test_selected_option_snapshots_avoid_loading_the_full_chain(
    historical_data_server: str,
) -> None:
    """Retrieve exact current state for a bounded strategy contract set."""
    client = AlpacaHistoricalDataClient(
        api_key="test-key",
        api_secret="test-secret",
        base_url=historical_data_server,
    )
    symbols = ["AAPL260116C00200000", "AAPL260116P00200000"]

    snapshots = await client.get_option_snapshots(
        symbols,
        updated_since="2026-01-02",
        max_items=2,
    )

    assert list(snapshots) == symbols
    assert snapshots[symbols[0]]["latest_trade"]["price"] == "12.340000001"
    assert snapshots[symbols[1]]["latest_trade"]["size"] == 2
    assert snapshots[symbols[0]]["greeks"]["vega"] == "0.047540520834498785"
    queries = HistoricalDataHandler.queries["/v1beta1/options/snapshots"]
    assert queries[0]["symbols"] == [",".join(symbols)]
    assert queries[0]["feed"] == ["indicative"]
    assert queries[0]["updated_since"] == ["2026-01-02"]
    assert queries[1]["page_token"] == ["selected-options-2"]
    assert queries[1]["limit"] == ["1"]


@pytest.mark.asyncio
async def test_historical_option_bars_and_trades_are_grouped_and_paginated(
    historical_data_server: str,
) -> None:
    """Load exact contract time series across a research universe."""
    client = AlpacaHistoricalDataClient(
        api_key="test-key",
        api_secret="test-secret",
        base_url=historical_data_server,
    )
    symbols = ["AAPL260116C00200000", "AAPL260116P00200000"]
    bounds = ("2026-01-02T14:30:00Z", "2026-01-02T15:00:00Z")

    bars = await client.get_option_bars(symbols, "1Min", *bounds, max_items=2)
    trades = await client.get_option_trades(symbols, *bounds, max_items=2)

    assert bars[symbols[0]][0]["close"] == "200.500000001"
    assert bars[symbols[1]][0]["volume"] == "1234.000000001"
    assert trades[symbols[0]][0]["price"] == "12.340000001"
    assert trades[symbols[1]][0]["size"] == 2
    bar_queries = HistoricalDataHandler.queries["/v1beta1/options/bars"]
    assert bar_queries[0]["symbols"] == [",".join(symbols)]
    assert bar_queries[0]["timeframe"] == ["1Min"]
    assert bar_queries[0]["limit"] == ["2"]
    assert bar_queries[1]["page_token"] == ["option-bars-2"]
    assert bar_queries[1]["limit"] == ["1"]
    trade_queries = HistoricalDataHandler.queries["/v1beta1/options/trades"]
    assert trade_queries[0]["sort"] == ["asc"]
    assert trade_queries[1]["page_token"] == ["option-trades-2"]


@pytest.mark.asyncio
async def test_screeners_return_ranked_exact_market_context(
    historical_data_server: str,
) -> None:
    """Expose discovery endpoints directly to research and assistant workflows."""
    client = AlpacaHistoricalDataClient(
        api_key="test-key",
        api_secret="test-secret",
        base_url=historical_data_server,
    )

    actives = await client.get_most_actives(by="trades", top=25)
    movers = await client.get_market_movers(top=15)

    assert actives["most_actives"][0] == {
        "symbol": "AAPL",
        "volume": 122709184,
        "trade_count": 639626,
    }
    assert actives["last_updated"] == "2026-01-02T15:00:00.123456789Z"
    assert movers["gainers"][0]["percent_change"] == "145.56"
    assert movers["losers"][0]["price"] == "2.3"
    assert movers["market_type"] == "stocks"
    assert HistoricalDataHandler.queries["/v1beta1/screener/stocks/most-actives"] == [
        {"by": ["trades"], "top": ["25"]},
    ]
    assert HistoricalDataHandler.queries["/v1beta1/screener/stocks/movers"] == [
        {"top": ["15"]},
    ]


@pytest.mark.asyncio
async def test_market_metadata_resolves_wire_codes(historical_data_server: str) -> None:
    """Resolve exchange and condition codes for readable research output."""
    client = AlpacaHistoricalDataClient(
        api_key="test-key",
        api_secret="test-secret",
        base_url=historical_data_server,
    )

    exchanges = await client.get_exchange_codes()
    conditions = await client.get_condition_codes("trade", "C")

    assert exchanges["V"] == "IEX"
    assert conditions == {"@": "Regular Sale", "I": "Odd Lot Trade"}
    assert HistoricalDataHandler.queries["/v2/stocks/meta/conditions/trade"] == [
        {"tape": ["C"]},
    ]


@pytest.mark.asyncio
async def test_news_paginates_with_content_for_event_research(historical_data_server: str) -> None:
    """Return bounded historical articles suitable for NLP and event studies."""
    client = AlpacaHistoricalDataClient(
        api_key="test-key",
        api_secret="test-secret",
        base_url=historical_data_server,
    )

    articles = await client.get_news(
        symbols="AAPL",
        start="2026-01-01T00:00:00Z",
        end="2026-01-03T00:00:00Z",
        sort="asc",
        include_content=True,
        exclude_contentless=True,
        max_items=2,
    )

    assert [article["id"] for article in articles] == [1, 2]
    assert articles[0]["content"] == "Full article body for sentiment analysis."
    assert articles[0]["symbols"] == ["AAPL"]
    queries = HistoricalDataHandler.queries["/v1beta1/news"]
    assert queries[0]["limit"] == ["2"]
    assert queries[0]["include_content"] == ["true"]
    assert queries[1]["limit"] == ["1"]
    assert queries[1]["page_token"] == ["news-2"]


@pytest.mark.asyncio
async def test_corporate_actions_paginate_and_normalize_event_types(
    historical_data_server: str,
) -> None:
    """Expose exact structured events for adjustment checks and event studies."""
    client = AlpacaHistoricalDataClient(
        api_key="test-key",
        api_secret="test-secret",
        base_url=historical_data_server,
    )

    actions = await client.get_corporate_actions(
        symbols="AAPL",
        action_types="cash_dividend,forward_split",
        start="2026-01-01",
        end="2026-01-31",
        region="us",
        data_quality="complete",
        max_items=2,
    )

    assert [action["type"] for action in actions] == ["cash_dividend", "forward_split"]
    assert actions[0]["rate"] == "0.2400000001"
    queries = HistoricalDataHandler.queries["/v1/corporate-actions"]
    assert queries[0]["types"] == ["cash_dividend,forward_split"]
    assert queries[0]["limit"] == ["2"]
    assert queries[1]["limit"] == ["1"]
    assert queries[1]["page_token"] == ["actions-2"]


def test_historical_validation_fails_before_network() -> None:
    """Reject invalid ranges, feeds, symbols, limits, and timeframes synchronously."""
    client = AlpacaHistoricalDataClient(api_key="test-key", api_secret="test-secret")
    bounds = ("2026-01-02T14:30:00Z", "2026-01-02T15:00:00Z")

    with pytest.raises(ValueError, match="one non-empty equity symbol"):
        client.get_trades("AAPL,MSFT", *bounds)
    with pytest.raises(ValueError, match="one non-empty equity symbol"):
        client.get_snapshot("AAPL/USD")
    with pytest.raises(ValueError, match="at least one equity symbol"):
        client.get_snapshots([])
    with pytest.raises(ValueError, match="must not contain duplicates"):
        client.get_snapshots(["AAPL", "AAPL"])
    with pytest.raises(ValueError, match="at least one equity symbol"):
        client.get_bars_multi([], "1Min", *bounds)
    with pytest.raises(ValueError, match="must not contain duplicates"):
        client.get_quotes_multi(["AAPL", "AAPL"], *bounds)
    with pytest.raises(ValueError, match="Invalid Alpaca historical asof date"):
        client.get_trades("AAPL", *bounds, asof="today")
    with pytest.raises(ValueError, match="at least one equity symbol"):
        client.get_auctions([], *bounds)
    with pytest.raises(ValueError, match="ranking must be"):
        client.get_most_actives(by="notional")
    with pytest.raises(ValueError, match="top must be between"):
        client.get_most_actives(top=0)
    with pytest.raises(ValueError, match="top must be between"):
        client.get_market_movers(top=51)
    with pytest.raises(ValueError, match="tick_type must be"):
        client.get_condition_codes("auction", "C")
    with pytest.raises(ValueError, match="tape must be"):
        client.get_condition_codes("trade", "D")
    with pytest.raises(ValueError, match="does not support historical ranges"):
        client.get_quotes("AAPL", *bounds, feed=AlpacaDataFeed.OVERNIGHT)
    with pytest.raises(ValueError, match="max_items must be between"):
        client.get_trades("AAPL", *bounds, max_items=0)
    with pytest.raises(ValueError, match="Invalid Alpaca timeframe"):
        client.get_bars("AAPL", "minute", *bounds)
    for timeframe in ("0Min", "60Min", "24Hour", "2Day", "2Week", "5Month", "abcDay"):
        with pytest.raises(ValueError, match="Invalid Alpaca timeframe"):
            client.get_bars("AAPL", timeframe, *bounds)
    with pytest.raises(ValueError, match="start must be earlier"):
        client.get_trades("AAPL", bounds[1], bounds[0])
    with pytest.raises(ValueError, match="news sort must be"):
        client.get_news(sort="newest")
    with pytest.raises(ValueError, match="news symbols must be"):
        client.get_news(symbols="AAPL, MSFT")
    with pytest.raises(ValueError, match="news max_items must be"):
        client.get_news(max_items=0)
    with pytest.raises(ValueError, match="Unsupported Alpaca corporate-action type"):
        client.get_corporate_actions(action_types="surprise")
    with pytest.raises(ValueError, match="region must be"):
        client.get_corporate_actions(region="eu")
    with pytest.raises(ValueError, match="Invalid Alpaca corporate-action start"):
        client.get_corporate_actions(start="January 1")
    with pytest.raises(ValueError, match="IDs cannot be combined"):
        client.get_corporate_actions(ids="action-1", symbols="AAPL")
    with pytest.raises(ValueError, match="option feed must be"):
        client.get_option_chain("AAPL", feed="sip")
    with pytest.raises(ValueError, match="option type must be"):
        client.get_option_chain("AAPL", option_type="both")
    with pytest.raises(ValueError, match="option-snapshot max_items"):
        client.get_option_snapshots(["AAPL260116C00200000"], max_items=101)
    with pytest.raises(ValueError, match="updated_since"):
        client.get_option_snapshots(["AAPL260116C00200000"], updated_since="yesterday")
    with pytest.raises(ValueError, match="minimum strike must not exceed"):
        client.get_option_chain("AAPL", strike_price_gte="210", strike_price_lte="200")
    with pytest.raises(ValueError, match="between 1 and 100 contract symbols"):
        client.get_option_bars([], "1Min", *bounds)
    with pytest.raises(ValueError, match="uppercase ASCII"):
        client.get_option_trades(["aapl260116c00200000"], *bounds)
    with pytest.raises(ValueError, match="must not contain duplicates"):
        client.get_option_trades(["AAPL260116C00200000"] * 2, *bounds)
