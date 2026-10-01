use serde_json::{json, Value};

use crate::api::{get_album_by_upc, get_track_by_isrc};
use crate::contributors::append_version;
use crate::error::DeezerError;
use crate::http::HttpClient;
use crate::session::Session;
use crate::track::{tracks_from_list, Track};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UrlParts {
    pub kind: String,
    pub id: String,
}

#[derive(Clone, Debug)]
pub struct ParsedLink {
    pub info: UrlParts,
    pub link_type: String,
    pub link_info: Value,
    pub tracks: Vec<Track>,
}

pub async fn get_url_parts(url: &str) -> Result<UrlParts, DeezerError> {
    let mut url = if let Some(rest) = url.strip_prefix("spotify:") {
        let mut parts = rest.split(':');
        let kind = parts.next().unwrap_or("");
        let id = parts.next().unwrap_or("");
        format!("https://open.spotify.com/{kind}/{id}")
    } else {
        url.to_string()
    };
    let site = if url.contains("deezer") {
        "deezer"
    } else if url.contains("spotify") {
        "spotify"
    } else if url.contains("tidal") {
        "tidal"
    } else if url.contains("youtu.be") {
        "youtu.be"
    } else if url.contains("youtube") || url.contains("youtu") {
        "youtube"
    } else {
        return Err(DeezerError::Message(format!("Unknown URL: {url}")));
    };
    match site {
        "deezer" => {
            if url.contains("page.link") {
                let response = HttpClient::new("", Vec::new(), Vec::new()).head(&url).await?;
                url = response.final_url;
            }
            let (kind, id) = split_deezer(&url)?;
            Ok(UrlParts { kind, id })
        }
        "spotify" => {
            let (kind, id) = split_service(&url, &["track", "album", "playlist", "artist"])?;
            Ok(UrlParts { kind: format!("spotify-{kind}"), id })
        }
        "tidal" => {
            let (kind, id) = split_service(&url, &["track", "album", "playlist", "artist"])?;
            Ok(UrlParts { kind: format!("tidal-{kind}"), id })
        }
        "youtube" => {
            let id = url.split("v=").nth(1).unwrap_or("").split('&').next().unwrap_or("").to_string();
            if id.is_empty() {
                return Err(DeezerError::Message(format!("Unable to parse URL: {url}")));
            }
            Ok(UrlParts { kind: "youtube-track".into(), id })
        }
        "youtu.be" => {
            let id = url.trim_end_matches('/').rsplit('/').next().unwrap_or("").split('?').next().unwrap_or("").to_string();
            Ok(UrlParts { kind: "youtube-track".into(), id })
        }
        _ => Err(DeezerError::Message(format!("Unable to parse URL: {url}"))),
    }
}

fn split_deezer(url: &str) -> Result<(String, String), DeezerError> {
    for kind in ["track", "album", "audiobook", "playlist", "artist"] {
        if let Some(index) = url.find(&format!("/{kind}/")) {
            let id = url[index + kind.len() + 2..].split(['/', '?']).next().unwrap_or("");
            if !id.is_empty() && id.chars().all(|ch| ch.is_ascii_digit()) {
                return Ok((kind.to_string(), id.to_string()));
            }
        }
    }
    Err(DeezerError::Message(format!("Unable to parse URL: {url}")))
}

fn split_service(url: &str, kinds: &[&str]) -> Result<(String, String), DeezerError> {
    for kind in kinds {
        let marker = format!("/{kind}/");
        if let Some(index) = url.find(&marker) {
            let id = url[index + marker.len()..].split(['/', '?']).next().unwrap_or("");
            if !id.is_empty() {
                return Ok(((*kind).to_string(), id.to_string()));
            }
        }
    }
    Err(DeezerError::Message(format!("Unable to parse URL: {url}")))
}

