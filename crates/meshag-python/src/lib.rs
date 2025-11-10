use pyo3::prelude::*;

mod frames;
mod pipeline;
mod processor;
mod runner;

use frames::*;
use pipeline::*;
use runner::*;

#[pymodule]
fn meshag(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyTextFrame>()?;
    m.add_class::<PyAudioFrame>()?;
    m.add_class::<PyPipeline>()?;
    m.add_class::<PyRunner>()?;
    Ok(())
}
