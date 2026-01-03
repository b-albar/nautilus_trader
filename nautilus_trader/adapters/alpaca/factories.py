# -------------------------------------------------------------------------------------------------
#  Copyright (C) 2015-2025 Nautech Systems Pty Ltd. All rights reserved.
#  https://nautechsystems.io
#
#  Licensed under the GNU Lesser General Public License Version 3.0 (the "License");
#  You may not use this file except in compliance with the License.
#  You may obtain a copy of the License at https://www.gnu.org/licenses/lgpl-3.0.en.html
#
#  Unless required by applicable law or agreed to in writing, software
#  distributed under the License is distributed on an "AS IS" BASIS,
#  WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
#  See the License for the specific language governing permissions and
#  limitations under the License.
# -------------------------------------------------------------------------------------------------
"""
Alpaca adapter factory classes for creating live clients.
"""

from __future__ import annotations

import asyncio
from functools import lru_cache
from typing import TYPE_CHECKING

from nautilus_trader.adapters.alpaca.config import AlpacaDataClientConfig
from nautilus_trader.adapters.alpaca.config import AlpacaExecClientConfig
from nautilus_trader.adapters.alpaca.data import AlpacaDataClient
from nautilus_trader.adapters.alpaca.execution import AlpacaExecutionClient
from nautilus_trader.adapters.alpaca.http import AlpacaHttpClient
from nautilus_trader.adapters.alpaca.providers import AlpacaInstrumentProvider
from nautilus_trader.cache.cache import Cache
from nautilus_trader.common.component import LiveClock
from nautilus_trader.common.component import MessageBus
from nautilus_trader.config import InstrumentProviderConfig
from nautilus_trader.live.factories import LiveDataClientFactory
from nautilus_trader.live.factories import LiveExecClientFactory


if TYPE_CHECKING:
    from typing import Any


@lru_cache(1)
def get_cached_alpaca_http_client(
    api_key: str | None = None,
    api_secret: str | None = None,
    paper: bool = True,
    base_url_http: str | None = None,
    base_url_data: str | None = None,
    timeout_secs: int = 10,
) -> AlpacaHttpClient:
    """
    Cache and return an Alpaca HTTP client with the given parameters.

    If a cached client with matching parameters already exists, the cached client will be returned.

    Parameters
    ----------
    api_key : str, optional
        The Alpaca API public key.
        If ``None`` then will source from environment variables.
    api_secret : str, optional
        The Alpaca API secret key.
        If ``None`` then will source from environment variables.
    paper : bool, default True
        If True, uses paper trading endpoints.
    base_url_http : str, optional
        The base URL for the trading API endpoints.
    base_url_data : str, optional
        The base URL for the market data API endpoints.
    timeout_secs : int, default 10
        The timeout (seconds) for HTTP requests.

    Returns
    -------
    AlpacaHttpClient
        The Alpaca HTTP client instance.

    """
    return AlpacaHttpClient(
        api_key=api_key,
        api_secret=api_secret,
        paper=paper,
        base_url_http=base_url_http,
        base_url_data=base_url_data,
        timeout_secs=timeout_secs,
    )


@lru_cache(1)
def get_cached_alpaca_instrument_provider(
    client: AlpacaHttpClient,
    config: InstrumentProviderConfig | None = None,
) -> AlpacaInstrumentProvider:
    """
    Cache and return an Alpaca instrument provider.

    If a cached provider already exists, then that provider will be returned.

    Parameters
    ----------
    client : AlpacaHttpClient
        The Alpaca HTTP client.
    config : InstrumentProviderConfig, optional
        The instrument provider configuration, by default None.

    Returns
    -------
    AlpacaInstrumentProvider
        The Alpaca instrument provider instance.

    """
    return AlpacaInstrumentProvider(
        client=client,
        config=config,
    )


