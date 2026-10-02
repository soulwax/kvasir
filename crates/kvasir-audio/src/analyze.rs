use std::io::{Cursor, Read, Seek, SeekFrom};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use bytes::Bytes;
use futures_util::Stream;
use lofty::file::{AudioFile, TaggedFileExt};
use lofty::picture::Picture;
use lofty::probe::Probe;
use lofty::tag::{Accessor, ItemKey};
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::{MediaSource, MediaSourceStream};
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;
use tokio::task::spawn_blocking;

use crate::errors::AudioMetadataError;
use crate::formats::{
    detect_audio_format, find_audio_format_by_extension, find_audio_format_by_mime_type, AudioFormat,
    AudioFormatId,
};
use crate::metadata::{
    normalize_metadata, AudioAnalysis, AudioWarning, AudioWarningCode, NormalizeOptions, ParserMetadata,
    ParserPicture,
};

pub const DEFAULT_MAX_FILE_BYTES: u64 = 128 * 1024 * 1024;
pub const DEFAULT_MAX_ARTWORK_BYTES: u64 = 8 * 1024 * 1024;
pub const DEFAULT_MAX_ARTWORK_COUNT: u32 = 4;
const FORMAT_HEADER_BYTES: usize = 4096;

#[derive(Debug, Clone, Default)]
pub struct AudioHints {
    pub file_name: Option<String>,
    pub mime_type: Option<String>,
    pub size: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct StreamAudioHints {
    pub file_name: Option<String>,
    pub mime_type: Option<String>,
    pub size: u64,
}

impl StreamAudioHints {
    fn as_hints(&self) -> AudioHints {
        AudioHints {
            file_name: self.file_name.clone(),
            mime_type: self.mime_type.clone(),
            size: Some(self.size),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct AnalyzeOptions {
    pub max_file_bytes: Option<u64>,
    pub strict_hints: bool,
    pub include_artwork: bool,
    pub max_artwork_bytes: Option<u64>,
    pub max_artwork_count: Option<u32>,
    pub duration: Option<bool>,
    pub aborted: Option<Arc<AtomicBool>>,
}

fn positive_limit(value: Option<u64>, fallback: u64, name: &str) -> Result<u64, AudioMetadataError> {
    let resolved = value.unwrap_or(fallback);
    if resolved == 0 {
        return Err(AudioMetadataError::InvalidLimit(name.to_string()));
    }
    Ok(resolved)
}

fn abort_if_needed(options: &AnalyzeOptions) -> Result<(), AudioMetadataError> {
    if options
        .aborted
        .as_ref()
        .is_some_and(|flag| flag.load(Ordering::Relaxed))
    {
        return Err(AudioMetadataError::Aborted);
    }
    Ok(())
}

fn hint_warnings(detected_id: AudioFormatId, hints: &AudioHints) -> Vec<AudioWarning> {
    let mut warnings = Vec::new();
    let declared_mime = hints.mime_type.as_deref().map(|mime| {
        mime.trim()
            .to_ascii_lowercase()
            .split(';')
            .next()
            .unwrap_or("")
            .trim()
            .to_string()
    });
    let mime_format = hints
        .mime_type
        .as_deref()
        .and_then(find_audio_format_by_mime_type);
    if declared_mime
        .as_deref()
        .is_some_and(|mime| !mime.is_empty() && mime != "application/octet-stream")
        && mime_format.map(|format| format.id) != Some(detected_id)
    {
        warnings.push(AudioWarning {
            code: AudioWarningCode::MimeMismatch,
            message: "The declared MIME type does not match the detected audio format.".into(),
        });
    }

    let file_name = hints.file_name.as_deref().unwrap_or("").trim();
    let extension_format = if file_name.is_empty() {
        None
    } else {
        find_audio_format_by_extension(file_name)
    };
    let extension = file_name.rsplit('.').next();
    let has_extension = extension.is_some_and(|extension| !extension.is_empty() && extension != file_name);
    if has_extension && extension_format.map(|format| format.id) != Some(detected_id) {
        warnings.push(AudioWarning {
            code: AudioWarningCode::ExtensionMismatch,
            message: "The filename extension does not match the detected audio format.".into(),
        });
    }
    warnings
}

fn validate_hints(
    detected_id: AudioFormatId,
    hints: &AudioHints,
    strict_hints: bool,
) -> Result<Vec<AudioWarning>, AudioMetadataError> {
    let warnings = hint_warnings(detected_id, hints);
    if strict_hints && !warnings.is_empty() {
        return Err(AudioMetadataError::HintMismatch(
            "Declared audio hints do not match the detected format.".into(),
        ));
    }
    Ok(warnings)
}

struct CursorSource {
    cursor: Cursor<Vec<u8>>,
}

impl Read for CursorSource {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.cursor.read(buf)
    }
}

impl Seek for CursorSource {
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        self.cursor.seek(pos)
    }
}

impl MediaSource for CursorSource {
    fn is_seekable(&self) -> bool {
        true
    }

