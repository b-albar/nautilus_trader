# Alpaca adapter

Native Alpaca adapter for NautilusTrader.

## Capability matrix

| Capability | Status |
|---|---|
| US equity historical bars over HTTP | Native `DataClient` request with pagination |
| Raw, split-, dividend-, or fully-adjusted historical bars | Configurable globally or per request |
| US equity historical trades and quotes over HTTP | Paginated native tick responses |
| IEX, SIP, delayed SIP, BOATS, overnight, and OTC feed selection | Available |
| Streaming trades, quotes, and bars | Native `DataClient` subscriptions |
| Streaming halts, resumptions, pauses, quotations, and short-sale restrictions | Native `InstrumentStatus` subscriptions |
| Streaming LULD price bands | Wire-safe decoding; retained explicitly pending a core LULD data type |
| Streaming connection, authentication, reference-counted subscriptions, and reconnect replay | Available |
| Authenticated US-equity instrument provider with selective or full-universe loading | Available |
| Nautilus lifecycle, event routing, request correlation, and clean reconnect | Available |
| Exact account balances, buying power, exposure, and trading-risk metadata | Available |
| Non-blocking account refresh after order changes, fills, and stream recovery | Available |
| Paper/live account lookup and order submit/cancel protocol | Available |
| Nautilus `ExecutionClient` with account bootstrap and REST submit/cancel | Available |
| Atomic bracket, OCO, and OTO orders with native contingent legs | Available |
| Replace orders, batch cancel, and paginated symbol/side cancel-all | Available |
| Query-by-venue/client ID and external-order private-stream registration | Available |
| Private `trade_updates` stream, binary paper frames, and reconnect authentication | Available |
| Windowed order, paginated fill-activity, and position reconciliation reports | Available |
| Exact fills, regulatory fees, and non-trade account-activity ledgers | Native REST/Python client |
| Account, position, order, and US market schedule snapshots | Read-only native Python client |
| On-demand asset lookup and filtered US-equity discovery | Read-only native Python client |
| OCC option-contract discovery and exact contract terms | Read-only native Python client |
| Filtered option chains with trades, quotes, IV, and Greeks | Read-only native Python client |
| Selected-contract option snapshots for strategy books | Read-only native Python client |
| Multi-contract historical option bars and trades | Read-only native Python client |
| Direct single- or multi-symbol historical bars, trades, and quotes | Read-only native Python client |
| Single- and multi-symbol consolidated market snapshots | Read-only native Python client |
| SIP opening and closing auction history | Read-only native Python client |
| Most-active and top-mover US equity screens | Read-only native Python client |
| Human-readable stock exchange and condition-code metadata | Read-only native Python client |
| Historical news for event studies and language-model research | Read-only native Python client |
| Structured corporate actions for adjustment audits and event studies | Read-only native Python client |
| Python configuration, enums, type stubs, and live-node factories | Available |

Credentials resolve from explicit configuration or `ALPACA_API_KEY` and `ALPACA_API_SECRET`.
Paper trading is the default execution environment. Live trading requires explicitly selecting
`AlpacaEnvironment.LIVE`; the adapter never promotes a paper configuration to live implicitly.
When configured, `proxy_url` is applied consistently to both HTTP and WebSocket transports.

Runnable Python examples are available in `examples/live/alpaca`: `data_tester.py` exercises live
and historical market data without placing orders, while `ema_cross.py` demonstrates a complete
Python strategy and paper-trading node with order submission disabled by default.

Set `instrument_ids=[InstrumentId.from_str("AAPL.ALPACA")]` on
`AlpacaDataClientConfig` for focused startup. Omitting `instrument_ids` loads the full active Alpaca
US-equity universe, which is useful for discovery but unnecessarily expensive for most strategies.

Historical bars default to `AlpacaBarAdjustment.RAW`, making the backtest price basis explicit.
Set `bar_adjustment` on `AlpacaDataClientConfig`, or pass an `adjustment` request parameter with
`raw`, `split`, `dividend`, or `all` to override it for an individual research query.
Live and historical feeds can be selected independently with `feed` and `historical_feed`. This
supports delayed SIP and overnight streaming alongside IEX, SIP, BOATS, or OTC historical ranges,
while rejecting feed/endpoint combinations Alpaca does not provide.

