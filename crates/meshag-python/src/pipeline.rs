use pyo3::prelude::*;

#[pyclass]
#[derive(Clone)]
pub struct PyPipeline {
    _name: String,
}

#[pymethods]
impl PyPipeline {
    #[new]
    fn new(name: String) -> Self {
        Self { _name: name }
    }

    fn with_websocket_url(&mut self, _url: String) -> PyResult<()> {
        Ok(())
    }

    fn add_stt_processor(&mut self) -> PyResult<()> {
        Ok(())
    }

    fn add_llm_processor(&mut self) -> PyResult<()> {
        Ok(())
    }

    fn add_tts_processor(&mut self) -> PyResult<()> {
        Ok(())
    }
}
