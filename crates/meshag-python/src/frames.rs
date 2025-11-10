use pyo3::prelude::*;

#[pyclass]
#[derive(Clone)]
pub struct PyTextFrame {
    #[pyo3(get, set)]
    pub session_id: String,
    #[pyo3(get, set)]
    pub text: String,
    #[pyo3(get, set)]
    pub language: Option<String>,
}

#[pymethods]
impl PyTextFrame {
    #[new]
    fn new(session_id: String, text: String, language: Option<String>) -> Self {
        Self {
            session_id,
            text,
            language,
        }
    }
}

#[pyclass]
#[derive(Clone)]
pub struct PyAudioFrame {
    #[pyo3(get, set)]
    pub session_id: String,
    #[pyo3(get)]
    pub sample_rate: u32,
    #[pyo3(get)]
    pub num_channels: u16,
}

#[pymethods]
impl PyAudioFrame {
    #[new]
    fn new(session_id: String, _data: Vec<u8>, sample_rate: u32, num_channels: u16) -> Self {
        Self {
            session_id,
            sample_rate,
            num_channels,
        }
    }
}
