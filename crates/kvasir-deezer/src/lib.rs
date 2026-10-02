#![forbid(unsafe_code)]

mod api;
mod cache;
mod contributors;
mod convert;
mod decrypt;
mod error;
mod favorites;
mod http;
mod lrc;
mod lyrics;
mod media;
mod session;
mod stream;
mod tag;
mod track;

#[cfg(feature = "enrich")]
mod enrich;

pub use api::*;
pub use cache::{CacheBucketStats, CacheStats};
pub use contributors::{normalize_contributors, NormalizedContributors, Person};
pub use convert::{get_url_parts, isrc_to_deezer, parse_info, upc_to_deezer, ParsedLink, UrlParts};
pub use decrypt::{decrypt_download, encrypt_download, song_file_name, TrackDecryptor};
pub use error::DeezerError;
pub use favorites::*;
pub use http::{get_bytes, get_json, get_text, HttpResponse};
pub use lrc::{to_lrc, LrcMeta};
pub use lyrics::{configure_musixmatch, fallback_lyrics, musixmatch_status};
pub use media::{
    get_track_download_url, refresh_track_tokens, resolve_download_urls, Quality, ResolvedUrl, DEEZER_FORMATS,
};
pub use session::{
    cache_stats, clear_shared_caches, configure_cache, create_session, current_session, init_deezer_api,
    request_public_api, set_default_session, ApiRoots, RetryPolicy, Session, SessionUserData, DEFAULT_ARL,
    RETRY_POLICY,
};
pub use stream::{download_track_bytes, open_download, TrackStream};
pub use tag::{
    add_track_tags, build_tag_model, download_album_cover, download_artist_image, get_rich_album, id3_bytes,
    probe_audio_offset, resolve_tag_model, ExplicitLabel, Probe, RichAlbum, TagOptions, TagStream, TrackTagModel,
    MAX_COVER_SIZE,
};
pub use track::{tracks_from_list, ArtistCredit, Track};

#[cfg(feature = "enrich")]
pub use enrich::{
    configure_musicbrainz, get_best_cover_art_url, get_cover_art, get_cover_art_by_isrc, get_musicbrainz_recording,
    get_musicbrainz_release, lookup_recording_by_isrc, CoverArtImage, MbArtistCredit, MbRecording, MbRelease,
};