pub async fn isrc_to_deezer(session: &Session, name: &str, isrc: Option<&str>) -> Result<Track, DeezerError> {
    let Some(isrc) = isrc.filter(|value| !value.is_empty()) else {
        return Err(DeezerError::Message(format!("ISRC code not found for {name}")));
    };
    let data = get_track_by_isrc(isrc).await.map_err(|_| DeezerError::Message(format!("No match on deezer for {name} (ISRC: {isrc})")))?;
    if data.get("error").is_some() {
        return Err(DeezerError::Message(format!("No match on deezer for {name} (ISRC: {isrc})")));
    }
    let id = data.get("id").map(|value| match value {
        Value::String(text) => text.clone(),
        Value::Number(number) => number.to_string(),
        _ => String::new(),
    }).unwrap_or_default();
    session.get_track_info(&id).await
}

pub async fn upc_to_deezer(session: &Session, name: &str, upc: Option<&str>) -> Result<(Value, Vec<Track>), DeezerError> {
    let Some(upc) = upc.filter(|value| !value.is_empty()) else {
        return Err(DeezerError::Message(format!("UPC code not found for {name}")));
    };
    let album = get_album_by_upc(upc).await.map_err(|_| DeezerError::Message(format!("No match on deezer for {name} (UPC: {upc})")))?;
    let id = album.get("id").map(|value| match value {
        Value::String(text) => text.clone(),
        Value::Number(number) => number.to_string(),
        _ => String::new(),
    }).unwrap_or_default();
    let info = session.get_album_info(&id).await?;
    let tracks = tracks_from_list(&session.get_album_tracks(&id).await?);
    Ok((info, tracks))
}

pub async fn parse_info(session: &Session, url: &str) -> Result<ParsedLink, DeezerError> {
    let info = get_url_parts(url).await?;
    if info.id.is_empty() {
        return Err(DeezerError::Message("Unable to parse id".into()));
    }
    let mut link_type = "track".to_string();
    let mut link_info = json!({});
    let mut tracks = Vec::new();
    match info.kind.as_str() {
        "track" => tracks.push(session.get_track_info(&info.id).await?),
        "album" | "audiobook" => {
            link_info = session.get_album_info(&info.id).await?;
            link_type = "album".into();
            tracks = tracks_from_list(&session.get_album_tracks(&info.id).await?);
        }
        "playlist" => {
            link_info = session.get_playlist_info(&info.id).await?;
            link_type = "playlist".into();
            tracks = tracks_from_list(&session.get_playlist_tracks(&info.id).await?);
        }
        "artist" => {
            link_info = session.get_artist_info(&info.id).await?;
            link_type = "artist".into();
            let discography = session.get_discography(&info.id, 500).await?;
            for album in discography.get("data").and_then(Value::as_array).into_iter().flatten() {
                let artists = album.get("ARTISTS").and_then(Value::as_array);
                let credited = artists.is_some_and(|items| items.iter().any(|artist| artist.get("ART_ID").and_then(Value::as_str) == Some(info.id.as_str()) || artist.get("ART_ID").and_then(Value::as_i64).map(|id| id.to_string()) == Some(info.id.clone())));
                if !credited {
                    continue;
                }
                let album_id = album.get("ALB_ID").map(|value| match value {
                    Value::String(text) => text.clone(),
                    Value::Number(number) => number.to_string(),
                    _ => String::new(),
                }).unwrap_or_default();
                let album_tracks = tracks_from_list(&session.get_album_tracks(&album_id).await?);
                tracks.extend(album_tracks.into_iter().filter(|track| track.artist_id() == info.id));
            }
        }
        "spotify-track" => tracks.push(spotify_track(session, &info.id).await?),
        "spotify-album" => {
            let (album, album_tracks) = spotify_album(session, &info.id).await?;
            link_info = album;
            tracks = album_tracks;
            link_type = "album".into();
        }
        "spotify-playlist" => {
            let (playlist, playlist_tracks) = spotify_playlist(session, &info.id).await?;
            link_info = playlist;
            tracks = playlist_tracks;
            link_type = "playlist".into();
        }
        "spotify-artist" => {
            tracks = spotify_artist(session, &info.id).await?;
            link_type = "artist".into();
        }
        "tidal-track" => tracks.push(tidal_track(session, &info.id).await?),
        "tidal-album" => {
            let (album, album_tracks) = tidal_album(session, &info.id).await?;
            link_info = album;
            tracks = album_tracks;
            link_type = "album".into();
        }
        "tidal-playlist" => {
            let (playlist, playlist_tracks) = tidal_playlist(session, &info.id).await?;
            link_info = playlist;
            tracks = playlist_tracks;
            link_type = "playlist".into();
        }
        "tidal-artist" => {
            tracks = tidal_artist(session, &info.id).await?;
            link_type = "artist".into();
        }
        "youtube-track" => tracks.push(youtube_track(session, &info.id).await?),
        other => return Err(DeezerError::Message(format!("Unknown type: {other}"))),
    }
    for track in &mut tracks {
        append_version(track);
    }
    Ok(ParsedLink { info, link_type, link_info, tracks })
}

