// -------------------------------------------------------------------------------------------------
//  Copyright (C) 2015-2026 Nautech Systems Pty Ltd. All rights reserved.
//  https://nautechsystems.io
//
//  Licensed under the GNU Lesser General Public License Version 3.0 (the "License");
// -------------------------------------------------------------------------------------------------

use super::{
    consts::{
        ALPACA_DATA_HTTP_URL, ALPACA_DATA_SANDBOX_WS_URL, ALPACA_DATA_WS_URL,
        ALPACA_LIVE_TRADING_HTTP_URL, ALPACA_PAPER_TRADING_HTTP_URL,
    },
    enums::{AlpacaDataEnvironment, AlpacaDataFeed, AlpacaEnvironment},
};

#[must_use]
pub const fn data_http_url() -> &'static str {
    ALPACA_DATA_HTTP_URL
}

#[must_use]
pub fn data_ws_url(environment: AlpacaDataEnvironment, feed: AlpacaDataFeed) -> String {
    let base = match environment {
        AlpacaDataEnvironment::Live => ALPACA_DATA_WS_URL,
        AlpacaDataEnvironment::Sandbox => ALPACA_DATA_SANDBOX_WS_URL,
    };
    let version = match feed {
        AlpacaDataFeed::Iex
        | AlpacaDataFeed::Sip
        | AlpacaDataFeed::DelayedSip
        | AlpacaDataFeed::Otc => "v2",
        AlpacaDataFeed::Boats | AlpacaDataFeed::Overnight => "v1beta1",
    };

    format!("{base}/{version}/{feed}")
}

#[must_use]
pub const fn trading_http_url(environment: AlpacaEnvironment) -> &'static str {
    match environment {
        AlpacaEnvironment::Live => ALPACA_LIVE_TRADING_HTTP_URL,
        AlpacaEnvironment::Paper => ALPACA_PAPER_TRADING_HTTP_URL,
    }
}

#[must_use]
pub fn trading_ws_url(environment: AlpacaEnvironment) -> String {
    trading_http_url(environment).replacen("https://", "wss://", 1) + "/stream"
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    #[rstest]
    fn test_trading_urls_are_environment_specific() {
        assert_eq!(
            trading_http_url(AlpacaEnvironment::Live),
            "https://api.alpaca.markets"
        );
        assert_eq!(
            trading_http_url(AlpacaEnvironment::Paper),
            "https://paper-api.alpaca.markets"
        );
        assert_eq!(
            trading_ws_url(AlpacaEnvironment::Paper),
            "wss://paper-api.alpaca.markets/stream"
        );
    }

    #[rstest]
    #[case(AlpacaDataFeed::Iex, "wss://stream.data.alpaca.markets/v2/iex")]
    #[case(AlpacaDataFeed::Sip, "wss://stream.data.alpaca.markets/v2/sip")]
    #[case(
        AlpacaDataFeed::DelayedSip,
        "wss://stream.data.alpaca.markets/v2/delayed_sip"
    )]
    #[case(
        AlpacaDataFeed::Boats,
        "wss://stream.data.alpaca.markets/v1beta1/boats"
    )]
    #[case(
        AlpacaDataFeed::Overnight,
        "wss://stream.data.alpaca.markets/v1beta1/overnight"
    )]
    fn test_data_ws_url_uses_feed_protocol_version(
        #[case] feed: AlpacaDataFeed,
        #[case] expected: &str,
    ) {
        assert_eq!(data_ws_url(AlpacaDataEnvironment::Live, feed), expected);
    }
}
