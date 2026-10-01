use serde_json::{json, Value};

use crate::error::DeezerError;
use crate::session::{current_session, request_public_api, Session};
use crate::track::{tracks_from_list, Track};

pub fn build_advanced_query(filters: &Value) -> String {
    let mut parts = Vec::new();
    if let Some(query) = filters.get("query").and_then(Value::as_str) {
        if !query.trim().is_empty() {
            parts.push(query.trim().to_string());
        }
    }
    for (op, key) in [("artist", "artist"), ("album", "album"), ("track", "track"), ("label", "label")] {
        if let Some(value) = filters.get(key).and_then(Value::as_str) {
            if !value.trim().is_empty() {
                parts.push(format!("{op}:\"{}\"", value.trim().replace('"', "")));
            }
        }
    }
    for (op, key) in [("dur_min", "durMin"), ("dur_max", "durMax"), ("bpm_min", "bpmMin"), ("bpm_max", "bpmMax")] {
        if let Some(value) = filters.get(key).and_then(Value::as_f64) {
            if value.is_finite() && value >= 0.0 {
                parts.push(format!("{op}:{}", value.round() as i64));
            }
        }
    }
    parts.join(" ")
}

fn with_params(slug: &str, params: &[(&str, Option<String>)]) -> String {
    let query = params
        .iter()
        .filter_map(|(key, value)| value.as_ref().filter(|text| !text.is_empty()).map(|text| format!("{key}={text}")))
        .collect::<Vec<_>>()
        .join("&");
    if query.is_empty() { slug.to_string() } else { format!("{slug}?{query}") }
}

pub async fn search_public(query: &str, entity: Option<&str>, order: Option<&str>, strict: bool, limit: Option<u32>, index: Option<u32>) -> Result<Value, DeezerError> {
    let mut search = format!("q={}", query.replace(' ', "%20"));
    if strict {
        search.push_str("&strict=on");
    }
    if let Some(order) = order {
        search.push_str(&format!("&order={order}"));
    }
    if let Some(limit) = limit {
        search.push_str(&format!("&limit={limit}"));
    }
    if let Some(index) = index {
        search.push_str(&format!("&index={index}"));
    }
    let segment = match entity {
        Some(entity) if entity != "track" && matches!(entity, "album" | "artist" | "playlist" | "user" | "radio" | "podcast") => format!("/{entity}"),
        _ => String::new(),
    };
    request_public_api(&format!("/search{segment}?{search}")).await
}

pub async fn search_tracks(query: &str, limit: Option<u32>) -> Result<Value, DeezerError> {
    search_public(query, Some("track"), None, false, limit, None).await
}

pub async fn search_albums(query: &str) -> Result<Value, DeezerError> {
    search_public(query, Some("album"), None, false, None, None).await
}

pub async fn search_artists(query: &str) -> Result<Value, DeezerError> {
    search_public(query, Some("artist"), None, false, None, None).await
}

pub async fn search_playlists(query: &str) -> Result<Value, DeezerError> {
    search_public(query, Some("playlist"), None, false, None, None).await
}

pub fn search_facets(result: &Value) -> Value {
    let total = |key: &str| result.pointer(&format!("/{key}/total")).and_then(Value::as_i64).unwrap_or(0);
    json!({
        "track": total("TRACK"),
        "album": total("ALBUM"),
        "artist": total("ARTIST"),
        "playlist": total("PLAYLIST"),
        "radio": total("RADIO"),
        "show": total("SHOW"),
        "user": total("USER"),
        "order": result.get("ORDER").cloned().unwrap_or(json!([])),
    })
}

