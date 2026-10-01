mod analyze;
mod errors;
mod formats;
mod metadata;

pub use analyze::{
    analyze_audio, analyze_audio_blocking, analyze_stream, wave_fixture, AnalyzeOptions, AudioHints,
    StreamAudioHints, DEFAULT_MAX_ARTWORK_BYTES, DEFAULT_MAX_ARTWORK_COUNT, DEFAULT_MAX_FILE_BYTES,
};
pub use errors::{AudioMetadataError, AudioMetadataErrorCode};
pub use formats::{
    audio_accept, detect_audio_format, find_audio_format_by_extension, find_audio_format_by_mime_type,
    AudioFormat, AudioFormatId, AUDIO_FORMATS,
};
pub use metadata::{
    normalize_metadata, text, AudioAnalysis, AudioArtwork, AudioTags, AudioTechnicalMetadata, AudioWarning,
    AudioWarningCode, NormalizeOptions, NumberPair, ParserMetadata, ParserPicture,
};
