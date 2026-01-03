# Live Trading Examples: Alpaca + EODHD

This directory contains live trading examples demonstrating how to use NautilusTrader
with Alpaca (paper or live) and EODHD market data.

## Examples

### 1. `alpaca_ema_cross_aapl.py` (Recommended Starting Point)

A simple live trading example using **Alpaca only** for both data and execution:

- **Data Source**: Alpaca IEX feed (free) or SIP (paid subscription)
- **Execution**: Alpaca Paper or Live Trading
- **Strategy**: EMA Cross Long Only (5/10 period)
- **Instrument**: AAPL

This is the simplest setup and recommended for getting started with live trading.

### 2. `eodhd_alpaca_ema_cross.py` (Advanced - Dual Data Source)

A more advanced example using **two data providers**:

- **Data Source**: EODHD real-time WebSocket streaming
- **Execution**: Alpaca Paper or Live Trading
- **Strategy**: Custom EMA Cross strategy with venue mapping
- **Instrument**: AAPL (mapped between EODHD US venue and Alpaca venue)

This example demonstrates how to use different data and execution providers.

## Prerequisites

1. **Alpaca Account** (Required)
   - Sign up at [https://alpaca.markets/](https://alpaca.markets/)
   - For paper trading: Navigate to "Paper Trading" -> "API Keys"
   - For live trading: Navigate to "Live Trading" -> "API Keys"

2. **EODHD Account** (Required only for `eodhd_alpaca_ema_cross.py`)
   - Sign up at [https://eodhd.com/](https://eodhd.com/)
   - Get your API key from the dashboard

## Setup

1. Copy the environment template:
   ```bash
   cp .env.example .env
   ```

2. Edit `.env` and configure your settings:

   **For Paper Trading (Recommended for testing):**
   ```bash
   ALPACA_API_KEY=your_paper_api_key
   ALPACA_API_SECRET=your_paper_api_secret
   ALPACA_ENDPOINT=https://paper-api.alpaca.markets/v2
   ALPACA_PAPER=true
   ALPACA_DATA_FEED=IEX
   EODHD_API_KEY=your_eodhd_api_key  # Optional
   ```

   **For Live Trading (Use with caution!):**
   ```bash
   ALPACA_API_KEY=your_live_api_key
   ALPACA_API_SECRET=your_live_api_secret
   ALPACA_ENDPOINT=https://api.alpaca.markets/v2
   ALPACA_PAPER=false
   ALPACA_DATA_FEED=SIP  # Recommended for live
   EODHD_API_KEY=your_eodhd_api_key  # Optional
   ```

3. Run an example:
   ```bash
   # Simple Alpaca-only example
   python alpaca_ema_cross_aapl.py

   # Advanced dual-source example
   python eodhd_alpaca_ema_cross.py
   ```

## Environment Variables

| Variable | Description | Default |
|----------|-------------|---------|
| `ALPACA_API_KEY` | Alpaca API key | Required |
| `ALPACA_API_SECRET` | Alpaca API secret | Required |
| `ALPACA_ENDPOINT` | API endpoint URL | Auto (based on paper flag) |
| `ALPACA_PAPER` | Paper trading mode (`true`/`false`) | `true` |
| `ALPACA_DATA_FEED` | Data feed (`IEX`/`SIP`) | `IEX` |
| `EODHD_API_KEY` | EODHD API key | Required for dual-source |

## Data Feed Notes

### Alpaca Data Feeds

- **IEX (free)**: 15-minute delayed for most symbols, real-time for IEX-listed
- **SIP (paid)**: Full real-time NBBO data from all US exchanges

### EODHD Data

- Real-time WebSocket streaming for US equities, FOREX, and crypto
- Historical EOD and intraday data via HTTP API
- Requires EODHD API subscription for real-time data

## Important Notes

### ⚠️ PAPER vs LIVE TRADING

- **Paper Trading** (`ALPACA_PAPER=true`): Uses virtual money, safe for testing
- **Live Trading** (`ALPACA_PAPER=false`): Uses REAL money!

The examples include a 5-second safety delay when live trading mode is detected.

### Strategy Disclaimer

These strategies have **NO ALPHA ADVANTAGE** and are purely for demonstration purposes.
Do not use for live trading with real money without thorough backtesting and risk management.

## Strategy Configuration

Both examples use the EMA Cross strategy with the following default settings:

| Parameter | Value | Description |
|-----------|-------|-------------|
| Fast EMA | 5 | Fast EMA period (bars) |
| Slow EMA | 10 | Slow EMA period (bars) |
| Trade Size | 1 | Shares per trade |
| Bar Type | 1-MINUTE | Bar aggregation for signals |

## Troubleshooting

### "Cannot find instrument" error

- Ensure your Alpaca API keys are set correctly
- Check that the market is open (instruments only available during market hours)
- Try using `load_all=True` in the instrument provider config for debugging

### "WebSocket connection failed" error

- Verify your API keys are valid
- Check your internet connection
- Ensure you're using the correct endpoint for your trading mode

### "Authentication failed" error

- Double-check that `ALPACA_ENDPOINT` matches your API key type:
  - Paper keys → `https://paper-api.alpaca.markets/v2`
  - Live keys → `https://api.alpaca.markets/v2`

### No signals generated

- The EMA indicators need time to warm up
- Wait for enough bars (at least `slow_ema_period` bars)
- Check the logs for indicator values

## License

These examples are provided under the same license as NautilusTrader.
