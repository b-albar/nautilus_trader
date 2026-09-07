// -------------------------------------------------------------------------------------------------
//  Copyright (C) 2015-2026 Nautech Systems Pty Ltd. All rights reserved.
//  https://nautechsystems.io
//
//  Licensed under the GNU Lesser General Public License Version 3.0 (the "License");
// -------------------------------------------------------------------------------------------------

use pyo3::prelude::*;

use crate::common::enums::{
    AlpacaBarAdjustment, AlpacaDataEnvironment, AlpacaDataFeed, AlpacaEnvironment,
};

macro_rules! impl_python_enum {
    ($ty:ty, {$($variant:path => $name:literal),+ $(,)?}) => {
        #[pymethods]
        #[pyo3_stub_gen::derive::gen_stub_pymethods]
        impl $ty {
            #[new]
            fn py_new() -> Self { Self::default() }
            const fn __hash__(&self) -> isize { *self as isize }
            fn __str__(&self) -> &'static str { match self { $($variant => $name),+ } }
            fn __repr__(&self) -> String { format!("{}.{}", stringify!($ty), self.__str__()) }
            #[getter]
            fn name(&self) -> String { self.__str__().to_string() }
            #[getter]
            const fn value(&self) -> u8 { *self as u8 }
        }
    };
}

impl_python_enum!(AlpacaEnvironment, { AlpacaEnvironment::Live => "LIVE", AlpacaEnvironment::Paper => "PAPER" });
impl_python_enum!(AlpacaDataEnvironment, { AlpacaDataEnvironment::Live => "LIVE", AlpacaDataEnvironment::Sandbox => "SANDBOX" });
impl_python_enum!(AlpacaDataFeed, { AlpacaDataFeed::Iex => "IEX", AlpacaDataFeed::Sip => "SIP", AlpacaDataFeed::DelayedSip => "DELAYED_SIP", AlpacaDataFeed::Boats => "BOATS", AlpacaDataFeed::Overnight => "OVERNIGHT", AlpacaDataFeed::Otc => "OTC" });
impl_python_enum!(AlpacaBarAdjustment, { AlpacaBarAdjustment::Raw => "RAW", AlpacaBarAdjustment::Split => "SPLIT", AlpacaBarAdjustment::Dividend => "DIVIDEND", AlpacaBarAdjustment::All => "ALL" });
