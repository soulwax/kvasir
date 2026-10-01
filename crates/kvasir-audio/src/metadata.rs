use crate::formats::{AudioFormat, AudioFormatId};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AudioWarningCode {
    MimeMismatch,
    ExtensionMismatch,
    ArtworkOmitted,
    ArtworkLimit,
    PartialMetadata,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioWarning {
    pub code: AudioWarningCode,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioArtwork {
    pub content_type: String,
    pub data: Vec<u8>,
    pub description: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NumberPair {
    pub number: Option<u32>,
    pub total: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioTags {
    pub title: Option<String>,
    pub artists: Vec<String>,
    pub album: Option<String>,
    pub album_artists: Vec<String>,
    pub track: Option<NumberPair>,
    pub disc: Option<NumberPair>,
    pub date: Option<String>,
    pub year: Option<u32>,
    pub genres: Vec<String>,
    pub composers: Vec<String>,
    pub isrc: Option<String>,
    pub copyright: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AudioTechnicalMetadata {
    pub id: AudioFormatId,
    pub content_type: String,
    pub container: Option<String>,
    pub codec: Option<String>,
    pub duration_seconds: Option<f64>,
    pub bitrate: Option<f64>,
    pub sample_rate: Option<f64>,
    pub channels: Option<u32>,
    pub bits_per_sample: Option<u32>,
    pub lossless: Option<bool>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AudioAnalysis {
    pub format: AudioTechnicalMetadata,
    pub tags: AudioTags,
    pub artwork: Vec<AudioArtwork>,
    pub warnings: Vec<AudioWarning>,
}

#[derive(Debug, Clone)]
pub struct ParserPicture {
    pub format: String,
    pub data: Vec<u8>,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct ParserMetadata {
    pub container: Option<String>,
    pub codec: Option<String>,
    pub duration: Option<f64>,
    pub bitrate: Option<f64>,
    pub sample_rate: Option<f64>,
    pub number_of_channels: Option<u32>,
    pub bits_per_sample: Option<u32>,
    pub lossless: Option<bool>,
    pub title: Option<String>,
    pub artist: Option<String>,
    pub artists: Vec<String>,
    pub album: Option<String>,
    pub album_artist: Option<String>,
    pub track_number: Option<u32>,
    pub track_total: Option<u32>,
    pub disc_number: Option<u32>,
    pub disc_total: Option<u32>,
    pub date: Option<String>,
    pub year: Option<u32>,
    pub genres: Vec<String>,
    pub composers: Vec<String>,
    pub isrc: Vec<String>,
    pub copyright: Option<String>,
    pub pictures: Vec<ParserPicture>,
}

#[derive(Debug, Clone, Copy)]
pub struct NormalizeOptions {
    pub include_artwork: bool,
    pub max_artwork_bytes: u64,
    pub max_artwork_count: u32,
}

pub fn text(value: &str, max_length: usize) -> Option<String> {
    let normalized: String = value
        .chars()
        .map(|ch| if ch.is_control() { ' ' } else { ch })
        .collect();
    let collapsed = normalized.split_whitespace().collect::<Vec<_>>().join(" ");
    let trimmed: String = collapsed.chars().take(max_length).collect();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

fn strings(values: impl IntoIterator<Item = Option<String>>) -> Vec<String> {
    let mut result = Vec::new();
    for value in values.into_iter().flatten() {
        if !result.contains(&value) {
            result.push(value);
        }
        if result.len() == 64 {
            break;
        }
    }
    result
}

fn pair(number: Option<u32>, total: Option<u32>) -> Option<NumberPair> {
    if number.is_none() && total.is_none() {
        None
    } else {
        Some(NumberPair { number, total })
    }
}

fn normalize_artwork(
    pictures: &[ParserPicture],
    options: NormalizeOptions,
    warnings: &mut Vec<AudioWarning>,
) -> Vec<AudioArtwork> {
    if !options.include_artwork {
        if !pictures.is_empty() {
            warnings.push(AudioWarning {
                code: AudioWarningCode::ArtworkOmitted,
                message: "Embedded artwork was omitted.".into(),
            });
        }
        return Vec::new();
    }

    let mut artwork = Vec::new();
    let mut total_bytes = 0u64;
    for picture in pictures {
        let len = picture.data.len() as u64;
        if artwork.len() as u32 >= options.max_artwork_count
            || len > options.max_artwork_bytes
            || total_bytes.saturating_add(len) > options.max_artwork_bytes
        {
            warnings.push(AudioWarning {
                code: AudioWarningCode::ArtworkLimit,
                message: "Embedded artwork exceeded configured limits.".into(),
            });
            break;
        }
        let Some(content_type) = text(&picture.format, 128) else {
            continue;
        };
        let description = picture
            .description
            .as_deref()
            .and_then(|value| text(value, 256));
        artwork.push(AudioArtwork {
            content_type,
            data: picture.data.clone(),
            description,
        });
        total_bytes += len;
    }
    artwork
}

pub fn normalize_metadata(
    detected: AudioFormat,
    metadata: &ParserMetadata,
    options: NormalizeOptions,
    initial_warnings: Vec<AudioWarning>,
) -> AudioAnalysis {
    let mut warnings = initial_warnings;
    let artists = strings(
        metadata
            .artists
            .iter()
            .map(|artist| text(artist, 512))
            .chain(std::iter::once(metadata.artist.as_deref().and_then(|value| text(value, 512)))),
    );
    let album_artists = strings(std::iter::once(
        metadata.album_artist.as_deref().and_then(|value| text(value, 512)),
    ));
    let genres = strings(metadata.genres.iter().map(|genre| text(genre, 512)));
    let composers = strings(metadata.composers.iter().map(|composer| text(composer, 512)));
    let artwork = normalize_artwork(&metadata.pictures, options, &mut warnings);

    AudioAnalysis {
        format: AudioTechnicalMetadata {
            id: detected.id,
            content_type: detected.content_type.to_string(),
            container: metadata.container.as_deref().and_then(|value| text(value, 128)),
            codec: metadata.codec.as_deref().and_then(|value| text(value, 128)),
            duration_seconds: metadata.duration.filter(|value| value.is_finite() && *value >= 0.0),
            bitrate: metadata.bitrate.filter(|value| value.is_finite() && *value >= 0.0),
            sample_rate: metadata.sample_rate.filter(|value| value.is_finite() && *value >= 0.0),
            channels: metadata.number_of_channels,
            bits_per_sample: metadata.bits_per_sample,
            lossless: metadata.lossless,
        },
        tags: AudioTags {
            title: metadata.title.as_deref().and_then(|value| text(value, 512)),
            artists,
            album: metadata.album.as_deref().and_then(|value| text(value, 512)),
            album_artists,
            track: pair(metadata.track_number, metadata.track_total),
            disc: pair(metadata.disc_number, metadata.disc_total),
            date: metadata.date.as_deref().and_then(|value| text(value, 64)),
            year: metadata.year,
            genres,
            composers,
            isrc: metadata
                .isrc
                .first()
                .and_then(|value| text(value, 64)),
            copyright: metadata.copyright.as_deref().and_then(|value| text(value, 512)),
        },
        artwork,
        warnings,
    }
}