#[cfg(feature = "resolve-spotify")]
async fn spotify_token() -> Result<String, DeezerError> {
    let data = crate::http::get_json("https://open.spotify.com/get_access_token?reason=transport&productType=embed", &[]).await?;
    data.get("accessToken").and_then(serde_json::Value::as_str).map(str::to_string).ok_or_else(|| DeezerError::Message("spotify anonymous token missing".into()))
}

#[cfg(feature = "resolve-spotify")]
async fn spotify_get(path: &str) -> Result<Value, DeezerError> {
    let token = spotify_token().await?;
    crate::http::get_json(&format!("https://api.spotify.com/v1/{path}"), &[("Authorization", format!("Bearer {token}"))]).await
}

#[cfg(feature = "resolve-spotify")]
async fn spotify_track(session: &Session, id: &str) -> Result<Track, DeezerError> {
    let track = spotify_get(&format!("tracks/{id}")).await?;
    let name = track.get("name").and_then(Value::as_str).unwrap_or("");
    let isrc = track.pointer("/external_ids/isrc").and_then(Value::as_str);
    isrc_to_deezer(session, name, isrc).await
}

#[cfg(feature = "resolve-spotify")]
async fn spotify_album(session: &Session, id: &str) -> Result<(Value, Vec<Track>), DeezerError> {
    let album = spotify_get(&format!("albums/{id}")).await?;
    let name = album.get("name").and_then(Value::as_str).unwrap_or("");
    let upc = album.pointer("/external_ids/upc").and_then(Value::as_str);
    upc_to_deezer(session, name, upc).await
}

#[cfg(feature = "resolve-spotify")]
async fn spotify_playlist(session: &Session, id: &str) -> Result<(Value, Vec<Track>), DeezerError> {
    let playlist = spotify_get(&format!("playlists/{id}")).await?;
    let mut tracks = Vec::new();
    let mut items = playlist.pointer("/tracks/items").and_then(Value::as_array).cloned().unwrap_or_default();
    let mut next = playlist.pointer("/tracks/next").and_then(Value::as_str).map(str::to_string);
    while let Some(url) = next.take() {
        let token = spotify_token().await?;
        let page = crate::http::get_json(&url, &[("Authorization", format!("Bearer {token}"))]).await?;
        if let Some(more) = page.get("items").and_then(Value::as_array) {
            items.extend(more.iter().cloned());
        }
        next = page.get("next").and_then(Value::as_str).map(str::to_string);
    }
    for item in items {
        let Some(track) = item.get("track") else { continue };
        let name = track.get("name").and_then(Value::as_str).unwrap_or("");
        let isrc = track.pointer("/external_ids/isrc").and_then(Value::as_str);
        if let Ok(track) = isrc_to_deezer(session, name, isrc).await {
            tracks.push(track);
        }
    }
    Ok((playlist, tracks))
}

#[cfg(feature = "resolve-spotify")]
async fn spotify_artist(session: &Session, id: &str) -> Result<Vec<Track>, DeezerError> {
    let body = spotify_get(&format!("artists/{id}/top-tracks?market=GB")).await?;
    let mut tracks = Vec::new();
    for track in body.get("tracks").and_then(Value::as_array).into_iter().flatten().take(10) {
        let name = track.get("name").and_then(Value::as_str).unwrap_or("");
        let isrc = track.pointer("/external_ids/isrc").and_then(Value::as_str);
        if let Ok(track) = isrc_to_deezer(session, name, isrc).await {
            tracks.push(track);
        }
    }
    Ok(tracks)
}

