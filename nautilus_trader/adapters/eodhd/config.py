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
EODHD adapter configuration.
"""

from nautilus_trader.common.config import PositiveInt
from nautilus_trader.config import LiveDataClientConfig


class EodhdDataClientConfig(LiveDataClientConfig, frozen=True):
    """
    Configuration for ``EodhdDataClient`` instances.

    Parameters
    ----------
    api_key : str, optional
        The EODHD API key.
        If ``None`` then will source the `EODHD_API_KEY` environment variable.
    base_url_http : str, optional
        The base URL for the EODHD HTTP API.
        If ``None`` then will default to https://eodhd.com/api.
    base_url_ws : str, optional
        The base URL for the EODHD WebSocket API.
        If ``None`` then will default to wss://ws.eodhistoricaldata.com/ws.
    update_instruments_interval_mins : PositiveInt or None, default 60
        The interval (minutes) between reloading instruments from the venue.
    ws_connection_delay_secs : PositiveInt, default 2
        The delay (seconds) prior to main websocket connection to allow initial subscriptions to arrive.
    subscribe_trade_ticks : bool, default True
        If True, subscribe to trade tick data for US equities.
    subscribe_quote_ticks : bool, default True
        If True, subscribe to quote tick data for US equities and FOREX.

    References
    ----------
    EODHD API Documentation: https://eodhd.com/financial-apis/

    """

    api_key: str | None = None
    base_url_http: str | None = None
    base_url_ws: str | None = None
    update_instruments_interval_mins: PositiveInt | None = 60
    ws_connection_delay_secs: PositiveInt = 2
    subscribe_trade_ticks: bool = True
    subscribe_quote_ticks: bool = True
