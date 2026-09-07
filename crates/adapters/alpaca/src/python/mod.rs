// -------------------------------------------------------------------------------------------------
//  Copyright (C) 2015-2026 Nautech Systems Pty Ltd. All rights reserved.
//  https://nautechsystems.io
//
//  Licensed under the GNU Lesser General Public License Version 3.0 (the "License");
// -------------------------------------------------------------------------------------------------

pub mod config;
pub mod enums;
pub mod factories;
pub mod http;

use nautilus_common::factories::{ClientConfig, DataClientFactory, ExecutionClientFactory};
use nautilus_core::python::{to_pyruntime_err, to_pyvalue_err};
use nautilus_system::get_global_pyo3_registry;
use pyo3::prelude::*;

use crate::{
    common::consts::{ALPACA, ALPACA_CLIENT_ID, ALPACA_VENUE},
    config::{AlpacaDataClientConfig, AlpacaExecutionClientConfig},
    factories::{AlpacaDataClientFactory, AlpacaExecutionClientFactory},
};

#[expect(clippy::needless_pass_by_value)]
fn extract_data_factory(py: Python<'_>, value: Py<PyAny>) -> PyResult<Box<dyn DataClientFactory>> {
    value
        .extract::<AlpacaDataClientFactory>(py)
        .map(|value| Box::new(value) as Box<dyn DataClientFactory>)
        .map_err(|error| to_pyvalue_err(error.to_string()))
}
#[expect(clippy::needless_pass_by_value)]
fn extract_exec_factory(
    py: Python<'_>,
    value: Py<PyAny>,
) -> PyResult<Box<dyn ExecutionClientFactory>> {
    value
        .extract::<AlpacaExecutionClientFactory>(py)
        .map(|value| Box::new(value) as Box<dyn ExecutionClientFactory>)
        .map_err(|error| to_pyvalue_err(error.to_string()))
}
#[expect(clippy::needless_pass_by_value)]
fn extract_data_config(py: Python<'_>, value: Py<PyAny>) -> PyResult<Box<dyn ClientConfig>> {
    value
        .extract::<AlpacaDataClientConfig>(py)
        .map(|value| Box::new(value) as Box<dyn ClientConfig>)
        .map_err(|error| to_pyvalue_err(error.to_string()))
}
#[expect(clippy::needless_pass_by_value)]
fn extract_exec_config(py: Python<'_>, value: Py<PyAny>) -> PyResult<Box<dyn ClientConfig>> {
    value
        .extract::<AlpacaExecutionClientConfig>(py)
        .map(|value| Box::new(value) as Box<dyn ClientConfig>)
        .map_err(|error| to_pyvalue_err(error.to_string()))
}

#[pymodule]
pub fn alpaca(_: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add(stringify!(ALPACA), ALPACA)?;
    m.add(stringify!(ALPACA_CLIENT_ID), *ALPACA_CLIENT_ID)?;
    m.add(stringify!(ALPACA_VENUE), *ALPACA_VENUE)?;
    m.add_class::<crate::AlpacaEnvironment>()?;
    m.add_class::<crate::AlpacaDataEnvironment>()?;
    m.add_class::<crate::AlpacaDataFeed>()?;
    m.add_class::<crate::AlpacaBarAdjustment>()?;
    m.add_class::<AlpacaDataClientConfig>()?;
    m.add_class::<AlpacaExecutionClientConfig>()?;
    m.add_class::<AlpacaDataClientFactory>()?;
    m.add_class::<AlpacaExecutionClientFactory>()?;
    m.add_class::<http::AlpacaAccountActivityClient>()?;
    m.add_class::<http::AlpacaPortfolioClient>()?;
    m.add_class::<http::AlpacaReferenceDataClient>()?;
    m.add_class::<http::AlpacaHistoricalDataClient>()?;

    let registry = get_global_pyo3_registry();
    registry
        .register_factory_extractor(ALPACA.to_string(), extract_data_factory)
        .map_err(|error| to_pyruntime_err(error.to_string()))?;
    registry
        .register_exec_factory_extractor(ALPACA.to_string(), extract_exec_factory)
        .map_err(|error| to_pyruntime_err(error.to_string()))?;
    registry
        .register_config_extractor("AlpacaDataClientConfig".to_string(), extract_data_config)
        .map_err(|error| to_pyruntime_err(error.to_string()))?;
    registry
        .register_config_extractor(
            "AlpacaExecutionClientConfig".to_string(),
            extract_exec_config,
        )
        .map_err(|error| to_pyruntime_err(error.to_string()))?;
    Ok(())
}
