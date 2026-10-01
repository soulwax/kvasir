use std::sync::Mutex;

use serde_json::Value;

use crate::error::DeezerError;
use crate::http::get_text;
use crate::track::Track;

struct Latch {
    failures: u32,
    max_failures: u32,
    off: bool,
}

fn latch() -> std::sync::MutexGuard<'static, Latch> {
    static LATCH: Mutex<Latch> = Mutex::new(Latch {
        failures: 0,
        max_failures: 3,
        off: false,
    });
    LATCH.lock().expect("musixmatch latch")
}

pub fn configure_musixmatch(max_failures: Option<u32>, enabled: Option<bool>) {
    let mut state = latch();
    if let Some(max_failures) = max_failures.filter(|value| *value > 0) {
        state.max_failures = max_failures;
    }
    if let Some(enabled) = enabled {
        state.off = !enabled;
        if enabled {
            state.failures = 0;
        }
    }
}

pub fn musixmatch_status() -> (bool, u32, u32) {
    let state = latch();
    (!state.off, state.failures, state.max_failures)
}

fn note_transport_failure() {
    let mut state = latch();
    state.failures = state.failures.saturating_add(1);
    if state.failures >= state.max_failures {
        state.off = true;
    }
}

fn note_success() {
    latch().failures = 0;
}

#[cfg(feature = "lyrics-fallback")]
pub async fn fallback_lyrics(track: &Track) -> Result<String, DeezerError> {
    if latch().off {
        return Err(DeezerError::Message(
            "Musixmatch fallback is unavailable from this network (latched off)".into(),
        ));
    }
    let query = format!("{} {}", track.artist_name(), track.title());
    match scrape(&query).await {
        Ok(lyrics) => {
            note_success();
            Ok(lyrics)
        }
        Err(DeezerError::Message(message)) if message.starts_with("No ") && message.ends_with("found!") => {
            Err(DeezerError::Message(message))
        }
        Err(err) => {
            note_transport_failure();
            Err(err)
        }
    }
}

#[cfg(not(feature = "lyrics-fallback"))]
pub async fn fallback_lyrics(_track: &Track) -> Result<String, DeezerError> {
    Err(DeezerError::Message("lyrics-fallback feature is disabled".into()))
}

#[cfg(feature = "lyrics-fallback")]
async fn scrape(query: &str) -> Result<String, DeezerError> {
    let search = get_text(
        &format!("https://musixmatch.com/search/{}/tracks", urlencoding_query(query)),
        &[("User-Agent", "Mozilla/5.0".into()), ("referer", "https://l.facebook.com/".into())],
    )
    .await?;
    let href = search
        .split("href=\"")
        .skip(1)
        .map(|part| part.split('"').next().unwrap_or(""))
        .find(|href| href.contains("/lyrics/"))
        .ok_or_else(|| DeezerError::Message("No song found!".into()))?
        .replace("/add", "");
    let url = if href.starts_with("/lyrics/") {
        format!("https://musixmatch.com{href}")
    } else {
        href
    };
    let page = get_text(
        &url,
        &[("User-Agent", "Mozilla/5.0".into()), ("referer", "https://musixmatch.com/".into())],
    )
    .await?;
    let start = page.find("\"body\":\"").ok_or_else(|| DeezerError::Message("No lyrics found!".into()))? + "\"body\":\"".len();
    let end = page[start..].find("\",\"language\"").ok_or_else(|| DeezerError::Message("No lyrics found!".into()))?;
    Ok(page[start..start + end].replace("\\n", "\n"))
}

#[cfg(feature = "lyrics-fallback")]
fn urlencoding_query(query: &str) -> String {
    query
        .chars()
        .map(|ch| match ch {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => ch.to_string(),
            ' ' => "%20".into(),
            other => format!("%{:02X}", other as u32),
        })
        .collect()
}

#[allow(dead_code)]
fn _value_marker(value: &Value) -> bool {
    value.is_null()
}