#[cfg(not(feature = "resolve-spotify"))]
async fn spotify_track(_session: &Session, _id: &str) -> Result<Track, DeezerError> {
    Err(DeezerError::Message("resolve-spotify feature is disabled".into()))
}
#[cfg(not(feature = "resolve-spotify"))]
async fn spotify_album(_session: &Session, _id: &str) -> Result<(Value, Vec<Track>), DeezerError> {
    Err(DeezerError::Message("resolve-spotify feature is disabled".into()))
}
#[cfg(not(feature = "resolve-spotify"))]
async fn spotify_playlist(_session: &Session, _id: &str) -> Result<(Value, Vec<Track>), DeezerError> {
    Err(DeezerError::Message("resolve-spotify feature is disabled".into()))
}
#[cfg(not(feature = "resolve-spotify"))]
async fn spotify_artist(_session: &Session, _id: &str) -> Result<Vec<Track>, DeezerError> {
    Err(DeezerError::Message("resolve-spotify feature is disabled".into()))
}

#[cfg(feature = "resolve-tidal")]
async fn tidal_get(path: &str) -> Result<Value, DeezerError> {
    crate::http::get_json(
        &format!("https://api.tidal.com/v1/{path}"),
        &[("user-agent", "TIDAL/3704 CFNetwork/1220.1 Darwin/20.3.0".into()), ("x-tidal-token", "i4ZDjcyhed7Mu47q".into())],
    )
    .await
}

#[cfg(feature = "resolve-tidal")]
async fn tidal_track(session: &Session, id: &str) -> Result<Track, DeezerError> {
    let track = tidal_get(&format!("tracks/{id}?countryCode=US")).await?;
    isrc_to_deezer(session, track.get("title").and_then(Value::as_str).unwrap_or(""), track.get("isrc").and_then(Value::as_str)).await
}
#[cfg(feature = "resolve-tidal")]
async fn tidal_album(session: &Session, id: &str) -> Result<(Value, Vec<Track>), DeezerError> {
    let album = tidal_get(&format!("albums/{id}?countryCode=US")).await?;
    upc_to_deezer(session, album.get("title").and_then(Value::as_str).unwrap_or(""), album.get("upc").and_then(Value::as_str)).await
}
#[cfg(feature = "resolve-tidal")]
async fn tidal_playlist(session: &Session, id: &str) -> Result<(Value, Vec<Track>), DeezerError> {
    let playlist = tidal_get(&format!("playlists/{id}?countryCode=US")).await?;
    let items = tidal_get(&format!("playlists/{id}/tracks?countryCode=US&limit=500")).await?;
    let mut tracks = Vec::new();
    for item in items.get("items").and_then(Value::as_array).into_iter().flatten() {
        if let Ok(track) = isrc_to_deezer(session, item.get("title").and_then(Value::as_str).unwrap_or(""), item.get("isrc").and_then(Value::as_str)).await {
            tracks.push(track);
        }
    }
    Ok((playlist, tracks))
}
#[cfg(feature = "resolve-tidal")]
async fn tidal_artist(session: &Session, id: &str) -> Result<Vec<Track>, DeezerError> {
    let items = tidal_get(&format!("artists/{id}/toptracks?countryCode=US&limit=10")).await?;
    let mut tracks = Vec::new();
    for item in items.get("items").and_then(Value::as_array).into_iter().flatten().take(10) {
        if let Ok(track) = isrc_to_deezer(session, item.get("title").and_then(Value::as_str).unwrap_or(""), item.get("isrc").and_then(Value::as_str)).await {
            tracks.push(track);
        }
    }
    Ok(tracks)
}
#[cfg(not(feature = "resolve-tidal"))]
async fn tidal_track(_session: &Session, _id: &str) -> Result<Track, DeezerError> {
    Err(DeezerError::Message("resolve-tidal feature is disabled".into()))
}
#[cfg(not(feature = "resolve-tidal"))]
async fn tidal_album(_session: &Session, _id: &str) -> Result<(Value, Vec<Track>), DeezerError> {
    Err(DeezerError::Message("resolve-tidal feature is disabled".into()))
}
#[cfg(not(feature = "resolve-tidal"))]
async fn tidal_playlist(_session: &Session, _id: &str) -> Result<(Value, Vec<Track>), DeezerError> {
    Err(DeezerError::Message("resolve-tidal feature is disabled".into()))
}
#[cfg(not(feature = "resolve-tidal"))]
async fn tidal_artist(_session: &Session, _id: &str) -> Result<Vec<Track>, DeezerError> {
    Err(DeezerError::Message("resolve-tidal feature is disabled".into()))
}

