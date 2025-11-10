use pyo3::prelude::*;

#[pyclass]
pub struct PyProcessor {
    _name: String,
}

#[pymethods]
impl PyProcessor {
    #[new]
    fn new(name: String) -> Self {
        Self { _name: name }
    }
}
