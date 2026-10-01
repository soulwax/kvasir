use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde_json::Value;

use crate::error::DeezerError;

#[derive(Clone, Debug)]
pub struct MbArtistCredit {
    pub name: String,
    pub mbid: Option<String>,
    pub join_phrase: Option<String>,
}

#[derive(Clone, Debug)]
pub struct MbRelease {
    pub mbid: String,
    pub title: String,
    pub date: Option<String>,
    pub country: Option<String>,
    pub status: Option<String>,
    pub barcode: Option<String>,
    pub release_group_mbid: Option<String>,
    pub primary_type: Option<String>,
    pub label: Option<String>,
    pub catalog_number: Option<String>,
}

#[derive(Clone, Debug)]
pub struct MbRecording {
    pub mbid: String,
    pub title: String,
    pub disambiguation: Option<String>,
    pub length_ms: Option<u64>,
    pub isrcs: Vec<String>,
    pub score: Option<u64>,
    pub artist_credit: Vec<MbArtistCredit>,
    pub artist: String,
    pub first_release_date: Option<String>,
    pub releases: Vec<MbRelease>,
}

#[derive(Clone, Debug)]
pub struct CoverArtImage {
    pub id: String,
    pub front: bool,
    pub approved: bool,
    pub image: String,
    pub thumbnails: Vec<(u32, String)>,
}

struct Polite {
    user_agent: String,
    min_interval: Duration,
    last_at: Option<Instant>,
}

fn state() -> std::sync::MutexGuard<'static, Polite> {
    static STATE: Mutex<Polite> = Mutex::new(Polite {
        user_agent: String::new(),
        min_interval: Duration::from_millis(1100),
        last_at: None,
    });
    STATE.lock().expect("musicbrainz")
}

pub fn configure_musicbrainz(user_agent: &str, min_interval_ms: Option<u64>) {
    let mut polite = state();
    if user_agent.trim().is_empty() {
        return;
    }
    polite.user_agent = user_agent.to_string();
    if let Some(ms) = min_interval_ms {
        polite.min_interval = Duration::from_millis(ms);
    }
}

async fn polite_get(url: &str) -> Result<Option<Value>, DeezerError> {
    let (user_agent, wait) = {
        let mut polite = state();
        if polite.user_agent.trim().is_empty() {
            return Err(DeezerError::Message("configure_musicbrainz requires a descriptive user agent".into()));
        }
        let wait = polite.last_at.map(|at| polite.min_interval.saturating_sub(at.elapsed())).unwrap_or_default();
        polite.last_at = Some(Instant::now() + wait);
        (polite.user_agent.clone(), wait)
    };
    if !wait.is_zero() {
        tokio::time::sleep(wait).await;
    }
    for attempt in 0..3 {
        match crate::http::get_json(url, &[("User-Agent", user_agent.clone()), ("Accept", "application/json".into())]).await {
            Ok(value) => return Ok(Some(value)),
            Err(DeezerError::HttpStatus { status: 404, .. }) => return Ok(None),
            Err(DeezerError::HttpStatus { status: 429 | 503, .. }) if attempt < 2 => {
                tokio::time::sleep(Duration::from_millis(400 * (attempt as u64 + 1))).await;
            }
            Err(err) => return Err(err),
        }
    }
    Err(DeezerError::Message("enrichment endpoint stayed busy".into()))
}

fn map_release(raw: &Value) -> MbRelease {
    MbRelease {
        mbid: raw.get("id").and_then(Value::as_str).unwrap_or("").to_string(),
        title: raw.get("title").and_then(Value::as_str).unwrap_or("").to_string(),
        date: raw.get("date").and_then(Value::as_str).map(str::to_string),
        country: raw.get("country").and_then(Value::as_str).map(str::to_string),
        status: raw.get("status").and_then(Value::as_str).map(str::to_string),
        barcode: raw.get("barcode").and_then(Value::as_str).map(str::to_string),
        release_group_mbid: raw.pointer("/release-group/id").and_then(Value::as_str).map(str::to_string),
        primary_type: raw.pointer("/release-group/primary-type").and_then(Value::as_str).map(str::to_string),
        label: raw.pointer("/label-info/0/label/name").and_then(Value::as_str).map(str::to_string),
        catalog_number: raw.pointer("/label-info/0/catalog-number").and_then(Value::as_str).map(str::to_string),
    }
}

