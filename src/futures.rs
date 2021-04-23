#![allow(dead_code)]
use pyo3::prelude::*;

pub(crate) fn set_result(
    py: Python,
    loop_: PyObject,
    future: PyObject,
    result: PyObject,
) -> PyResult<()> {
    let set = future.getattr(py, "set_result")?;
    loop_.call_method1(py, "call_soon_threadsafe", (set, result))?;
    Ok(())
}

pub(crate) fn set_exception(
    py: Python,
    loop_: PyObject,
    future: PyObject,
    exception: PyErr,
) -> PyResult<()> {
    let set = future.getattr(py, "set_exception")?;
    loop_.call_method1(py, "call_soon_threadsafe", (set, exception.to_object(py)))?;
    Ok(())
}
