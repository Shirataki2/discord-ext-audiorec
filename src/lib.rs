#![allow(clippy::needless_range_loop)]
#[macro_use]
extern crate log;
#[macro_use]
extern crate serde_derive;

pub(crate) mod connection;
pub(crate) mod error;
pub(crate) mod futures;
pub(crate) mod payload;
pub(crate) mod player;
pub(crate) mod recorder;
pub(crate) mod state;
pub(crate) mod ws;

use pyo3::prelude::*;
use pyo3_log::{Caching, Logger};

use crate::{
    connection::{VoiceConnection, VoiceConnector},
    error::*,
};

#[pymodule]
fn ffi(py: Python, m: &PyModule) -> PyResult<()> {
    let _ = Logger::new(py, Caching::LoggersAndLevels)?.install();

    m.add_class::<VoiceConnector>()?;
    m.add_class::<VoiceConnection>()?;
    m.add("MissingFieldError", py.get_type::<MissingFieldError>())?;
    m.add("InternalError", py.get_type::<InternalError>())?;
    m.add("InternalIOError", py.get_type::<InternalIOError>())?;
    m.add("TlsError", py.get_type::<TlsError>())?;
    m.add("GatewayError", py.get_type::<GatewayError>())?;
    m.add("TryReconnect", py.get_type::<TryReconnect>())?;
    m.add("EncryptionFailed", py.get_type::<EncryptionFailed>())?;
    Ok(())
}