    fn byte_len(&self) -> Option<u64> {
        Some(self.cursor.get_ref().len() as u64)
    }
}

fn picture_from_lofty(picture: &Picture) -> ParserPicture {
    ParserPicture {
        format: picture
            .mime_type()
            .map(|mime| mime.to_string())
            .unwrap_or_else(|| "application/octet-stream".into()),
        data: picture.data().to_vec(),
        description: picture.description().filter(|value| !value.is_empty()).map(|value| value.to_string()),
    }
}

fn metadata_from_lofty(bytes: &[u8], read_duration: bool) -> Result<ParserMetadata, AudioMetadataError> {
    let tagged = Probe::new(Cursor::new(bytes.to_vec()))
        .guess_file_type()
        .map_err(|_| AudioMetadataError::MalformedAudio)?
        .read()
        .map_err(|_| AudioMetadataError::MalformedAudio)?;
    let properties = tagged.properties();
    let mut metadata = ParserMetadata {
        container: Some(format!("{:?}", tagged.file_type())),
        codec: None,
        duration: if read_duration {
            Some(properties.duration().as_secs_f64())
        } else {
            None
        },
        bitrate: properties.audio_bitrate().map(|bitrate| bitrate as f64),
        sample_rate: properties.sample_rate().map(|rate| rate as f64),
        number_of_channels: properties.channels().map(|channels| u32::from(channels)),
        bits_per_sample: properties.bit_depth().map(u32::from),
        lossless: None,
        ..ParserMetadata::default()
    };

    if let Some(tag) = tagged.primary_tag() {
        metadata.title = tag.title().map(|value| value.to_string());
        metadata.artist = tag.artist().map(|value| value.to_string());
        metadata.artists = tag.get_strings(&ItemKey::TrackArtist).map(|value| value.to_string()).collect();
        metadata.album = tag.album().map(|value| value.to_string());
        metadata.album_artist = tag.get_string(&ItemKey::AlbumArtist).map(|value| value.to_string());
        metadata.track_number = tag.track();
        metadata.track_total = tag.track_total();
        metadata.disc_number = tag.disk();
        metadata.disc_total = tag.disk_total();
        metadata.year = tag.year();
        metadata.date = tag.get_string(&ItemKey::RecordingDate).map(|value| value.to_string());
        metadata.genres = tag.genre().map(|genre| vec![genre.to_string()]).unwrap_or_default();
        metadata.composers = tag.get_strings(&ItemKey::Composer).map(|value| value.to_string()).collect();
        metadata.isrc = tag
            .get_string(&ItemKey::Isrc)
            .map(|value| vec![value.to_string()])
            .unwrap_or_default();
        metadata.copyright = tag.get_string(&ItemKey::CopyrightMessage).map(|value| value.to_string());
        metadata.pictures = tag.pictures().iter().map(picture_from_lofty).collect();
    }

    Ok(metadata)
}

fn metadata_from_symphonia(bytes: &[u8], read_duration: bool) -> Result<ParserMetadata, AudioMetadataError> {
    let source = CursorSource {
        cursor: Cursor::new(bytes.to_vec()),
    };
    let mss = MediaSourceStream::new(Box::new(source), Default::default());
    let probed = symphonia::default::get_probe()
        .format(&Hint::new(), mss, &FormatOptions::default(), &MetadataOptions::default())
        .map_err(|_| AudioMetadataError::MalformedAudio)?;
    let track = probed
        .format
        .default_track()
        .or_else(|| probed.format.tracks().first())
        .ok_or(AudioMetadataError::NoAudioStream)?;
    let params = &track.codec_params;
    let duration = if read_duration {
        params
            .n_frames
            .zip(params.sample_rate)
            .map(|(frames, rate)| frames as f64 / f64::from(rate))
    } else {
        None
    };
    Ok(ParserMetadata {
        container: None,
        codec: Some(format!("{:?}", params.codec)),
        duration,
        bitrate: params.bits_per_sample.and_then(|bits| {
            params.sample_rate.map(|rate| {
                let channels = params.channels.map(|channels| channels.count()).unwrap_or(1) as u32;
                f64::from(rate) * f64::from(bits) * f64::from(channels)
            })
        }),
        sample_rate: params.sample_rate.map(f64::from),
        number_of_channels: params.channels.map(|channels| channels.count() as u32),
        bits_per_sample: params.bits_per_sample.map(u32::from),
        lossless: None,
        ..ParserMetadata::default()
    })
}

fn parse_metadata(bytes: &[u8], detected: AudioFormat, read_duration: bool) -> Result<ParserMetadata, AudioMetadataError> {
    let lofty_result = metadata_from_lofty(bytes, read_duration);
    let metadata = match detected.id {
        AudioFormatId::Aac | AudioFormatId::Webm => metadata_from_symphonia(bytes, read_duration).or(lofty_result),
        _ => lofty_result.or_else(|_| metadata_from_symphonia(bytes, read_duration)),
    }?;
    let has_audio = metadata.codec.is_some()
        || metadata.sample_rate.unwrap_or(0.0) > 0.0
        || metadata.number_of_channels.unwrap_or(0) > 0
        || metadata.bitrate.unwrap_or(0.0) > 0.0;
    if !has_audio {
        return Err(AudioMetadataError::NoAudioStream);
    }
    Ok(metadata)
}

fn analyze_bytes(
    bytes: &[u8],
    hints: &AudioHints,
    options: &AnalyzeOptions,
    max_file_bytes: u64,
    max_artwork_bytes: u64,
    max_artwork_count: u32,
) -> Result<AudioAnalysis, AudioMetadataError> {
    abort_if_needed(options)?;
    if bytes.is_empty() {
        return Err(AudioMetadataError::EmptyInput);
    }
    if bytes.len() as u64 > max_file_bytes {
        return Err(AudioMetadataError::FileTooLarge);
    }
    if let Some(size) = hints.size {
        if size != bytes.len() as u64 {
            return Err(AudioMetadataError::HintMismatch(
                "The declared size does not match the audio input.".into(),
            ));
        }
    }
    let detected = detect_audio_format(bytes).ok_or(AudioMetadataError::UnsupportedFormat)?;
    let warnings = validate_hints(detected.id, hints, options.strict_hints)?;
    let parsed = parse_metadata(bytes, detected, options.duration.unwrap_or(true))?;
    abort_if_needed(options)?;
    Ok(normalize_metadata(
        detected,
        &parsed,
        NormalizeOptions {
            include_artwork: options.include_artwork,
            max_artwork_bytes,
            max_artwork_count,
        },
        warnings,
    ))
}

pub fn analyze_audio_blocking(
    bytes: &[u8],
    hints: &AudioHints,
    options: &AnalyzeOptions,
) -> Result<AudioAnalysis, AudioMetadataError> {
    let max_file_bytes = positive_limit(options.max_file_bytes, DEFAULT_MAX_FILE_BYTES, "max_file_bytes")?;
    let max_artwork_bytes = positive_limit(
        options.max_artwork_bytes,
        DEFAULT_MAX_ARTWORK_BYTES,
        "max_artwork_bytes",
    )?;
    let max_artwork_count = positive_limit(
        options.max_artwork_count.map(u64::from),
        u64::from(DEFAULT_MAX_ARTWORK_COUNT),
        "max_artwork_count",
    )? as u32;
    analyze_bytes(
        bytes,
        hints,
        options,
        max_file_bytes,
        max_artwork_bytes,
        max_artwork_count,
    )
}

pub async fn analyze_audio(
    bytes: impl Into<Bytes>,
    hints: AudioHints,
    options: AnalyzeOptions,
) -> Result<AudioAnalysis, AudioMetadataError> {
    let bytes = bytes.into();
    spawn_blocking(move || analyze_audio_blocking(&bytes, &hints, &options))
        .await
        .map_err(|_| AudioMetadataError::ParserFailure)?
}

pub async fn analyze_stream<S, E>(
    mut stream: S,
    hints: StreamAudioHints,
    options: AnalyzeOptions,
) -> Result<AudioAnalysis, AudioMetadataError>
where
    S: Stream<Item = Result<Bytes, E>> + Unpin,
    E: std::error::Error + Send + Sync + 'static,
{
    let max_file_bytes = positive_limit(options.max_file_bytes, DEFAULT_MAX_FILE_BYTES, "max_file_bytes")?;
    let max_artwork_bytes = positive_limit(
        options.max_artwork_bytes,
        DEFAULT_MAX_ARTWORK_BYTES,
        "max_artwork_bytes",
    )?;
    let max_artwork_count = positive_limit(
        options.max_artwork_count.map(u64::from),
        u64::from(DEFAULT_MAX_ARTWORK_COUNT),
        "max_artwork_count",
    )? as u32;
    abort_if_needed(&options)?;
    if hints.size == 0 {
        return Err(AudioMetadataError::EmptyInput);
    }
    if hints.size > max_file_bytes {
        return Err(AudioMetadataError::FileTooLarge);
    }

    let mut collected = Vec::new();
    while collected.len() < FORMAT_HEADER_BYTES && (collected.len() as u64) < hints.size {
        abort_if_needed(&options)?;
        let Some(chunk) = futures_util::StreamExt::next(&mut stream).await else {
            break;
        };
        let chunk = chunk.map_err(|_| AudioMetadataError::ParserFailure)?;
        if (collected.len() + chunk.len()) as u64 > hints.size {
            return Err(AudioMetadataError::HintMismatch(
                "The stream exceeds its declared size.".into(),
            ));
        }
        collected.extend_from_slice(&chunk);
    }
    let detected = detect_audio_format(&collected).ok_or(AudioMetadataError::UnsupportedFormat)?;
    let warnings = validate_hints(detected.id, &hints.as_hints(), options.strict_hints)?;

    while (collected.len() as u64) < hints.size {
        abort_if_needed(&options)?;
        let Some(chunk) = futures_util::StreamExt::next(&mut stream).await else {
            break;
        };
        let chunk = chunk.map_err(|_| AudioMetadataError::ParserFailure)?;
        if (collected.len() + chunk.len()) as u64 > hints.size {
            return Err(AudioMetadataError::HintMismatch(
                "The stream exceeds its declared size.".into(),
            ));
        }
        collected.extend_from_slice(&chunk);
    }

    let read_duration = options.duration.unwrap_or(true);
    let include_artwork = options.include_artwork;
    let aborted = options.aborted.clone();
    let analysis = spawn_blocking(move || {
        let parsed = parse_metadata(&collected, detected, read_duration)?;
        if aborted
            .as_ref()
            .is_some_and(|flag| flag.load(Ordering::Relaxed))
        {
            return Err(AudioMetadataError::Aborted);
        }
        Ok(normalize_metadata(
            detected,
            &parsed,
            NormalizeOptions {
                include_artwork,
                max_artwork_bytes,
                max_artwork_count,
            },
            warnings,
        ))
    })
    .await
    .map_err(|_| AudioMetadataError::ParserFailure)??;
    Ok(analysis)
}

pub fn wave_fixture() -> Vec<u8> {
    fn ascii(value: &str) -> Vec<u8> {
        value.as_bytes().to_vec()
    }
    fn le(value: u32, bytes: usize) -> Vec<u8> {
        (0..bytes).map(|index| ((value >> (index * 8)) & 0xff) as u8).collect()
    }
    let samples = [128u8, 144, 160, 144, 128, 112, 96, 112];
    let data_size = samples.len() as u32;
    let riff_size = 36 + data_size;
    let mut bytes = Vec::new();
    bytes.extend(ascii("RIFF"));
    bytes.extend(le(riff_size, 4));
    bytes.extend(ascii("WAVE"));
    bytes.extend(ascii("fmt "));
    bytes.extend(le(16, 4));
    bytes.extend(le(1, 2));
    bytes.extend(le(1, 2));
    bytes.extend(le(8000, 4));
    bytes.extend(le(8000, 4));
    bytes.extend(le(1, 2));
    bytes.extend(le(8, 2));
    bytes.extend(ascii("data"));
    bytes.extend(le(data_size, 4));
    bytes.extend(samples);
    bytes
}
