use pyo3::prelude::*;

use crate::pipeline::PyPipeline;

#[pyclass]
pub struct PyRunner {}

#[pymethods]
impl PyRunner {
    #[new]
    fn new(_pipeline: PyPipeline) -> Self {
        Self {}
    }

    fn start(&self) -> PyResult<()> {
        Ok(())
    }

    fn stop(&self) -> PyResult<()> {
        Ok(())
    }

    fn push_frame(&self, _frame: PyObject) -> PyResult<()> {
        Ok(())
    }
}