pub async fn get_genres() -> Result<Value, DeezerError> {
    request_public_api("/genre").await
}
pub async fn get_chart(genre_id: &str, limit: u32) -> Result<Value, DeezerError> {
    request_public_api(&with_params(&format!("/chart/{genre_id}"), &[("limit", Some(limit.to_string()))])).await
}
pub async fn get_chart_tracks(genre_id: &str, limit: u32, index: u32) -> Result<Value, DeezerError> {
    request_public_api(&with_params(&format!("/chart/{genre_id}/tracks"), &[("limit", Some(limit.to_string())), ("index", Some(index.to_string()))])).await
}
pub async fn get_genre_artists(genre_id: &str) -> Result<Value, DeezerError> {
    request_public_api(&format!("/genre/{genre_id}/artists")).await
}
pub async fn get_editorial_list() -> Result<Value, DeezerError> {
    request_public_api("/editorial").await
}
pub async fn get_editorial_releases(id: &str, limit: u32, index: u32) -> Result<Value, DeezerError> {
    request_public_api(&with_params(&format!("/editorial/{id}/releases"), &[("limit", Some(limit.to_string())), ("index", Some(index.to_string()))])).await
}
pub async fn get_editorial_selection(id: &str) -> Result<Value, DeezerError> {
    request_public_api(&format!("/editorial/{id}/selection")).await
}
pub async fn get_editorial_charts(id: &str) -> Result<Value, DeezerError> {
    request_public_api(&format!("/editorial/{id}/charts")).await
}
pub async fn get_artist_top_tracks(artist_id: &str, limit: u32) -> Result<Value, DeezerError> {
    request_public_api(&with_params(&format!("/artist/{artist_id}/top"), &[("limit", Some(limit.to_string()))])).await
}
pub async fn get_related_artists(artist_id: &str, limit: u32) -> Result<Value, DeezerError> {
    request_public_api(&with_params(&format!("/artist/{artist_id}/related"), &[("limit", Some(limit.to_string()))])).await
}
pub async fn get_artist_albums(artist_id: &str, limit: u32, index: u32) -> Result<Value, DeezerError> {
    request_public_api(&with_params(&format!("/artist/{artist_id}/albums"), &[("limit", Some(limit.to_string())), ("index", Some(index.to_string()))])).await
}
pub async fn get_artist_playlists(artist_id: &str, limit: u32) -> Result<Value, DeezerError> {
    request_public_api(&with_params(&format!("/artist/{artist_id}/playlists"), &[("limit", Some(limit.to_string()))])).await
}
pub async fn get_artist_radio_tracks(artist_id: &str) -> Result<Value, DeezerError> {
    request_public_api(&format!("/artist/{artist_id}/radio")).await
}
pub async fn get_track_by_isrc(isrc: &str) -> Result<Value, DeezerError> {
    request_public_api(&format!("/track/isrc:{isrc}")).await
}
pub async fn get_album_by_upc(upc: &str) -> Result<Value, DeezerError> {
    let code = if upc.len() > 12 && upc.starts_with('0') { &upc[upc.len() - 12..] } else { upc };
    request_public_api(&format!("/album/upc:{code}")).await
}
pub async fn get_user_flow(user_id: &str, limit: u32) -> Result<Value, DeezerError> {
    request_public_api(&with_params(&format!("/user/{user_id}/flow"), &[("limit", Some(limit.to_string()))])).await
}
pub async fn get_user_favorite_tracks(user_id: &str, limit: Option<u32>, index: Option<u32>) -> Result<Value, DeezerError> {
    request_public_api(&with_params(&format!("/user/{user_id}/tracks"), &[("limit", limit.map(|v| v.to_string())), ("index", index.map(|v| v.to_string()))])).await
}
pub async fn get_user_favorite_albums(user_id: &str) -> Result<Value, DeezerError> {
    request_public_api(&format!("/user/{user_id}/albums")).await
}
pub async fn get_user_favorite_artists(user_id: &str) -> Result<Value, DeezerError> {
    request_public_api(&format!("/user/{user_id}/artists")).await
}
pub async fn get_user_playlists(user_id: &str) -> Result<Value, DeezerError> {
    request_public_api(&format!("/user/{user_id}/playlists")).await
}
pub async fn get_user_radios(user_id: &str) -> Result<Value, DeezerError> {
    request_public_api(&format!("/user/{user_id}/radios")).await
}
pub async fn get_user_chart_tracks(user_id: &str, limit: u32) -> Result<Value, DeezerError> {
    request_public_api(&with_params(&format!("/user/{user_id}/charts/tracks"), &[("limit", Some(limit.to_string()))])).await
}
pub async fn get_radios() -> Result<Value, DeezerError> {
    request_public_api("/radio").await
}
pub async fn get_radio_tracks(radio_id: &str) -> Result<Value, DeezerError> {
    request_public_api(&format!("/radio/{radio_id}/tracks")).await
}
pub async fn get_radio_genres() -> Result<Value, DeezerError> {
    request_public_api("/radio/genres").await
}

async fn own_user_id(session: &Session, user_id: Option<&str>) -> Result<String, DeezerError> {
    if let Some(user_id) = user_id {
        return Ok(user_id.to_string());
    }
    let user = session.get_user().await?;
    Ok(user.get("USER_ID").map(|value| match value {
        Value::String(text) => text.clone(),
        Value::Number(number) => number.to_string(),
        _ => String::new(),
    }).unwrap_or_default())
}