Idempotent HTTP reads use bounded exponential backoff for transport errors, rate limits, and 5xx
responses, respecting Alpaca's `X-RateLimit-Reset` header (and standard `Retry-After`) within the
bounded retry budget. Authentication, validation, and decoding failures remain immediate. Trading writes
(submit, replace, and cancel) are deliberately never retried because a lost response does not prove
that Alpaca rejected the command. Ambiguous submissions are queried by their unique client order ID
and emitted as reconciliation reports when recovered; unresolved writes remain pending for the
private stream or later reconciliation instead of being falsely reported as rejected.
Set `http_max_retries=0` for latency-sensitive or deterministic test environments; the default is
three retries for both data and execution clients.

Python research and accounting workflows can query the non-trade ledger without loading a live
trading node:

```python
from nautilus_trader.adapters.alpaca import AlpacaAccountActivityClient

client = AlpacaAccountActivityClient()  # Uses ALPACA_API_KEY / ALPACA_API_SECRET
fees = await client.get_all_activities(
    "FEE",
    start="2026-01-01T00:00:00Z",
    end="2026-02-01T00:00:00Z",
)
fills = await client.get_fills(order_id="venue-order-id")
```

Decimal cash and quantity fields are returned as strings, preserving venue precision for P&L,
reporting, and assistant/tool workflows.

`AlpacaPortfolioClient` exposes read-only account, position, bounded paginated order-history,
market-clock, and calendar snapshots without any order-placement capability. It is suitable for
dashboards, deployment readiness checks, and narrowly permissioned assistant tools. Order history
includes native bracket/OCO/OTO legs by default and can be bounded by status, symbols, timestamps,
and a hard result limit. Individual orders can be retrieved by either Alpaca venue ID or a
strategy-supplied client order ID, allowing a UI or assistant to inspect one order without scanning
the ledger.
Its portfolio-history query returns aligned Unix timestamps, equity, absolute P&L, and percentage
P&L arrays for performance charts, with explicit period, resolution, session, and cash-flow filters.
Current account snapshots include Reg-T and non-marginable buying power, margin requirements,
accrued fees, pending transfers, options permissions, and crypto status when Alpaca supplies them.
The removed legacy PDT flag is optional, keeping account bootstrap compatible with Alpaca's
post-July-2026 schema.

`AlpacaReferenceDataClient` retrieves one instrument by ticker or Alpaca asset ID, or filters the
US-equity master by active state and exchange. It exposes venue-provided tradability, shortability,
fractionability, and any venue-provided increment metadata without requiring a running trading node.
Asset discovery can also filter Alpaca's current capability attributes, including options support,
overnight eligibility or halts, extended-hours fractional trading, IPO state, and PTP restrictions.
Current asset payloads expose `borrow_status`, CUSIP, and separate long and short margin
requirements. The native model also accepts Alpaca's deprecated `easy_to_borrow` and maintenance
margin fields during the venue's migration window, so mixed cached and live payloads remain usable.
It also reads account watchlists by list, ID, or user-visible name, giving research screens and
assistant tools one consistent source for saved universes without granting mutation permissions.
The same client retrieves individual OCC option contracts or bounded, automatically paginated
contract universes. Expiration, type, style, strike, Penny Program, and underlying filters are
validated before I/O; adjusted deliverables remain attached for correct non-standard-contract
analysis.

