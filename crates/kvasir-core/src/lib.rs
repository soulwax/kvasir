//! Self-contained facade for acquisition, byte-authoritative inspection, and tagging.
//!
//! Depend on this crate alone. It re-exports [`kvasir_audio`] and [`kvasir_deezer`]
//! and adds the reconcile step that neither of those crates can see on its own.
#![forbid(unsafe_code)]

use std::time::Duration;

use bytes::Bytes;
use kvasir_audio as audio;
use kvasir_deezer as catalogue;
use thiserror::Error;

pub use kvasir_audio::*;
pub use kvasir_deezer::*;

const DURATION_TOLERANCE: Duration = Duration::from_millis(1500);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReconcileIssue {
    ContainerMismatch { expected: String, detected: String },
    DurationMismatch { catalogue_ms: u64, audio_ms: u64 },
    IsrcMismatch { catalogue: String, embedded: String },
    TitleMismatch { catalogue: String, embedded: String },
}

#[derive(Debug, Clone)]
pub struct AcquiredTrack {
    pub bytes: Bytes,
    pub analysis: audio::AudioAnalysis,
    pub catalogue: catalogue::TrackTagModel,
    pub issues: Vec<ReconcileIssue>,
    pub tagged: bool,
}

#[derive(Debug, Error)]
pub enum AcquireError {
    #[error(transparent)]
    Deezer(#[from] catalogue::DeezerError),
    #[error(transparent)]
    Audio(#[from] audio::AudioMetadataError),
    #[error("track {0} is unavailable")]
    Unavailable(String),
}

pub fn reconcile(analysis: &audio::AudioAnalysis, catalogue: &catalogue::TrackTagModel, resolved_format: &str) -> Vec<ReconcileIssue> {
    let mut issues = Vec::new();
    let expected = if resolved_format == "FLAC" {
        Some(audio::AudioFormatId::Flac)
    } else if resolved_format.starts_with("MP3") {
        Some(audio::AudioFormatId::Mp3)
    } else {
        None
    };
    if let Some(expected) = expected {
        if analysis.format.id != expected {
            issues.push(ReconcileIssue::ContainerMismatch {
                expected: expected.as_str().to_string(),
                detected: analysis.format.id.as_str().to_string(),
            });
        }
    }
    if let Some(duration) = analysis.format.duration_seconds {
        let audio_ms = (duration * 1000.0).round() as u64;
        let delta = audio_ms.abs_diff(catalogue.duration_ms);
        if catalogue.duration_ms > 0 && delta > DURATION_TOLERANCE.as_millis() as u64 {
            issues.push(ReconcileIssue::DurationMismatch {
                catalogue_ms: catalogue.duration_ms,
                audio_ms,
            });
        }
    }
    if let (Some(catalogue_isrc), Some(embedded)) = (&catalogue.isrc, &analysis.tags.isrc) {
        if normalize_isrc(catalogue_isrc) != normalize_isrc(embedded) {
            issues.push(ReconcileIssue::IsrcMismatch {
                catalogue: catalogue_isrc.clone(),
                embedded: embedded.clone(),
            });
        }
    }
    if let Some(embedded) = &analysis.tags.title {
        if normalize_title(embedded) != normalize_title(&catalogue.title) {
            issues.push(ReconcileIssue::TitleMismatch {
                catalogue: catalogue.title.clone(),
                embedded: embedded.clone(),
            });
        }
    }
    issues
}

fn normalize_isrc(value: &str) -> String {
    value.chars().filter(|ch| *ch != '-').collect::<String>().to_ascii_uppercase()
}

fn normalize_title(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ").to_ascii_lowercase()
}

fn tagging_blocked(issues: &[ReconcileIssue]) -> bool {
    issues.iter().any(|issue| matches!(issue, ReconcileIssue::ContainerMismatch { .. }))
}

pub async fn acquire(session: &catalogue::Session, track_id: &str, quality: catalogue::Quality) -> Result<AcquiredTrack, AcquireError> {
    let track = session.get_track_info(track_id).await?;
    let mut refreshed = catalogue::refresh_track_tokens(session, vec![track], 300).await?;
    let track = refreshed.pop().ok_or_else(|| AcquireError::Unavailable(track_id.to_string()))?;
    let bytes = catalogue::download_track_bytes(session, &track, &quality)
        .await?
        .ok_or_else(|| AcquireError::Unavailable(track_id.to_string()))?;
    let model = catalogue::resolve_tag_model(session, track, &catalogue::TagOptions::default()).await?;
    acquire_from_bytes(bytes, &quality.format_name(), model).await
}

pub async fn acquire_from_bytes(
    bytes: Bytes,
    resolved_format: &str,
    catalogue_model: catalogue::TrackTagModel,
) -> Result<AcquiredTrack, AcquireError> {
    let analysis = audio::analyze_audio(bytes.clone(), audio::AudioHints::default(), Default::default()).await?;
    let issues = reconcile(&analysis, &catalogue_model, resolved_format);
    if tagging_blocked(&issues) || !matches!(analysis.format.id, audio::AudioFormatId::Mp3 | audio::AudioFormatId::Flac) {
        return Ok(AcquiredTrack {
            bytes,
            analysis,
            catalogue: catalogue_model,
            issues,
            tagged: false,
        });
    }
    let tagged = tokio::task::spawn_blocking({
        let audio_bytes = bytes.to_vec();
        let model = catalogue_model.clone();
        move || catalogue::add_track_tags(&audio_bytes, &model)
    })
    .await
    .map_err(|err| catalogue::DeezerError::Message(err.to_string()))??;
    Ok(AcquiredTrack {
        bytes: Bytes::from(tagged),
        analysis,
        catalogue: catalogue_model,
        issues,
        tagged: true,
    })
}

pub async fn analyze_preview(session: &catalogue::Session, track: &catalogue::Track) -> Result<audio::AudioAnalysis, AcquireError> {
    let preview = catalogue::get_track_preview(session, track).await?;
    let Some(preview) = preview else {
        return Err(AcquireError::Unavailable(track.sng_id()));
    };
    let bytes = catalogue::download_preview(session, track).await?.ok_or_else(|| AcquireError::Unavailable(track.sng_id()))?;
    let _ = preview;
    Ok(audio::analyze_audio(bytes, audio::AudioHints { mime_type: Some("audio/mpeg".into()), ..audio::AudioHints::default() }, Default::default()).await?)
}
