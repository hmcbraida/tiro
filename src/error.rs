use std::{fmt, io};

#[derive(Debug)]
pub enum TiroError {
    Io(io::Error),
    NotFound(String),
    Parse(String),
}

impl fmt::Display for TiroError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TiroError::Io(e) => write!(f, "i/o error: {e}"),
            TiroError::NotFound(id) => write!(f, "note not found: {id}"),
            TiroError::Parse(msg) => write!(f, "parse error: {msg}"),
        }
    }
}

impl std::error::Error for TiroError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            TiroError::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<io::Error> for TiroError {
    fn from(e: io::Error) -> Self {
        TiroError::Io(e)
    }
}

pub type Result<T> = std::result::Result<T, TiroError>;
