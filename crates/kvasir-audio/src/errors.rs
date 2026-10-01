use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioMetadataErrorCode {
    EmptyInput,
    FileTooLarge,
    UnsupportedFormat,
    HintMismatch,
    MalformedAudio,
    Aborted,
    ParserFailure,
}

#[derive(Debug, Error)]
pub enum AudioMetadataError {
    #[error("Audio input is empty.")]
    EmptyInput,
    #[error("Audio input exceeds the configured size limit.")]
    FileTooLarge,
    #[error("The audio format is not supported.")]
    UnsupportedFormat,
    #[error("{0}")]
    HintMismatch(String),
    #[error("The audio file could not be parsed.")]
    MalformedAudio,
    #[error("The file has no parseable audio stream.")]
    NoAudioStream,
    #[error("Audio analysis was aborted.")]
    Aborted,
    #[error("The metadata parser failed unexpectedly.")]
    ParserFailure,
    #[error("{0} must be a positive integer.")]
    InvalidLimit(String),
}

impl AudioMetadataError {
    pub fn code(&self) -> AudioMetadataErrorCode {
        match self {
            Self::EmptyInput => AudioMetadataErrorCode::EmptyInput,
            Self::FileTooLarge => AudioMetadataErrorCode::FileTooLarge,
            Self::UnsupportedFormat => AudioMetadataErrorCode::UnsupportedFormat,
            Self::HintMismatch(_) => AudioMetadataErrorCode::HintMismatch,
            Self::MalformedAudio | Self::NoAudioStream => AudioMetadataErrorCode::MalformedAudio,
            Self::Aborted => AudioMetadataErrorCode::Aborted,
            Self::ParserFailure => AudioMetadataErrorCode::ParserFailure,
            Self::InvalidLimit(_) => AudioMetadataErrorCode::ParserFailure,
        }
    }
}
