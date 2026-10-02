use std::pin::Pin;

use bytes::Bytes;
use futures_core::Stream;
use futures_util::StreamExt;

use crate::decrypt::TrackDecryptor;
use crate::error::DeezerError;
use crate::http::byte_stream;
use crate::media::{get_track_download_url, Quality, ResolvedUrl};
use crate::session::Session;
use crate::track::Track;

const CHUNK: u64 = 2048;

pub struct TrackStream {
    pub stream: Pin<Box<dyn Stream<Item = Result<Bytes, DeezerError>> + Send>>,
    pub size: u64,
    pub started_at: u64,
    pub is_encrypted: bool,
    pub resolved: ResolvedUrl,
}

pub async fn open_download(
    session: &Session,
    track: &Track,
    quality: &Quality,
    resume_from: u64,
) -> Result<TrackStream, DeezerError> {
    let resolved = get_track_download_url(session, track, quality)
        .await?
        .ok_or_else(|| {
            DeezerError::Message(format!(
                "Track {} is unavailable at quality {}",
                track.sng_id(),
                quality.format_name()
            ))
        })?;
    let started_at = if resume_from == 0 {
        0
    } else {
        (resume_from / CHUNK) * CHUNK
    };
    let (incoming, content_length) = byte_stream(&resolved.track_url, started_at).await?;
    let size = if resolved.file_size > 0 {
        resolved.file_size
    } else if content_length > 0 {
        content_length + started_at
    } else {
        0
    };
    let song_id = track.sng_id();
    let encrypted = resolved.is_encrypted;
    let stream = decrypting_stream(incoming, song_id, encrypted, started_at / CHUNK);
    Ok(TrackStream {
        stream,
        size,
        started_at,
        is_encrypted: encrypted,
        resolved,
    })
}

fn decrypting_stream(
    mut incoming: Pin<Box<dyn Stream<Item = Result<Bytes, DeezerError>> + Send>>,
    song_id: String,
    encrypted: bool,
    start_chunk: u64,
) -> Pin<Box<dyn Stream<Item = Result<Bytes, DeezerError>> + Send>> {
    Box::pin(async_stream::stream! {
        let mut decryptor = encrypted.then(|| TrackDecryptor::new(&song_id, start_chunk));
        while let Some(item) = incoming.next().await {
            let chunk = item?;
            if let Some(current) = decryptor.as_mut() {
                let produced = current.push(&chunk);
                if !produced.is_empty() {
                    yield Ok(Bytes::from(produced));
                }
            } else {
                yield Ok(chunk);
            }
        }
        if let Some(mut current) = decryptor {
            let rest = current.finish();
            if !rest.is_empty() {
                yield Ok(Bytes::from(rest));
            }
        }
    })
}

pub async fn download_track_bytes(session: &Session, track: &Track, quality: &Quality) -> Result<Option<Bytes>, DeezerError> {
    let download = match open_download(session, track, quality, 0).await {
        Err(DeezerError::Message(message)) if message.contains("unavailable") => return Ok(None),
        Err(err) => return Err(err),
        Ok(download) => download,
    };
    let mut stream = download.stream;
    let mut output = Vec::new();
    if (1..=128 * 1024 * 1024).contains(&download.size) {
        output.reserve(download.size as usize);
    }
    while let Some(chunk) = stream.next().await {
        output.extend_from_slice(&chunk?);
    }
    Ok(Some(Bytes::from(output)))
}
