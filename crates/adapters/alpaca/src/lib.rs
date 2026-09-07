// -------------------------------------------------------------------------------------------------
//  Copyright (C) 2015-2026 Nautech Systems Pty Ltd. All rights reserved.
//  https://nautechsystems.io
//
//  Licensed under the GNU Lesser General Public License Version 3.0 (the "License");
//  You may not use this file except in compliance with the License.
//  You may obtain a copy of the License at https://www.gnu.org/licenses/lgpl-3.0.en.html
// -------------------------------------------------------------------------------------------------

//! Native Alpaca adapter for NautilusTrader.

#![warn(rustc::all)]
#![deny(unsafe_code)]
#![deny(nonstandard_style)]
#![deny(missing_debug_implementations)]
#![deny(clippy::missing_panics_doc)]
#![deny(rustdoc::broken_intra_doc_links)]

pub mod common;
pub mod config;
pub mod data;
pub mod execution;
pub mod factories;
pub mod http;
pub mod provider;
#[cfg(feature = "python")]
pub mod python;
pub mod websocket;

pub use common::{
    credential::AlpacaCredential, enums::AlpacaBarAdjustment, enums::AlpacaDataEnvironment,
    enums::AlpacaDataFeed, enums::AlpacaEnvironment, urls::trading_ws_url,
};
pub use config::{AlpacaDataClientConfig, AlpacaExecutionClientConfig};
pub use data::AlpacaDataClient;
pub use execution::AlpacaExecutionClient;
pub use factories::{AlpacaDataClientFactory, AlpacaExecutionClientFactory};
pub use http::{
    client::AlpacaRawHttpClient, models::AlpacaAccount, models::AlpacaAsset,
    models::AlpacaAssetClass, models::AlpacaAssetStatus, models::AlpacaBar,
    models::AlpacaBarsResponse, models::AlpacaNonTradeActivity, models::AlpacaOrder,
    models::AlpacaOrderRequest, models::AlpacaOrderSide, models::AlpacaOrderStatus,
    models::AlpacaOrderType, models::AlpacaPosition, models::AlpacaSingleSymbolBarsResponse,
    models::AlpacaTimeInForce, models::AlpacaTradeActivity, query::AlpacaActivitiesQuery,
    query::AlpacaAssetsQuery, query::AlpacaBarsQuery, query::AlpacaOrdersQuery,
};
pub use provider::AlpacaInstrumentProvider;
pub use websocket::{
    AlpacaLiveMessage, AlpacaTradeEvent, AlpacaTradeUpdate, AlpacaTradingMessage,
    AlpacaTradingWebSocketClient, AlpacaWebSocketClient,
};
