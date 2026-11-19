use std::fmt;

use crate::save_fault::{classify_save, SaveFault};

#[derive(Debug)]
pub struct CoreError {
    pub message: String,
    pub fault: Option<SaveFault>,
}

impl CoreError {
    pub fn new(message: impl Into<String>) -> Self {
        Self { message: message.into(), fault: None }
    }
}

impl fmt::Display for CoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for CoreError {}

impl From<std::io::Error> for CoreError {
    fn from(value: std::io::Error) -> Self {
        let fault = classify_save(value.kind(), value.raw_os_error());
        Self { message: value.to_string(), fault: Some(fault) }
    }
}

impl From<seal_sealtext::TextError> for CoreError {
    fn from(value: seal_sealtext::TextError) -> Self {
        Self::new(value.to_string())
    }
}