fn map_recording(raw: &Value, fallback_isrc: Option<&str>) -> MbRecording {
    let credits = raw.get("artist-credit").and_then(Value::as_array).map(|items| {
        items.iter().map(|credit| MbArtistCredit {
            name: credit.get("name").and_then(Value::as_str).or_else(|| credit.pointer("/artist/name").and_then(Value::as_str)).unwrap_or("").to_string(),
            mbid: credit.pointer("/artist/id").and_then(Value::as_str).map(str::to_string),
            join_phrase: credit.get("joinphrase").and_then(Value::as_str).map(str::to_string),
        }).collect::<Vec<_>>()
    }).unwrap_or_default();
    let artist = credits.iter().enumerate().map(|(index, credit)| {
        let join = if index + 1 == credits.len() { "" } else { credit.join_phrase.as_deref().unwrap_or(", ") };
        format!("{}{join}", credit.name)
    }).collect::<String>();
    let releases = raw.get("releases").and_then(Value::as_array).map(|items| items.iter().map(map_release).collect::<Vec<_>>()).unwrap_or_default();
    let first_release_date = releases.iter().filter_map(|release| release.date.clone()).min();
    MbRecording {
        mbid: raw.get("id").and_then(Value::as_str).unwrap_or("").to_string(),
        title: raw.get("title").and_then(Value::as_str).unwrap_or("").to_string(),
        disambiguation: raw.get("disambiguation").and_then(Value::as_str).map(str::to_string),
        length_ms: raw.get("length").and_then(Value::as_u64),
        isrcs: raw.get("isrcs").and_then(Value::as_array).map(|items| items.iter().filter_map(|item| item.as_str().map(str::to_string)).collect()).unwrap_or_else(|| fallback_isrc.map(|isrc| vec![isrc.to_string()]).unwrap_or_default()),
        score: raw.get("score").and_then(Value::as_u64),
        artist_credit: credits,
        artist,
        first_release_date,
        releases,
    }
}

pub async fn lookup_recording_by_isrc(isrc: &str) -> Result<Option<MbRecording>, DeezerError> {
    let url = format!("https://musicbrainz.org/ws/2/recording?query=isrc:{isrc}&fmt=json&limit=5&inc=releases");
    let Some(data) = polite_get(&url).await? else { return Ok(None) };
    Ok(data.pointer("/recordings/0").map(|raw| map_recording(raw, Some(isrc))))
}

pub async fn get_musicbrainz_recording(mbid: &str) -> Result<Option<MbRecording>, DeezerError> {
    let url = format!("https://musicbrainz.org/ws/2/recording/{mbid}?fmt=json&inc=artist-credits+isrcs+releases");
    Ok(polite_get(&url).await?.map(|raw| map_recording(&raw, None)))
}

pub async fn get_musicbrainz_release(mbid: &str) -> Result<Option<MbRelease>, DeezerError> {
    let url = format!("https://musicbrainz.org/ws/2/release/{mbid}?fmt=json&inc=labels+release-groups");
    Ok(polite_get(&url).await?.map(|raw| map_release(&raw)))
}

pub async fn get_cover_art(mbid: &str, entity: &str) -> Result<Option<Vec<CoverArtImage>>, DeezerError> {
    let url = format!("https://coverartarchive.org/{entity}/{mbid}");
    let Some(data) = polite_get(&url).await? else { return Ok(None) };
    let images = data.get("images").and_then(Value::as_array).map(|items| {
        items.iter().map(|image| {
            let thumbnails = image.get("thumbnails").and_then(Value::as_object).map(|map| {
                map.iter().filter_map(|(key, url)| Some((key.parse().ok()?, url.as_str()?.to_string()))).collect()
            }).unwrap_or_default();
            CoverArtImage {
                id: image.get("id").map(|value| match value { Value::String(text) => text.clone(), Value::Number(number) => number.to_string(), _ => String::new() }).unwrap_or_default(),
                front: image.get("front").and_then(Value::as_bool).unwrap_or(false),
                approved: image.get("approved").and_then(Value::as_bool).unwrap_or(false),
                image: image.get("image").and_then(Value::as_str).unwrap_or("").to_string(),
                thumbnails,
            }
        }).collect()
    });
    Ok(images)
}

pub async fn get_best_cover_art_url(mbid: &str, entity: &str, min_size: u32) -> Result<Option<String>, DeezerError> {
    let Some(images) = get_cover_art(mbid, entity).await? else { return Ok(None) };
    let front = images.iter().find(|image| image.front && image.approved).or_else(|| images.iter().find(|image| image.front)).or(images.first());
    let Some(front) = front else { return Ok(None) };
    let sized = front.thumbnails.iter().filter(|(size, _)| *size >= min_size).min_by_key(|(size, _)| *size);
    Ok(Some(sized.map(|(_, url)| url.clone()).unwrap_or_else(|| front.image.clone())))
}

pub async fn get_cover_art_by_isrc(isrc: &str, min_size: u32, max_tries: usize) -> Result<Option<String>, DeezerError> {
    let Some(recording) = lookup_recording_by_isrc(isrc).await? else { return Ok(None) };
    let mut seen = Vec::new();
    let mut ranked: Vec<(i32, String, String)> = recording.releases.iter().filter_map(|release| {
        let group = release.release_group_mbid.clone()?;
        if seen.contains(&group) { return None; }
        seen.push(group.clone());
        let score = (if release.status.as_deref() == Some("Official") { 0 } else if release.status.is_some() { 10 } else { 5 })
            + if release.primary_type.as_deref() == Some("Album") { 0 } else if release.primary_type.is_some() { 2 } else { 1 };
        Some((score, release.date.clone().unwrap_or_else(|| "9999".into()), group))
    }).collect();
    ranked.sort_by(|left, right| left.0.cmp(&right.0).then(left.1.cmp(&right.1)));
    for (_, _, group) in ranked.into_iter().take(max_tries) {
        if let Some(url) = get_best_cover_art_url(&group, "release-group", min_size).await? {
            return Ok(Some(url));
        }
    }
    Ok(None)
}
