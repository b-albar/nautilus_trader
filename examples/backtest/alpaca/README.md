# Alpaca Backtest Examples

This directory contains backtest examples demonstrating how to use NautilusTrader
with historical data from Alpaca's API.

## Examples

### 1. `alpaca_ema_cross_aapl.py` (Bar Data)

A backtest using **daily bar data** with the EMA crossover strategy:

- **Data Type**: Daily OHLCV bars
- **Strategy**: EMACrossLongOnly (5/10 period EMA)
- **Data Loader**: `AlpacaDataLoader.load_bars()`
- **Instrument**: AAPL

Best for: Testing strategies that operate on bar aggregations.

### 2. `alpaca_tick_ema_cross.py` (Trade Tick Data)

A backtest using **trade tick data** with a tick-based EMA strategy:

- **Data Type**: Individual trade executions
- **Strategy**: TickEMACross (50/200 tick EMA)
- **Data Loader**: `AlpacaDataLoader.load_trades()`
- **Instrument**: AAPL

Best for: High-frequency strategies, market microstructure analysis.

### 3. `alpaca_quote_ema_cross.py` (Quote Tick Data)

A backtest using **quote tick data (NBBO)** with a quote-based EMA strategy:

- **Data Type**: National Best Bid and Offer quotes
- **Strategy**: QuoteEMACross (50/200 quote EMA on mid-price)
- **Data Loader**: `AlpacaDataLoader.load_quotes()`
- **Instrument**: AAPL

Best for: Spread analysis, market making strategies, bid-ask dynamics.

## Prerequisites

1. **Alpaca Account** (Free)
   - Sign up at [https://alpaca.markets/](https://alpaca.markets/)
   - Get your API keys from Paper Trading -> API Keys

## Setup

1. Copy the environment template:
   ```bash
   cp .env.example .env
   ```

2. Edit `.env` and add your API keys:
   ```bash
   ALPACA_API_KEY=your_api_key
   ALPACA_API_SECRET=your_api_secret
   ```

3. Run an example:
   ```bash
   # Bar data backtest
   python alpaca_ema_cross_aapl.py

   # Trade tick backtest
   python alpaca_tick_ema_cross.py

   # Quote tick backtest
   python alpaca_quote_ema_cross.py
   ```

## Data Types Comparison

| Data Type | Method | Granularity | Size (1 hour) | Best For |
|-----------|--------|-------------|---------------|----------|
| Bars | `load_bars()` | Aggregated OHLCV | ~60 bars (1min) | Traditional strategies |
| Trade Ticks | `load_trades()` | Individual trades | ~10K-50K ticks | HFT, tick analysis |
| Quote Ticks | `load_quotes()` | Bid/Ask updates | ~100K-500K quotes | Spread, market making |

## Available Bar Timeframes

The `AlpacaBarTimeframe` enum supports:

- `MINUTE_1`, `MINUTE_5`, `MINUTE_15`, `MINUTE_30`
- `HOUR_1`, `HOUR_2`, `HOUR_4`
- `DAY_1`, `WEEK_1`, `MONTH_1`, `MONTH_2`, `MONTH_3`, `MONTH_6`, `MONTH_12`

## Notes

### Tick Data Volume

Tick data for active stocks like AAPL can be extremely large:
- ~50,000+ trades per hour
- ~500,000+ quotes per hour

The examples use short time windows (30-60 minutes) to keep data size manageable.

### Strategy Disclaimer

These strategies have **NO ALPHA ADVANTAGE** and are purely for demonstration purposes.
Do not use for live trading without thorough backtesting and risk management.

## Troubleshooting

### "No data loaded" error

- Verify your API keys are set correctly
- Check that the time range falls within market hours (9:30 AM - 4:00 PM ET)
- Ensure it's a trading day (not weekend/holiday)

### Memory issues with tick data

- Reduce the time window
- Use bar data instead for longer periods
- Consider streaming data instead of loading all at once

## License

These examples are provided under the same license as NautilusTrader.
