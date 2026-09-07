#!/usr/bin/env python3
# -------------------------------------------------------------------------------------------------
#  Copyright (C) 2015-2026 Nautech Systems Pty Ltd. All rights reserved.
#  https://nautechsystems.io
#
#  Licensed under the GNU Lesser General Public License Version 3.0 (the "License");
# -------------------------------------------------------------------------------------------------
"""Run a Python EMA-cross strategy against Alpaca paper trading."""

from __future__ import annotations

from decimal import Decimal
from typing import Self

from nautilus_trader.adapters.alpaca import (
    ALPACA,
    AlpacaDataClientConfig,
    AlpacaDataClientFactory,
    AlpacaDataFeed,
    AlpacaEnvironment,
    AlpacaExecutionClientConfig,
    AlpacaExecutionClientFactory,
)
from nautilus_trader.common import Environment
from nautilus_trader.config import StrategyConfig
from nautilus_trader.indicators import ExponentialMovingAverage
from nautilus_trader.live import LiveNode
from nautilus_trader.model import (
    AccountId,
    AccountType,
    Bar,
    BarType,
    InstrumentId,
    OrderSide,
    StrategyId,
    TimeInForce,
    TraderId,
)
from nautilus_trader.trading import Strategy


class AlpacaEmaCrossConfig(StrategyConfig):
    """Configuration for the example Python strategy."""

    _CUSTOM_FIELDS = (
        "instrument_id",
        "bar_type",
        "quantity",
        "fast_period",
        "slow_period",
        "dry_run",
    )

    def __new__(cls, *args: object, **kwargs: object) -> Self:
        for field in cls._CUSTOM_FIELDS:
            kwargs.pop(field, None)
        return super().__new__(cls, *args, **kwargs)

    def __init__(
        self,
        instrument_id: InstrumentId,
        bar_type: BarType,
        quantity: Decimal,
        fast_period: int = 10,
        slow_period: int = 30,
        dry_run: bool = True,
        **_kwargs: object,
    ) -> None:
        super().__init__()
        if quantity <= 0:
            raise ValueError("quantity must be positive")
        if not 1 <= fast_period < slow_period:
            raise ValueError("periods must satisfy 1 <= fast_period < slow_period")
        self.instrument_id = instrument_id
        self.bar_type = bar_type
        self.quantity = quantity
        self.fast_period = fast_period
        self.slow_period = slow_period
        self.dry_run = dry_run


class AlpacaEmaCross(Strategy):
    """Long-only EMA cross, disabled from submitting orders by default."""

    def __init__(self, config: AlpacaEmaCrossConfig) -> None:
        super().__init__(config)
        self._fast = ExponentialMovingAverage(config.fast_period)
        self._slow = ExponentialMovingAverage(config.slow_period)
        self._was_bullish: bool | None = None

    def on_start(self) -> None:
        if self.cache.instrument(self.config.instrument_id) is None:
            self.log.error(f"Instrument not found: {self.config.instrument_id}")
            self.stop()
            return
        self.register_indicator_for_bars(self.config.bar_type, self._fast)
        self.register_indicator_for_bars(self.config.bar_type, self._slow)
        self.subscribe_bars(self.config.bar_type)

    def on_bar(self, _bar: Bar) -> None:
        if not self.indicators_initialized():
            return
        bullish = self._fast.value > self._slow.value
        if bullish == self._was_bullish:
            return
        self._was_bullish = bullish
        if self.config.dry_run:
            self.log.warning(f"DRY RUN: EMA signal bullish={bullish}")
            return
        if bullish and self.portfolio.is_net_flat(self.config.instrument_id):
            self._submit_buy()
        elif not bullish and self.portfolio.is_net_long(self.config.instrument_id):
            self.close_all_positions(self.config.instrument_id)

    def on_stop(self) -> None:
        self.cancel_all_orders(self.config.instrument_id)
        if not self.config.dry_run:
            self.close_all_positions(self.config.instrument_id)
        self.unsubscribe_bars(self.config.bar_type)

    def _submit_buy(self) -> None:
        instrument = self.cache.instrument(self.config.instrument_id)
        if instrument is None:
            return
        order = self.order_factory.market(
            instrument_id=self.config.instrument_id,
            order_side=OrderSide.BUY,
            quantity=instrument.make_qty(self.config.quantity),
            time_in_force=TimeInForce.DAY,
        )
        self.submit_order(order)


TRADER_ID = TraderId.from_str("ALPACA-PAPER-001")
ACCOUNT_ID = AccountId.from_str("ALPACA-001")
INSTRUMENT_ID = InstrumentId.from_str(f"AAPL.{ALPACA}")
BAR_TYPE = BarType.from_str(f"{INSTRUMENT_ID}-1-MINUTE-LAST-EXTERNAL")
DRY_RUN = True


def main() -> None:
    """Run the strategy with paper execution and environment-based credentials."""
    node = (
        LiveNode.builder("ALPACA-PAPER-EMA-001", TRADER_ID, Environment.LIVE)
        .with_reconciliation(reconciliation=True)
        .add_data_client(
            None,
            AlpacaDataClientFactory(),
            AlpacaDataClientConfig(
                instrument_ids=[INSTRUMENT_ID],
                feed=AlpacaDataFeed.IEX,
                trading_environment=AlpacaEnvironment.PAPER,
            ),
        )
        .add_exec_client(
            None,
            AlpacaExecutionClientFactory(),
            AlpacaExecutionClientConfig(
                account_id=ACCOUNT_ID,
                environment=AlpacaEnvironment.PAPER,
                account_type=AccountType.MARGIN,
            ),
        )
        .build()
    )
    node.add_strategy(
        AlpacaEmaCross(
            AlpacaEmaCrossConfig(
                strategy_id=StrategyId.from_str("ALPACA-EMA-001"),
                instrument_id=INSTRUMENT_ID,
                bar_type=BAR_TYPE,
                quantity=Decimal(1),
                dry_run=DRY_RUN,
            ),
        ),
    )
    node.run()


if __name__ == "__main__":
    main()
