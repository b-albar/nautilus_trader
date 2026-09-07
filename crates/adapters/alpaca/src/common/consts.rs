// -------------------------------------------------------------------------------------------------
//  Copyright (C) 2015-2026 Nautech Systems Pty Ltd. All rights reserved.
//  https://nautechsystems.io
//
//  Licensed under the GNU Lesser General Public License Version 3.0 (the "License");
// -------------------------------------------------------------------------------------------------

use std::sync::LazyLock;

use nautilus_model::identifiers::{ClientId, Venue};

pub const ALPACA: &str = "ALPACA";
pub static ALPACA_CLIENT_ID: LazyLock<ClientId> = LazyLock::new(|| ClientId::new(ALPACA));
pub static ALPACA_VENUE: LazyLock<Venue> = LazyLock::new(|| Venue::new(ALPACA));
pub const ALPACA_DATA_HTTP_URL: &str = "https://data.alpaca.markets";
pub const ALPACA_DATA_WS_URL: &str = "wss://stream.data.alpaca.markets";
pub const ALPACA_DATA_SANDBOX_WS_URL: &str = "wss://stream.data.sandbox.alpaca.markets";
pub const ALPACA_LIVE_TRADING_HTTP_URL: &str = "https://api.alpaca.markets";
pub const ALPACA_PAPER_TRADING_HTTP_URL: &str = "https://paper-api.alpaca.markets";
pub const ALPACA_API_KEY_HEADER: &str = "APCA-API-KEY-ID";
pub const ALPACA_API_SECRET_HEADER: &str = "APCA-API-SECRET-KEY";
