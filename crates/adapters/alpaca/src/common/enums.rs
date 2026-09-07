// -------------------------------------------------------------------------------------------------
//  Copyright (C) 2015-2026 Nautech Systems Pty Ltd. All rights reserved.
//  https://nautechsystems.io
//
//  Licensed under the GNU Lesser General Public License Version 3.0 (the "License");
// -------------------------------------------------------------------------------------------------

use serde::{Deserialize, Serialize};
use strum::{Display, EnumString};

#[derive(
    Copy, Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, Display, EnumString,
)]
#[strum(serialize_all = "lowercase")]
#[cfg_attr(
    feature = "python",
    pyo3::pyclass(
        frozen,
        eq,
        eq_int,
        from_py_object,
        module = "nautilus_trader.adapters.alpaca",
        rename_all = "SCREAMING_SNAKE_CASE"
    )
)]
#[cfg_attr(
    feature = "python",
    pyo3_stub_gen::derive::gen_stub_pyclass_enum(module = "nautilus_trader.adapters.alpaca")
)]
pub enum AlpacaEnvironment {
    Live,
    #[default]
    Paper,
}

#[derive(
    Copy, Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, Display, EnumString,
)]
#[strum(serialize_all = "lowercase")]
#[cfg_attr(
    feature = "python",
    pyo3::pyclass(
        frozen,
        eq,
        eq_int,
        from_py_object,
        module = "nautilus_trader.adapters.alpaca",
        rename_all = "SCREAMING_SNAKE_CASE"
    )
)]
#[cfg_attr(
    feature = "python",
    pyo3_stub_gen::derive::gen_stub_pyclass_enum(module = "nautilus_trader.adapters.alpaca")
)]
pub enum AlpacaDataEnvironment {
    #[default]
    Live,
    Sandbox,
}

#[derive(
    Copy, Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, Display, EnumString,
)]
#[cfg_attr(
    feature = "python",
    pyo3::pyclass(
        frozen,
        eq,
        eq_int,
        from_py_object,
        module = "nautilus_trader.adapters.alpaca",
        rename_all = "SCREAMING_SNAKE_CASE"
    )
)]
#[cfg_attr(
    feature = "python",
    pyo3_stub_gen::derive::gen_stub_pyclass_enum(module = "nautilus_trader.adapters.alpaca")
)]
#[serde(rename_all = "lowercase")]
#[strum(serialize_all = "lowercase")]
pub enum AlpacaDataFeed {
    #[default]
    Iex,
    Sip,
    #[serde(rename = "delayed_sip")]
    #[strum(serialize = "delayed_sip")]
    DelayedSip,
    Boats,
    Overnight,
    Otc,
}

impl AlpacaDataFeed {
    #[must_use]
    pub const fn supports_live_streaming(self) -> bool {
        !matches!(self, Self::Otc)
    }

    #[must_use]
    pub const fn supports_historical_ranges(self) -> bool {
        matches!(self, Self::Iex | Self::Sip | Self::Boats | Self::Otc)
    }
}

/// Corporate-action adjustment applied to historical stock bars.
#[derive(
    Copy, Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, Display, EnumString,
)]
#[cfg_attr(
    feature = "python",
    pyo3::pyclass(
        frozen,
        eq,
        eq_int,
        from_py_object,
        module = "nautilus_trader.adapters.alpaca",
        rename_all = "SCREAMING_SNAKE_CASE"
    )
)]
#[cfg_attr(
    feature = "python",
    pyo3_stub_gen::derive::gen_stub_pyclass_enum(module = "nautilus_trader.adapters.alpaca")
)]
#[serde(rename_all = "lowercase")]
#[strum(serialize_all = "lowercase")]
pub enum AlpacaBarAdjustment {
    #[default]
    Raw,
    Split,
    Dividend,
    All,
}
