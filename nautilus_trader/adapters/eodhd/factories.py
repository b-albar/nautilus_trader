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
EODHD adapter factories.
"""

import asyncio
import os
from functools import lru_cache

from nautilus_trader.adapters.eodhd.config import EodhdDataClientConfig
from nautilus_trader.adapters.eodhd.constants import EODHD_BASE_URL_HTTP
from nautilus_trader.adapters.eodhd.data import EodhdDataClient
from nautilus_trader.adapters.eodhd.http_client import EodhdHttpClient
from nautilus_trader.adapters.eodhd.providers import EodhdInstrumentProvider
from nautilus_trader.cache.cache import Cache
from nautilus_trader.common.component import LiveClock
from nautilus_trader.common.component import MessageBus
from nautilus_trader.config import InstrumentProviderConfig
from nautilus_trader.live.factories import LiveDataClientFactory


@lru_cache(1)
def get_eodhd_http_client(
    api_key: str | None = None,
    base_url: str | None = None,
    timeout_secs: int = 60,
) -> EodhdHttpClient:
    """
    Cache and return an EODHD HTTP client with the given API key.

    If a cached client with matching key already exists, that cached
    client will be returned.

    Parameters
    ----------
    api_key : str, optional
        The EODHD API key.
        If ``None`` then will source the `EODHD_API_KEY` environment variable.
    base_url : str, optional
        The base URL for the API endpoints.
    timeout_secs : int, default 60
        The timeout (seconds) for HTTP requests.

    Returns
    -------
    EodhdHttpClient

    """
    resolved_api_key = api_key or os.environ.get("EODHD_API_KEY", "")

    return EodhdHttpClient(
        api_key=resolved_api_key,
        base_url=base_url or EODHD_BASE_URL_HTTP,
        timeout_secs=timeout_secs,
    )


@lru_cache(1)
def get_eodhd_instrument_provider(
    client: EodhdHttpClient,
    config: InstrumentProviderConfig,
) -> EodhdInstrumentProvider:
    """
    Cache and return an EODHD instrument provider.

    If a cached provider already exists, that provider will be returned.

    Parameters
    ----------
    client : EodhdHttpClient
        The HTTP client for the instrument provider.
    config : InstrumentProviderConfig
        The configuration for the instrument provider.

    Returns
    -------
    EodhdInstrumentProvider

    """
    return EodhdInstrumentProvider(
        client=client,
        config=config,
    )


class EodhdLiveDataClientFactory(LiveDataClientFactory):
    """
    Provides an EODHD live data client factory.
    """

    @staticmethod
    def create(  # type: ignore
        loop: asyncio.AbstractEventLoop,
        name: str,
        config: EodhdDataClientConfig,
        msgbus: MessageBus,
        cache: Cache,
        clock: LiveClock,
    ) -> EodhdDataClient:
        """
        Create a new EODHD data client.

        Parameters
        ----------
        loop : asyncio.AbstractEventLoop
            The event loop for the client.
        name : str
            The custom client ID.
        config : EodhdDataClientConfig
            The client configuration.
        msgbus : MessageBus
            The message bus for the client.
        cache : Cache
            The cache for the client.
        clock : LiveClock
            The clock for the instrument provider.

        Returns
        -------
        EodhdDataClient

        """
        http_client = get_eodhd_http_client(
            api_key=config.api_key,
            base_url=config.base_url_http,
        )

        provider = get_eodhd_instrument_provider(
            client=http_client,
            config=config.instrument_provider,
        )

        return EodhdDataClient(
            loop=loop,
            http_client=http_client,
            msgbus=msgbus,
            cache=cache,
            clock=clock,
            instrument_provider=provider,
            config=config,
            name=name,
        )