`AlpacaHistoricalDataClient` provides a compact async API for notebooks, dashboards, and assistant
tools. Bars, trades, and quotes are automatically cursor-paginated within an explicit hard bound;
timestamps use UTC strings and decimal values remain strings. Quote sizes are named
`bid_size_lots`/`ask_size_lots` to make Alpaca's round-lot wire unit explicit.
The `get_bars_multi`, `get_trades_multi`, and `get_quotes_multi` methods retain Alpaca's grouped
symbol response and apply one global result bound, enabling portfolio research without serial
requests or accidental per-symbol limit multiplication.
All single- and multi-symbol historical methods accept an `asof` date for entity-aware ticker
mapping across symbol changes. Passing `"-"` explicitly disables mapping when research requires
the literal historical ticker instead.
`get_auctions` returns grouped opening and closing SIP auction prints with nanosecond timestamps,
exchange and condition codes, and exact prices and sizes. This supports market-on-open and
market-on-close execution studies without approximating auction fills from minute bars.
`get_most_actives` ranks current US equities by SIP volume or trade count, while
`get_market_movers` returns split-adjusted gainers and losers. Both include Alpaca's nanosecond
calculation timestamp and preserve price and change values as exact decimal strings for downstream
research and assistant tools.
`get_exchange_codes` and `get_condition_codes` resolve compact exchange, trade-condition, and
quote-condition wire codes into venue descriptions for readable dashboards and assistant answers.
Its one-call `get_snapshot` view combines the latest trade, quote, minute bar, daily bar, and
previous daily bar for responsive instrument panels without polling five separate endpoints.
`get_snapshots` provides the same exact view for an entire research universe in one request, which
avoids serial HTTP calls in screeners, dashboards, and assistant tools.
Historical trades and quotes retain exchange codes, condition codes, and tape identifiers for
microstructure filtering and execution-quality analysis.
Historical news is cursor-paginated within a hard bound, supports symbol and time filters, and can
optionally include full article content for sentiment models or assistant summarization.
Corporate actions are likewise cursor-paginated and expose the current structured Alpaca event
types, including splits, dividends, mergers, reorganizations, partial calls, and capital-gains
distributions. Each returned record has a normalized singular `type`, while venue decimal values
remain exact strings for backtest adjustment audits.
`get_option_chain` retrieves a bounded, cursor-paginated chain for one underlying, with contract
type, strike, expiration, root, feed, and freshness filters. Latest trades and quotes, implied
volatility, and all five venue Greeks use exact decimal strings, making the response directly useful
for volatility surfaces, contract selection, dashboards, and LLM research tools. The free
indicative feed is the safe default; OPRA must be selected explicitly and requires venue entitlement.
`get_option_snapshots` provides the same exact market state for an explicit set of up to 100 OCC
symbols. This is the efficient path for strategy books, saved screens, and assistant follow-ups that
should not reload an underlying's complete chain; it also supports venue-side freshness filtering.
`get_option_bars` and `get_option_trades` retrieve ascending, symbol-grouped time series for up to
100 OCC contracts per query. Both apply a global hard result bound and follow Alpaca's symbol-first
cursor ordering, so a page containing only the first contract cannot silently truncate the rest of
the requested research universe.

Alpaca stock quote sizes are expressed in round lots. The domain parser converts these values to
share quantities before constructing Nautilus quote ticks.

Alpaca documents asset increment fields as crypto-only, and current equity payloads normally omit
them. The native provider therefore uses Alpaca's documented equity protocol resolutions: four
price decimals across the instrument domain and nine quantity decimals for fractionable shares;
explicit venue fields still take precedence when present. Non-fractionable assets retain the
standard one-share increment. Before either submit or replace reaches the network, execution
enforces the price-dependent venue rule of at most two decimals at or above $1 and four below $1,
as well as the nine-decimal fractional quantity limit. Historical data, live data, strategy sizing,
execution, and reconciliation therefore share an exact decimal model without making real asset
bootstrap depend on crypto-only response fields.

Protocol references:

- <https://docs.alpaca.markets/reference/stockbars-1>
- <https://docs.alpaca.markets/docs/real-time-stock-pricing-data>
- <https://docs.alpaca.markets/reference/corporateactions-1>
- <https://docs.alpaca.markets/reference/stocksnapshots-1>
- <https://docs.alpaca.markets/reference/stockauctions-1>
- <https://docs.alpaca.markets/reference/mostactives-1>
- <https://docs.alpaca.markets/reference/movers-1>
- <https://docs.alpaca.markets/reference/stockmetaexchanges-1>
- <https://docs.alpaca.markets/reference/stockmetaconditions-1>
- <https://docs.alpaca.markets/reference/get-options-contracts>
- <https://docs.alpaca.markets/reference/optionchain>
- <https://docs.alpaca.markets/reference/optionbars>
- <https://docs.alpaca.markets/reference/optiontrades>
- <https://docs.alpaca.markets/docs/trading/orders/>
- <https://docs.alpaca.markets/docs/fractional-trading>
