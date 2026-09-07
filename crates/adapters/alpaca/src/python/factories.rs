// -------------------------------------------------------------------------------------------------
//  Copyright (C) 2015-2026 Nautech Systems Pty Ltd. All rights reserved.
//  https://nautechsystems.io
//
//  Licensed under the GNU Lesser General Public License Version 3.0 (the "License");
// -------------------------------------------------------------------------------------------------

use pyo3::prelude::*;

use crate::{
    common::consts::ALPACA,
    factories::{AlpacaDataClientFactory, AlpacaExecutionClientFactory},
};

#[pymethods]
#[pyo3_stub_gen::derive::gen_stub_pymethods]
impl AlpacaDataClientFactory {
    #[new]
    fn py_new() -> Self {
        Self
    }
    #[pyo3(name = "name")]
    fn py_name(&self) -> &'static str {
        ALPACA
    }
}

#[pymethods]
#[pyo3_stub_gen::derive::gen_stub_pymethods]
impl AlpacaExecutionClientFactory {
    #[new]
    fn py_new() -> Self {
        Self
    }
    #[pyo3(name = "name")]
    fn py_name(&self) -> &'static str {
        ALPACA
    }
}