#[cfg(feature = "resolve-youtube")]
fn parse_inline_object(document: &str, variable_name: &str) -> Option<Value> {
    let prefix = format!("{variable_name} = ");
    let start = document.find(&prefix)? + prefix.len();
    let bytes = document[start..].as_bytes();
    let mut depth = 0i32;
    let mut in_string = false;
    let mut escaped = false;
    for (index, byte) in bytes.iter().enumerate() {
        let ch = *byte as char;
        if in_string {
            if escaped { escaped = false; }
            else if ch == '\\' { escaped = true; }
            else if ch == '"' { in_string = false; }
            continue;
        }
        match ch {
            '"' => in_string = true,
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return serde_json::from_str(&document[start..start + index + 1]).ok();
                }
            }
            _ => {}
        }
    }
    None
}

#[cfg(feature = "resolve-youtube")]
fn normalize_youtube_title(title: &str) -> String {
    let lower = title.to_ascii_lowercase();
    let mut text = lower.replace("official video", "");
    if let Some(index) = text.find("(off") { text.truncate(index); }
    if let Some(index) = text.find("lyric") { text.truncate(index); }
    if let Some(index) = text.find("ft") { text.truncate(index); }
    text.replace([',', '-', '.'], " ").split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(feature = "resolve-youtube")]
async fn youtube_track(session: &Session, id: &str) -> Result<Track, DeezerError> {
    let page = crate::http::get_text(&format!("https://www.youtube.com/watch?v={id}&hl=en"), &[]).await?;
    let player = parse_inline_object(&page, "var ytInitialPlayerResponse");
    let initial = parse_inline_object(&page, "var ytInitialData");
    let rows = initial.as_ref().and_then(|value| value.pointer("/contents/twoColumnWatchNextResults/results/results/contents")).and_then(Value::as_array);
    if let Some(rows) = rows {
        let song = rows.iter().find_map(|row| {
            let title = row.pointer("/metadataRowRenderer/title/simpleText").and_then(Value::as_str)?;
            (title == "Song").then(|| row.pointer("/metadataRowRenderer/contents/0/simpleText").and_then(Value::as_str))
        }).flatten();
        let artist = rows.iter().find_map(|row| {
            let title = row.pointer("/metadataRowRenderer/title/simpleText").and_then(Value::as_str)?;
            (title == "Artist").then(|| row.pointer("/metadataRowRenderer/contents/0/runs/0/text").and_then(Value::as_str))
        }).flatten();
        if let (Some(song), Some(artist)) = (song, artist) {
            let found = session.search_alternative(artist, song, 1).await?;
            if let Some(track) = tracks_from_list(&found).into_iter().next() {
                return Ok(track);
            }
        }
    }
    if let Some(title) = player.as_ref().and_then(|value| value.pointer("/videoDetails/title")).and_then(Value::as_str) {
        let normalized = normalize_youtube_title(title);
        if !normalized.is_empty() {
            let found = session.search_music(&normalized, &["TRACK"], 20).await?;
            let tracks = tracks_from_list(&found.get("TRACK").cloned().unwrap_or(found));
            if let Some(track) = tracks.iter().find(|track| normalized.contains(&track.artist_name().to_ascii_lowercase())).cloned().or_else(|| tracks.into_iter().next()) {
                return Ok(track);
            }
        }
    }
    Err(DeezerError::Message(format!("No track found for youtube video {id}")))
}

#[cfg(not(feature = "resolve-youtube"))]
async fn youtube_track(_session: &Session, _id: &str) -> Result<Track, DeezerError> {
    Err(DeezerError::Message("resolve-youtube feature is disabled".into()))
}
