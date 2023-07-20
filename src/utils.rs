use clap::Parser;
use std::path::PathBuf;

#[derive(Debug, Clone, thiserror::Error)]
pub enum Error {
    #[error("Failed to parse header: {0}")]
    HeaderParseError(String),
    #[error("Failed to decode: {0}")]
    DecodingError(String),
}

#[derive(Debug, Parser)]
pub struct Args {
    pub file: PathBuf,
}
