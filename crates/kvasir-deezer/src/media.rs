use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{json, Value};

use crate::decrypt::legacy_cdn_url;
use crate::error::DeezerError;
use crate::http::{content_length, HttpClient};
use crate::session::Session;
use crate::track::Track;

pub const DEEZER_FORMATS: &[&str] = &[
    "FLAC",
    "MP3_320",
    "MP3_256",
    "MP3_128",
    "MP3_64",
    "AAC_64",
    "MP4_RA3",
    "MP4_RA2",
    "MP4_RA1",
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Quality {
    Flac,
    Mp3_320,
    Mp3_128,
    Named(String),
}

impl Quality {
    pub fn from_code(code: i64) -> Result<Self, DeezerError> {
        match code {
            9 => Ok(Self::Flac),
            3 => Ok(Self::Mp3_320),
            1 => Ok(Self::Mp3_128),
            other => Err(DeezerError::Message(format!("Unknown quality {other}"))),
        }
    }

    pub fn parse(value: &str) -> Self {
        match value {
            "FLAC" | "9" => Self::Flac,
            "MP3_320" | "3" => Self::Mp3_320,
            "MP3_128" | "1" => Self::Mp3_128,
            other => Self::Named(other.to_string()),
        }
    }

    pub fn format_name(&self) -> String {
        match self {
            Self::Flac => "FLAC".into(),
            Self::Mp3_320 => "MP3_320".into(),
            Self::Mp3_128 => "MP3_128".into(),
            Self::Named(name) => name.clone(),
        }
    }

    pub fn legacy_code(&self) -> i64 {
        match self {
            Self::Flac => 9,
            Self::Mp3_320 => 3,
            Self::Mp3_128 => 1,
            Self::Named(name) => match name.as_str() {
                "FLAC" => 9,
                "MP3_320" => 3,
                "MP3_128" => 1,
                _ => 1,
            },
        }
    }
}

#[derive(Clone, Debug)]
pub struct ResolvedUrl {
    pub track_url: String,
    pub is_encrypted: bool,
    pub file_size: u64,
    pub format: String,
    pub cipher: String,
}

fn cipher_encrypted(cipher: &str, url: &str) -> bool {
    if !cipher.is_empty() {
        cipher != "NONE"
    } else {
        url.contains("/mobile/") || url.contains("/media/")
    }
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}

async fn media_get_url(session: &Session, tokens: &[String], formats: &[String]) -> Result<(Vec<Value>, String), DeezerError> {
    let mut last_error = None;
    for attempt in 0..4 {
        let user = session.load_user_data(attempt > 0).await?;
        let body = json!({
            "license_token": user.license_token,
            "media": [{
                "type": "FULL",
                "formats": formats.iter().map(|format| json!({"format": format, "cipher": "BF_CBC_STRIPE"})).collect::<Vec<_>>(),
            }],
            "track_tokens": tokens,
        });
        match HttpClient::new("", Vec::new(), Vec::new())
            .post_json("https://media.deezer.com/v1/get_url", &body, &[])
            .await
        {
            Ok(response) => {
                let data = response.json()?;
                let rows = data.get("data").and_then(Value::as_array).cloned().unwrap_or_default();
                return Ok((rows, user.country));
            }
            Err(DeezerError::HttpStatus { status, .. }) if matches!(status, 403 | 429) || status >= 500 => {
                last_error = Some(DeezerError::HttpStatus { status, body: String::new() });
                if attempt == 3 {
                    break;
                }
                let delay = 500u64.saturating_mul(1 << attempt);
                tokio::time::sleep(std::time::Duration::from_millis(delay)).await;
            }
            Err(err) => return Err(err),
        }
    }
    Err(last_error.unwrap_or_else(|| DeezerError::Message("media API failed".into())))
}

fn parse_media_entry(entry: &Value, token: &str, country: &str) -> Result<Option<(String, String, String)>, DeezerError> {
    if let Some(errors) = entry.get("errors").and_then(Value::as_array) {
        let code = errors.first().and_then(|item| item.get("code")).and_then(Value::as_i64);
        return match code {
            Some(2002) => Err(DeezerError::GeoBlocked(country.to_string())),
            Some(2000 | 2001) => Err(DeezerError::ExpiredTrackToken(token.to_string())),
            _ => Err(DeezerError::gateway(errors.first().cloned().unwrap_or(json!({})))),
        };
    }
    let media = entry.pointer("/media/0");
    let url = media
        .and_then(|item| item.pointer("/sources/0/url"))
        .and_then(Value::as_str)
        .unwrap_or("");
    if url.is_empty() {
        return Ok(None);
    }
    let format = media.and_then(|item| item.get("format")).and_then(Value::as_str).unwrap_or("").to_string();
    let cipher = media
        .and_then(|item| item.pointer("/cipher/type"))
        .and_then(Value::as_str)
        .unwrap_or("BF_CBC_STRIPE")
        .to_string();
    Ok(Some((url.to_string(), format, cipher)))
}

pub async fn get_track_download_url(session: &Session, track: &Track, quality: &Quality) -> Result<Option<ResolvedUrl>, DeezerError> {
    let format = quality.format_name();
    let mut wrong_license = None;
    let mut geo_blocked = None;
    let mut expired = None;
    let mut media_blocked = None;
    let token_stale = track.track_token_expire() > 0 && track.track_token_expire() * 1000 < now_ms();
    if token_stale {
        expired = Some(DeezerError::ExpiredTrackToken(track.sng_id()));
    } else if !track.track_token().is_empty() {
        let user = session.load_user_data(false).await?;
        if (format == "FLAC" && !user.can_stream_lossless) || (format == "MP3_320" && !user.can_stream_hq) {
            wrong_license = Some(DeezerError::WrongLicense(format.clone()));
        } else {
            match media_get_url(session, &[track.track_token()], &[format.clone()]).await {
                Ok((rows, country)) => {
                    if let Some(entry) = rows.first() {
                        match parse_media_entry(entry, &track.track_token(), &country) {
                            Ok(Some((url, resolved_format, cipher))) => {
                                return Ok(Some(ResolvedUrl {
                                    is_encrypted: cipher_encrypted(&cipher, &url),
                                    file_size: track.filesize(&format),
                                    track_url: url,
                                    format: resolved_format,
                                    cipher,
                                }));
                            }
                            Ok(None) => {}
                            Err(err @ DeezerError::WrongLicense(_)) => wrong_license = Some(err),
                            Err(err @ DeezerError::GeoBlocked(_)) => geo_blocked = Some(err),
                            Err(err @ DeezerError::ExpiredTrackToken(_)) => expired = Some(err),
                            Err(err) => return Err(err),
                        }
                    }
                }
                Err(err @ DeezerError::HttpStatus { status: 403 | 429, .. }) => media_blocked = Some(err),
                Err(err) => return Err(err),
            }
        }
    }

    if !track.md5_origin().is_empty() {
        let url = legacy_cdn_url(&track.md5_origin(), quality.legacy_code(), &track.sng_id(), &track.media_version())?;
        if let Ok(response) = HttpClient::new("", Vec::new(), Vec::new()).head(&url).await {
            let file_size = content_length(&response.headers);
            if file_size > 0 {
                return Ok(Some(ResolvedUrl {
                    is_encrypted: cipher_encrypted("", &url),
                    track_url: url,
                    file_size,
                    format,
                    cipher: "BF_CBC_STRIPE".into(),
                }));
            }
        }
    }
    if let Some(err) = wrong_license.or(geo_blocked).or(expired).or(media_blocked) {
        return Err(err);
    }
    Ok(None)
}

pub async fn resolve_download_urls(session: &Session, tracks: &[Track], qualities: &[Quality]) -> Result<Vec<Option<ResolvedUrl>>, DeezerError> {
    if tracks.is_empty() {
        return Ok(Vec::new());
    }
    let formats: Vec<String> = qualities.iter().map(Quality::format_name).collect();
    let tokens: Vec<String> = tracks.iter().map(Track::track_token).collect();
    let (rows, country) = media_get_url(session, &tokens, &formats).await?;
    Ok(tracks
        .iter()
        .enumerate()
        .map(|(index, track)| {
            let entry = rows.get(index)?;
            let (url, format, cipher) = parse_media_entry(entry, &track.track_token(), &country).ok()??;
            Some(ResolvedUrl {
                is_encrypted: cipher_encrypted(&cipher, &url),
                file_size: track.filesize(&format),
                track_url: url,
                format,
                cipher,
            })
        })
        .collect())
}

pub async fn refresh_track_tokens(session: &Session, tracks: Vec<Track>, grace_seconds: i64) -> Result<Vec<Track>, DeezerError> {
    let now = now_ms();
    let grace_ms = grace_seconds * 1000;
    let stale: Vec<String> = tracks
        .iter()
        .filter(|track| {
            track.track_token().is_empty()
                || track.track_token_expire() == 0
                || track.track_token_expire() * 1000 - now < grace_ms
        })
        .map(Track::sng_id)
        .collect();
    if stale.is_empty() {
        return Ok(tracks);
    }
    let refreshed = session.gw(json!({"sng_ids": stale}), "song.getListData").await?;
    let rows = refreshed.get("data").and_then(Value::as_array).cloned().unwrap_or_default();
    Ok(tracks
        .into_iter()
        .map(|track| {
            rows.iter().find_map(|row| {
                let id = row.get("SNG_ID").map(|value| match value {
                    Value::String(text) => text.clone(),
                    Value::Number(number) => number.to_string(),
                    _ => String::new(),
                })?;
                if id != track.sng_id() {
                    return None;
                }
                let token = row.get("TRACK_TOKEN").and_then(Value::as_str).unwrap_or("");
                let expire = row
                    .get("TRACK_TOKEN_EXPIRE")
                    .and_then(Value::as_i64)
                    .or_else(|| row.get("TRACK_TOKEN_EXPIRE").and_then(Value::as_str).and_then(|value| value.parse().ok()))
                    .unwrap_or(0);
                Some(track.clone().with_tokens(token, expire))
            }).unwrap_or(track)
        })
        .collect())
}