class AlpacaLiveDataClientFactory(LiveDataClientFactory):
    """
    Provides an Alpaca live data client factory.

    Responsible for creating and configuring AlpacaDataClient instances
    for live trading and backtesting with live data feeds.
    """

    @staticmethod
    def create(  # type: ignore
        loop: asyncio.AbstractEventLoop,
        name: str,
        config: AlpacaDataClientConfig,
        msgbus: MessageBus,
        cache: Cache,
        clock: LiveClock,
    ) -> AlpacaDataClient:
        """
        Create a new Alpaca data client.

        Parameters
        ----------
        loop : asyncio.AbstractEventLoop
            The event loop for the client.
        name : str
            The custom client ID.
        config : AlpacaDataClientConfig
            The client configuration.
        msgbus : MessageBus
            The message bus for the client.
        cache : Cache
            The cache for the client.
        clock : LiveClock
            The clock for the client.

        Returns
        -------
        AlpacaDataClient
            The configured Alpaca data client.

        """
        client = get_cached_alpaca_http_client(
            api_key=config.api_key,
            api_secret=config.api_secret,
            paper=config.paper,
            base_url_http=config.base_url_http,
            base_url_data=config.base_url_data,
            timeout_secs=config.http_timeout_secs,
        )

        provider = get_cached_alpaca_instrument_provider(
            client=client,
            config=config.instrument_provider,
        )

        return AlpacaDataClient(
            loop=loop,
            client=client,
            msgbus=msgbus,
            cache=cache,
            clock=clock,
            instrument_provider=provider,
            config=config,
            name=name,
        )


class AlpacaLiveExecClientFactory(LiveExecClientFactory):
    """
    Provides an Alpaca live execution client factory.

    Responsible for creating and configuring AlpacaExecutionClient instances
    for live trading.
    """

    @staticmethod
    def create(  # type: ignore
        loop: asyncio.AbstractEventLoop,
        name: str,
        config: AlpacaExecClientConfig,
        msgbus: MessageBus,
        cache: Cache,
        clock: LiveClock,
    ) -> AlpacaExecutionClient:
        """
        Create a new Alpaca execution client.

        Parameters
        ----------
        loop : asyncio.AbstractEventLoop
            The event loop for the client.
        name : str
            The custom client ID.
        config : AlpacaExecClientConfig
            The client configuration.
        msgbus : MessageBus
            The message bus for the client.
        cache : Cache
            The cache for the client.
        clock : LiveClock
            The clock for the client.

        Returns
        -------
        AlpacaExecutionClient
            The configured Alpaca execution client.

        """
        client = get_cached_alpaca_http_client(
            api_key=config.api_key,
            api_secret=config.api_secret,
            paper=config.paper,
            base_url_http=config.base_url_http,
            timeout_secs=config.http_timeout_secs,
        )

        provider = get_cached_alpaca_instrument_provider(
            client=client,
            config=config.instrument_provider,
        )

        return AlpacaExecutionClient(
            loop=loop,
            client=client,
            msgbus=msgbus,
            cache=cache,
            clock=clock,
            instrument_provider=provider,
            config=config,
            name=name,
        )


def create_alpaca_clients(
    loop: asyncio.AbstractEventLoop,
    msgbus: MessageBus,
    cache: Cache,
    clock: LiveClock,
    data_config: AlpacaDataClientConfig | None = None,
    exec_config: AlpacaExecClientConfig | None = None,
    name: str | None = None,
) -> tuple[AlpacaDataClient | None, AlpacaExecutionClient | None]:
    """
    Create both Alpaca data and execution clients with shared HTTP client.

    This is a convenience function for creating both clients together
    with optimal resource sharing.

    Parameters
    ----------
    loop : asyncio.AbstractEventLoop
        The event loop for the clients.
    msgbus : MessageBus
        The message bus for the clients.
    cache : Cache
        The cache for the clients.
    clock : LiveClock
        The clock for the clients.
    data_config : AlpacaDataClientConfig, optional
        The data client configuration.
    exec_config : AlpacaExecClientConfig, optional
        The execution client configuration.
    name : str, optional
        The custom client ID.

    Returns
    -------
    tuple[AlpacaDataClient | None, AlpacaExecutionClient | None]
        The data and execution clients (either may be None if not configured).

    """
    data_client = None
    exec_client = None

    if data_config:
        data_client = AlpacaLiveDataClientFactory.create(
            loop=loop,
            name=name or "ALPACA",
            config=data_config,
            msgbus=msgbus,
            cache=cache,
            clock=clock,
        )

    if exec_config:
        exec_client = AlpacaLiveExecClientFactory.create(
            loop=loop,
            name=name or "ALPACA",
            config=exec_config,
            msgbus=msgbus,
            cache=cache,
            clock=clock,
        )

    return data_client, exec_client
