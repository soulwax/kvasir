use serde_json::{json, Value};

use crate::error::DeezerError;
use crate::session::Session;

/// Library writes. Nothing in the download path calls this module.
/// `create_playlist` is one-way: the gateway has no delete method.
pub async fn add_favorite_tracks(session: &Session, ids: &[String]) -> Result<Value, DeezerError> {
    session.gw(json!({"ids": ids}), "song.addFavorites").await
}
pub async fn remove_favorite_tracks(session: &Session, ids: &[String]) -> Result<Value, DeezerError> {
    session.gw(json!({"ids": ids}), "song.removeFavorites").await
}
pub async fn add_favorite_album(session: &Session, album_id: &str) -> Result<Value, DeezerError> {
    session.gw(json!({"alb_id": album_id}), "album.addFavorite").await
}
pub async fn remove_favorite_album(session: &Session, album_id: &str) -> Result<Value, DeezerError> {
    session.gw(json!({"alb_id": album_id}), "album.deleteFavorite").await
}
pub async fn add_favorite_artist(session: &Session, artist_id: &str) -> Result<Value, DeezerError> {
    session.gw(json!({"art_id": artist_id}), "artist.addFavorite").await
}
pub async fn remove_favorite_artist(session: &Session, artist_id: &str) -> Result<Value, DeezerError> {
    session.gw(json!({"art_id": artist_id}), "artist.deleteFavorite").await
}
pub async fn follow_playlist(session: &Session, playlist_id: &str) -> Result<Value, DeezerError> {
    session.gw(json!({"parent_playlist_id": playlist_id}), "playlist.addFavorite").await
}
pub async fn unfollow_playlist(session: &Session, playlist_id: &str) -> Result<Value, DeezerError> {
    session.gw(json!({"playlist_id": playlist_id}), "playlist.deleteFavorite").await
}
pub async fn add_favorite_show(session: &Session, show_id: &str) -> Result<Value, DeezerError> {
    session.gw(json!({"show_id": show_id}), "show.addFavorite").await
}
pub async fn create_playlist(session: &Session, title: &str, description: &str, status: u8, songs: &[String]) -> Result<Value, DeezerError> {
    let songs: Vec<Vec<Value>> = songs.iter().map(|id| vec![Value::String(id.clone()), json!(0)]).collect();
    session.gw(json!({"title": title, "description": description, "status": status, "songs": songs}), "playlist.create").await
}
pub async fn add_tracks_to_playlist(session: &Session, playlist_id: &str, songs: &[String]) -> Result<Value, DeezerError> {
    let songs: Vec<Vec<Value>> = songs.iter().map(|id| vec![Value::String(id.clone()), json!(0)]).collect();
    session.gw(json!({"playlist_id": playlist_id, "songs": songs}), "playlist.addSongs").await
}
pub async fn remove_tracks_from_playlist(session: &Session, playlist_id: &str, songs: &[String]) -> Result<Value, DeezerError> {
    let songs: Vec<Vec<Value>> = songs.iter().map(|id| vec![Value::String(id.clone()), json!(0)]).collect();
    session.gw(json!({"playlist_id": playlist_id, "songs": songs}), "playlist.deleteSongs").await
}
