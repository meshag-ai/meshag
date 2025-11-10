use pyo3::prelude::*;

mod frames;
mod pipeline;
mod processor;
mod runner;

use frames::*;
use pipeline::*;
use runner::*;

#[pymodule]
fn meshag(_py: Python, m: &PyModule) -> PyResult<()> {
    m.add_class::<PyTextFrame>()?;
    m.add_class::<PyAudioFrame>()?;
    m.add_class::<PyPipeline>()?;
    m.add_class::<PyRunner>()?;
    Ok(())
}