pub async fn get_my_playlists(session: &Session, user_id: Option<&str>, nb: i64, start: i64) -> Result<Value, DeezerError> {
    let user_id = own_user_id(session, user_id).await?;
    session.gw(json!({"user_id": user_id, "nb": nb, "start": start, "tab": "all"}), "playlist.getList").await
}
pub async fn get_my_favorite_tracks(session: &Session, user_id: Option<&str>, nb: i64, start: i64) -> Result<Vec<Track>, DeezerError> {
    let user_id = own_user_id(session, user_id).await?;
    Ok(tracks_from_list(&session.gw(json!({"user_id": user_id, "nb": nb, "start": start}), "song.getFavorites").await?))
}
pub async fn get_my_favorite_track_ids(session: &Session) -> Result<Value, DeezerError> {
    session.gw(json!({}), "song.getFavoriteIds").await
}
pub async fn get_my_favorite_albums(session: &Session, user_id: Option<&str>, nb: i64, start: i64) -> Result<Value, DeezerError> {
    let user_id = own_user_id(session, user_id).await?;
    session.gw(json!({"user_id": user_id, "nb": nb, "start": start}), "album.getFavorites").await
}
pub async fn get_my_favorite_artists(session: &Session, user_id: Option<&str>, nb: i64, start: i64) -> Result<Value, DeezerError> {
    let user_id = own_user_id(session, user_id).await?;
    session.gw(json!({"user_id": user_id, "nb": nb, "start": start}), "artist.getFavorites").await
}
pub async fn get_my_favorite_playlists(session: &Session, user_id: Option<&str>, nb: i64, start: i64) -> Result<Value, DeezerError> {
    let user_id = own_user_id(session, user_id).await?;
    session.gw(json!({"user_id": user_id, "nb": nb, "start": start}), "playlist.getFavorites").await
}
pub async fn get_my_favorite_radios(session: &Session, user_id: Option<&str>, nb: i64, start: i64) -> Result<Value, DeezerError> {
    let user_id = own_user_id(session, user_id).await?;
    session.gw(json!({"user_id": user_id, "nb": nb, "start": start}), "radio.getFavorites").await
}
pub async fn get_my_favorite_shows(session: &Session, user_id: Option<&str>, nb: i64, start: i64) -> Result<Value, DeezerError> {
    let user_id = own_user_id(session, user_id).await?;
    session.gw(json!({"user_id": user_id, "nb": nb, "start": start}), "show.getFavorites").await
}
pub async fn get_track_mix(session: &Session, song_id: &str, nb: i64, start: i64) -> Result<Vec<Track>, DeezerError> {
    Ok(tracks_from_list(&session.gw(json!({"sng_id": song_id, "start": start, "nb": nb}), "song.getSearchTrackMix").await?))
}

pub async fn get_show_info(session: &Session, show_id: &str, nb: i64, start: i64) -> Result<Value, DeezerError> {
    session.gw(json!({"SHOW_ID": show_id, "NB": nb, "START": start}), "mobile.pageShow").await
}
pub async fn get_episode(session: &Session, episode_id: &str) -> Result<Value, DeezerError> {
    session.gw(json!({"episode_id": episode_id}), "episode.getData").await
}
pub async fn get_show_episodes(session: &Session, show_id: &str, nb: i64, start: i64) -> Result<Value, DeezerError> {
    let show = get_show_info(session, show_id, nb, start).await?;
    Ok(show.get("EPISODES").cloned().unwrap_or(json!({"data": []})))
}
pub async fn get_channel_list(session: &Session) -> Result<Value, DeezerError> {
    session.gw(json!({}), "search_getChannels").await
}
pub async fn get_playlist_channel(session: &Session, page: &str) -> Result<Value, DeezerError> {
    let gateway_input = json!({
        "page": page,
        "version": "2.3",
        "lang": "en",
        "timezone_offset": "6",
    });
    session.gw_get("app_page_get", vec![("gateway_input".into(), gateway_input.to_string()), ("page".into(), page.into())]).await
}

#[derive(Clone, Debug)]
pub struct TrackPreview {
    pub url: String,
    pub duration: u32,
}

pub async fn get_track_preview(session: &Session, track: &Track) -> Result<Option<TrackPreview>, DeezerError> {
    if let Some(url) = track.preview_href() {
        return Ok(Some(TrackPreview { url, duration: 30 }));
    }
    let public = if track.sng_id().is_empty() {
        return Ok(None);
    } else {
        request_public_api(&format!("/track/{}", track.sng_id())).await?
    };
    let _ = session;
    Ok(public.get("preview").and_then(Value::as_str).filter(|url| !url.is_empty()).map(|url| TrackPreview { url: url.to_string(), duration: 30 }))
}

pub async fn download_preview(session: &Session, track: &Track) -> Result<Option<bytes::Bytes>, DeezerError> {
    let Some(preview) = get_track_preview(session, track).await? else {
        return Ok(None);
    };
    Ok(Some(crate::http::get_bytes(&preview.url, &[]).await?))
}

pub async fn search_music(query: &str, types: &[&str], nb: i64) -> Result<Value, DeezerError> {
    current_session().search_music(query, types, nb).await
}

pub async fn suggest(query: &str, nb: i64) -> Result<Value, DeezerError> {
    current_session().suggest(query, nb).await
}
