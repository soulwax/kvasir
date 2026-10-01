use bytes::Bytes;
use id3::frame::{Content, ExtendedText, Frame, Picture, PictureType};
use id3::{Tag, TagLike, Version};
use serde_json::Value;

use crate::contributors::normalize_contributors;
use crate::error::DeezerError;
use crate::http::get_bytes;
use crate::lrc::{to_lrc, LrcMeta};
use crate::session::Session;
use crate::track::Track;

pub const MAX_COVER_SIZE: u32 = 1800;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExplicitLabel {
    Explicit,
    Clean,
    Unknown,
}

#[derive(Clone, Debug)]
pub struct TrackTagModel {
    pub title: String,
    pub subtitle: Option<String>,
    pub album: String,
    pub artists: Vec<String>,
    pub main_artists: Vec<String>,
    pub featured_artists: Vec<String>,
    pub album_artist: String,
    pub composers: Vec<String>,
    pub lyricists: Vec<String>,
    pub producers: Vec<String>,
    pub engineers: Vec<crate::contributors::Person>,
    pub mixers: Vec<String>,
    pub performers: Vec<crate::contributors::Person>,
    pub publishers: Vec<String>,
    pub track_number: u32,
    pub track_total: Option<u32>,
    pub disc_number: u32,
    pub disc_total: Option<u32>,
    pub isrc: Option<String>,
    pub barcode: Option<String>,
    pub duration_ms: u64,
    pub bpm: Option<f64>,
    pub genres: Vec<String>,
    pub label: Option<String>,
    pub release_type: Option<String>,
    pub is_compilation: bool,
    pub date: Option<String>,
    pub year: Option<String>,
    pub original_date: Option<String>,
    pub original_year: Option<String>,
    pub copyright: Option<String>,
    pub producer_line: Option<String>,
    pub replay_gain_track_gain: Option<String>,
    pub explicit: ExplicitLabel,
    pub itunes_advisory: u8,
    pub lyrics: Option<String>,
    pub lyrics_synced: Option<String>,
    pub lyrics_writers: Option<String>,
    pub lyrics_copyright: Option<String>,
    pub deezer_track_id: Option<String>,
    pub deezer_album_id: Option<String>,
    pub deezer_artist_id: Option<String>,
    pub slug: Option<String>,
    pub label_id: Option<String>,
    pub provider_id: Option<String>,
    pub rank: Option<u64>,
    pub cover: Option<Bytes>,
    pub cover_size: u32,
    pub artist_image: Option<Bytes>,
}

#[derive(Clone, Debug)]
pub struct RichAlbum {
    pub title: String,
    pub album_artist: String,
    pub copyright: Option<String>,
    pub producer_line: Option<String>,
    pub release_date: Option<String>,
    pub original_date: Option<String>,
    pub upc: Option<String>,
    pub label: Option<String>,
    pub label_id: Option<String>,
    pub genres: Vec<String>,
    pub record_type: Option<String>,
    pub is_compilation: bool,
    pub is_live: bool,
    pub track_total: Option<u32>,
    pub disc_total: Option<u32>,
}

#[derive(Clone, Debug)]
pub struct TagOptions {
    pub cover_size: u32,
    pub embed_cover: bool,
    pub embed_artist_image: bool,
    pub write_lyrics: bool,
    pub lyrics_fallback: bool,
    pub embed_synced_lyrics: bool,
    pub rich_credits: bool,
    pub deezer_ids: bool,
    pub include_rank: bool,
    pub cover: Option<Option<Bytes>>,
    pub artist_image: Option<Option<Bytes>>,
    pub album: Option<Option<RichAlbum>>,
    pub lyrics: Option<Option<Value>>,
    pub public_track: Option<Option<Value>>,
}

impl Default for TagOptions {
    fn default() -> Self {
        Self {
            cover_size: 1000,
            embed_cover: true,
            embed_artist_image: true,
            write_lyrics: true,
            lyrics_fallback: false,
            embed_synced_lyrics: true,
            rich_credits: true,
            deezer_ids: true,
            include_rank: true,
            cover: None,
            artist_image: None,
            album: None,
            lyrics: None,
            public_track: None,
        }
    }
}

fn year_of(date: Option<&str>) -> Option<String> {
    date.and_then(|value| value.get(..4).filter(|year| year.chars().all(|ch| ch.is_ascii_digit())).map(str::to_string))
}

fn pick_date(value: Option<&str>) -> Option<String> {
    value.and_then(|text| {
        let text = text.trim();
        (text.len() >= 10 && text.as_bytes().get(4) == Some(&b'-') && !text.starts_with("0000")).then(|| text[..10].to_string())
    })
}

fn explicit_from(status: Option<i64>) -> (ExplicitLabel, u8) {
    match status {
        Some(1 | 4) => (ExplicitLabel::Explicit, 1),
        Some(3) => (ExplicitLabel::Clean, 2),
        _ => (ExplicitLabel::Unknown, 0),
    }
}

fn string_at(value: &Value, key: &str) -> String {
    match value.get(key) {
        Some(Value::String(text)) => text.clone(),
        Some(Value::Number(number)) => number.to_string(),
        _ => String::new(),
    }
}

pub async fn get_rich_album(session: &Session, album_id: &str) -> RichAlbum {
    let gw = session.get_album_info(album_id).await.ok();
    let public = crate::session::request_public_api(&format!("/album/{album_id}")).await.ok();
    let copyright = gw.as_ref().map(|item| string_at(item, "COPYRIGHT")).filter(|text| !text.trim().is_empty());
    let producer = gw.as_ref().map(|item| string_at(item, "PRODUCER_LINE")).filter(|text| !text.trim().is_empty());
    let copyright = copyright.or_else(|| producer.clone());
    let producer_line = producer.filter(|line| Some(line) != copyright.as_ref());
    let release_date = pick_date(gw.as_ref().map(|item| string_at(item, "DIGITAL_RELEASE_DATE")).as_deref())
        .or_else(|| pick_date(public.as_ref().and_then(|item| item.get("release_date")).and_then(Value::as_str)))
        .or_else(|| pick_date(gw.as_ref().map(|item| string_at(item, "PHYSICAL_RELEASE_DATE")).as_deref()))
        .or_else(|| pick_date(gw.as_ref().map(|item| string_at(item, "ORIGINAL_RELEASE_DATE")).as_deref()));
    let original = pick_date(gw.as_ref().map(|item| string_at(item, "ORIGINAL_RELEASE_DATE")).as_deref());
    let record_type = public.as_ref().and_then(|item| item.get("record_type")).and_then(Value::as_str).map(str::to_string);
    let genres = public
        .as_ref()
        .and_then(|item| item.pointer("/genres/data"))
        .and_then(Value::as_array)
        .map(|items| items.iter().filter_map(|item| item.get("name").and_then(Value::as_str).map(str::to_string)).collect())
        .unwrap_or_default();
    RichAlbum {
        title: gw.as_ref().map(|item| string_at(item, "ALB_TITLE")).filter(|text| !text.is_empty()).or_else(|| public.as_ref().and_then(|item| item.get("title")).and_then(Value::as_str).map(str::to_string)).unwrap_or_default(),
        album_artist: gw.as_ref().map(|item| string_at(item, "ART_NAME")).filter(|text| !text.is_empty()).or_else(|| public.as_ref().and_then(|item| item.pointer("/artist/name")).and_then(Value::as_str).map(str::to_string)).unwrap_or_default(),
        copyright,
        producer_line,
        release_date: release_date.clone(),
        original_date: original.filter(|date| Some(date) != release_date.as_ref()),
        upc: gw.as_ref().map(|item| string_at(item, "UPC")).filter(|text| !text.is_empty()).or_else(|| public.as_ref().and_then(|item| item.get("upc")).and_then(Value::as_str).map(str::to_string)),
        label: public.as_ref().and_then(|item| item.get("label")).and_then(Value::as_str).map(str::to_string),
        label_id: gw.as_ref().map(|item| string_at(item, "LABEL_ID")).filter(|text| !text.is_empty()),
        genres,
        is_compilation: gw.as_ref().and_then(|item| item.pointer("/SUBTYPES/isCompilation")).and_then(Value::as_bool).unwrap_or(false) || record_type.as_deref() == Some("compile"),
        is_live: gw.as_ref().and_then(|item| item.pointer("/SUBTYPES/isLive")).and_then(Value::as_bool).unwrap_or(false),
        record_type,
        track_total: gw.as_ref().and_then(|item| string_at(item, "NUMBER_TRACK").parse().ok()).or_else(|| public.as_ref().and_then(|item| item.get("nb_tracks")).and_then(Value::as_u64).map(|value| value as u32)),
        disc_total: gw.as_ref().and_then(|item| string_at(item, "NUMBER_DISK").parse().ok()),
    }
}

fn cover_url(kind: &str, md5: &str, size: u32) -> String {
    let size = size.clamp(56, MAX_COVER_SIZE);
    format!("https://e-cdns-images.dzcdn.net/images/{kind}/{md5}/{size}x{size}-000000-80-0-0.jpg")
}

pub async fn download_album_cover(track: &Track, size: u32) -> Option<Bytes> {
    let md5 = track.album_picture();
    if md5.is_empty() {
        return None;
    }
    get_bytes(&cover_url("cover", &md5, size), &[]).await.ok()
}

pub async fn download_artist_image(track: &Track, size: u32) -> Option<Bytes> {
    let md5 = track.artist_picture();
    if md5.is_empty() {
        return None;
    }
    get_bytes(&cover_url("artist", &md5, size), &[]).await.ok()
}

pub fn build_tag_model(
    track: &Track,
    album: Option<&RichAlbum>,
    public_track: Option<&Value>,
    lyrics: Option<&Value>,
    cover: Option<Bytes>,
    artist_image: Option<Bytes>,
    cover_size: u32,
    deezer_ids: bool,
    include_rank: bool,
) -> TrackTagModel {
    let credits = normalize_contributors(&track.contributors());
    let artists_all: Vec<String> = track.artists().into_iter().map(|artist| artist.name).filter(|name| !name.is_empty()).collect();
    let main_artists = if !credits.main_artists.is_empty() {
        credits.main_artists.clone()
    } else if !artists_all.is_empty() {
        artists_all.clone()
    } else {
        vec![track.artist_name()]
    };
    let featured_artists = if !credits.featuring.is_empty() {
        credits.featuring.clone()
    } else {
        artists_all.into_iter().filter(|name| !main_artists.contains(name)).collect()
    };
    let mut artists = main_artists.clone();
    for name in &featured_artists {
        if !artists.contains(name) {
            artists.push(name.clone());
        }
    }
    let is_compilation = album.is_some_and(|item| item.is_compilation) || track.artist_name().to_ascii_lowercase().starts_with("various");
    let date = album.and_then(|item| item.release_date.clone()).or_else(|| {
        public_track
            .and_then(|item| item.get("release_date"))
            .and_then(Value::as_str)
            .filter(|value| value.len() == 10)
            .map(str::to_string)
    });
    let original_date = album.and_then(|item| item.original_date.clone()).filter(|value| Some(value) != date.as_ref());
    let gain = track.gain().parse::<f64>().ok().or_else(|| public_track.and_then(|item| item.get("gain")).and_then(Value::as_f64));
    let bpm = public_track.and_then(|item| item.get("bpm")).and_then(Value::as_f64).filter(|value| *value > 0.0);
    let (explicit, itunes_advisory) = explicit_from(track.explicit_status());
    let mut release_type = album.and_then(|item| item.record_type.clone());
    if album.is_some_and(|item| item.is_live) {
        release_type = Some("live".into());
    } else if is_compilation {
        release_type = Some("compilation".into());
    }
    let version = {
        let version = track.version();
        if version.is_empty() {
            public_track.and_then(|item| item.get("title_version")).and_then(Value::as_str).unwrap_or("").trim().to_string()
        } else {
            version
        }
    };
    let lyrics_text = lyrics.and_then(|item| item.get("LYRICS_TEXT")).and_then(Value::as_str).map(str::to_string);
    let writers = lyrics.and_then(|item| item.get("LYRICS_WRITERS")).and_then(Value::as_str).map(str::to_string);
    TrackTagModel {
        title: track.title(),
        subtitle: (!version.is_empty()).then_some(version),
        album: {
            let album_title = track.album_title();
            if album_title.is_empty() { album.map(|item| item.title.clone()).unwrap_or_default() } else { album_title }
        },
        artists,
        main_artists: main_artists.clone(),
        featured_artists,
        album_artist: album.and_then(|item| (!item.album_artist.is_empty()).then(|| item.album_artist.clone())).unwrap_or_else(|| track.artist_name()),
        composers: credits.composers,
        lyricists: credits.lyricists,
        producers: credits.producers,
        engineers: credits.engineers,
        mixers: credits.mixers,
        performers: credits.performers,
        publishers: credits.publishers,
        track_number: track.track_number(),
        track_total: album.and_then(|item| item.track_total),
        disc_number: if track.disk_number() == 0 { 1 } else { track.disk_number() },
        disc_total: album.and_then(|item| item.disc_total),
        isrc: {
            let isrc = track.isrc();
            if isrc.is_empty() { public_track.and_then(|item| item.get("isrc")).and_then(Value::as_str).map(str::to_string) } else { Some(isrc) }
        },
        barcode: album.and_then(|item| item.upc.clone()),
        duration_ms: track.duration_seconds() * 1000,
        bpm,
        genres: album.map(|item| item.genres.clone()).unwrap_or_default(),
        label: album.and_then(|item| item.label.clone()),
        release_type,
        is_compilation,
        year: year_of(date.as_deref()),
        original_year: year_of(original_date.as_deref()),
        date,
        original_date,
        copyright: album.and_then(|item| item.copyright.clone()),
        producer_line: album.and_then(|item| item.producer_line.clone()),
        replay_gain_track_gain: gain.map(|value| format!("{value:.2} dB")),
        explicit,
        itunes_advisory,
        lyrics_synced: to_lrc(lyrics.and_then(|item| item.get("LYRICS_SYNC_JSON")), &LrcMeta {
            title: Some(track.title()),
            artist: Some(main_artists.join(", ")),
            album: Some(track.album_title()),
            writers: writers.clone(),
            length_seconds: Some(track.duration_seconds()),
        }),
        lyrics: lyrics_text,
        lyrics_writers: writers,
        lyrics_copyright: lyrics.and_then(|item| item.get("LYRICS_COPYRIGHTS")).and_then(Value::as_str).map(str::to_string),
        deezer_track_id: deezer_ids.then(|| track.sng_id()).filter(|value| !value.is_empty()),
        deezer_album_id: deezer_ids.then(|| track.album_id()).filter(|value| !value.is_empty()),
        deezer_artist_id: deezer_ids.then(|| track.artist_id()).filter(|value| !value.is_empty()),
        slug: deezer_ids.then(|| track.url_rewriting()).filter(|value| !value.is_empty()),
        label_id: deezer_ids.then(|| album.and_then(|item| item.label_id.clone())).flatten(),
        provider_id: deezer_ids.then(|| track.provider_id()).filter(|value| !value.is_empty()),
        rank: include_rank.then(|| track.rank().parse().ok()).flatten(),
        cover,
        cover_size,
        artist_image,
    }
}

async fn hydrate(session: &Session, track: Track) -> Track {
    let contributors = track.contributors();
    if !contributors.is_null() && !track.version().is_empty() && !track.gain().is_empty() {
        return track;
    }
    let Ok(full) = session.get_track_info(&track.sng_id()).await else {
        return track;
    };
    let mut track = track;
    track.merge_missing(&full, &["SNG_CONTRIBUTORS", "VERSION", "GAIN", "RANK", "URL_REWRITING", "PROVIDER_ID", "EXPLICIT_TRACK_CONTENT", "ART_PICTURE"]);
    track
}

pub async fn resolve_tag_model(session: &Session, track: Track, options: &TagOptions) -> Result<TrackTagModel, DeezerError> {
    let mut track = if options.rich_credits { hydrate(session, track).await } else { track };
    if track.artist_name().eq_ignore_ascii_case("various") {
        if let Some(object) = track.raw.as_object_mut() {
            object.insert("ART_NAME".into(), Value::String("Various Artists".into()));
        }
    }
    let album = match &options.album {
        Some(value) => value.clone(),
        None => Some(get_rich_album(session, &track.album_id()).await),
    };
    let lyrics = match &options.lyrics {
        Some(value) => value.clone(),
        None if options.write_lyrics => session.get_lyrics(&track.sng_id()).await.ok().or(None),
        None => None,
    };
    let lyrics = if lyrics.is_none() && options.lyrics_fallback {
        crate::lyrics::fallback_lyrics(&track).await.ok().map(|text| serde_json::json!({"LYRICS_TEXT": text}))
    } else {
        lyrics
    };
    let public_track = match &options.public_track {
        Some(value) => value.clone(),
        None if options.rich_credits => crate::session::request_public_api(&format!("/track/{}", track.sng_id())).await.ok(),
        None => None,
    };
    let cover = match &options.cover {
        Some(value) => value.clone(),
        None if options.embed_cover => download_album_cover(&track, options.cover_size).await,
        None => None,
    };
    let artist_image = match &options.artist_image {
        Some(value) => value.clone(),
        None if options.embed_artist_image => download_artist_image(&track, options.cover_size).await,
        None => None,
    };
    Ok(build_tag_model(
        &track,
        album.as_ref(),
        public_track.as_ref(),
        lyrics.as_ref(),
        if options.embed_cover { cover } else { None },
        if options.embed_artist_image { artist_image } else { None },
        options.cover_size,
        options.deezer_ids,
        options.include_rank,
    ))
}

fn txxx(tag: &mut Tag, description: &str, value: &str) {
    if value.is_empty() {
        return;
    }
    tag.add_frame(Frame::with_content(
        "TXXX",
        Content::ExtendedText(ExtendedText {
            description: description.into(),
            value: value.into(),
        }),
    ));
}

fn join(values: &[String]) -> String {
    values.iter().filter(|value| !value.is_empty()).cloned().collect::<Vec<_>>().join("; ")
}

pub fn id3_bytes(model: &TrackTagModel) -> Result<Vec<u8>, DeezerError> {
    let mut tag = Tag::new();
    tag.set_title(&model.title);
    if let Some(subtitle) = &model.subtitle {
        tag.set_text("TIT3", subtitle);
    }
    tag.set_album(&model.album);
    if !model.artists.is_empty() {
        tag.set_artist(model.artists.join("/"));
    }
    if !model.album_artist.is_empty() {
        tag.set_album_artist(&model.album_artist);
    }
    if !model.composers.is_empty() {
        tag.set_text("TCOM", join(&model.composers));
    }
    if !model.lyricists.is_empty() {
        tag.set_text("TEXT", join(&model.lyricists));
    }
    if !model.genres.is_empty() {
        tag.set_genre(join(&model.genres));
    }
    if let Some(label) = &model.label {
        tag.set_text("TPUB", label);
    }
    if let Some(isrc) = &model.isrc {
        tag.set_text("TSRC", isrc);
    }
    if model.duration_ms > 0 {
        tag.set_text("TLEN", model.duration_ms.to_string());
    }
    if let Some(bpm) = model.bpm {
        tag.set_text("TBPM", format!("{}", bpm.round() as u32));
    }
    if model.track_number > 0 {
        let value = match model.track_total {
            Some(total) => format!("{}/{}", model.track_number, total),
            None => model.track_number.to_string(),
        };
        tag.set_text("TRCK", value);
    }
    if model.disc_number > 0 {
        let value = match model.disc_total {
            Some(total) => format!("{}/{}", model.disc_number, total),
            None => model.disc_number.to_string(),
        };
        tag.set_text("TPOS", value);
    }
    if let Some(year) = &model.year {
        tag.set_text("TYER", year);
    }
    if let Some(copyright) = &model.copyright {
        tag.set_text("TCOP", copyright);
    }
    txxx(&mut tag, "DATE", model.date.as_deref().unwrap_or(""));
    txxx(&mut tag, "ORIGINALDATE", model.original_date.as_deref().unwrap_or(""));
    txxx(&mut tag, "BARCODE", model.barcode.as_deref().unwrap_or(""));
    txxx(&mut tag, "RELEASETYPE", model.release_type.as_deref().unwrap_or(""));
    txxx(&mut tag, "COMPILATION", if model.is_compilation { "1" } else { "0" });
    txxx(&mut tag, "ITUNESADVISORY", &model.itunes_advisory.to_string());
    if model.explicit != ExplicitLabel::Unknown {
        txxx(&mut tag, "EXPLICIT", if model.explicit == ExplicitLabel::Explicit { "1" } else { "0" });
    }
    if let Some(gain) = &model.replay_gain_track_gain {
        txxx(&mut tag, "REPLAYGAIN_TRACK_GAIN", gain);
    }
    if let Some(lyrics) = &model.lyrics {
        tag.add_frame(Frame::with_content(
            "USLT",
            Content::Lyrics(id3::frame::Lyrics {
                lang: "eng".into(),
                description: String::new(),
                text: lyrics.clone(),
            }),
        ));
    }
    if let Some(cover) = &model.cover {
        tag.add_frame(Frame::with_content(
            "APIC",
            Content::Picture(Picture {
                mime_type: "image/jpeg".into(),
                picture_type: PictureType::CoverFront,
                description: String::new(),
                data: cover.to_vec(),
            }),
        ));
    }
    if let Some(id) = &model.deezer_track_id {
        txxx(&mut tag, "DEEZER_TRACK_ID", id);
    }
    if let Some(id) = &model.deezer_album_id {
        txxx(&mut tag, "DEEZER_ALBUM_ID", id);
    }
    let mut out = Vec::new();
    tag.write_to(&mut out, Version::Id3v23)
        .map_err(|err| DeezerError::Message(err.to_string()))?;
    Ok(out)
}

fn vorbis_comments(model: &TrackTagModel) -> Vec<(String, String)> {
    let mut pairs = vec![
        ("TITLE".into(), model.title.clone()),
        ("ALBUM".into(), model.album.clone()),
        ("ARTIST".into(), model.artists.join("; ")),
        ("ALBUMARTIST".into(), model.album_artist.clone()),
    ];
    let push = |pairs: &mut Vec<(String, String)>, key: &str, value: &str| {
        if !value.is_empty() {
            pairs.push((key.into(), value.into()));
        }
    };
    push(&mut pairs, "ISRC", model.isrc.as_deref().unwrap_or(""));
    push(&mut pairs, "GENRE", &model.genres.join("; "));
    push(&mut pairs, "DATE", model.date.as_deref().unwrap_or(""));
    push(&mut pairs, "LABEL", model.label.as_deref().unwrap_or(""));
    push(&mut pairs, "COPYRIGHT", model.copyright.as_deref().unwrap_or(""));
    push(&mut pairs, "REPLAYGAIN_TRACK_GAIN", model.replay_gain_track_gain.as_deref().unwrap_or(""));
    if model.embeddable_lyrics() {
        push(&mut pairs, "LYRICS", model.lyrics.as_deref().unwrap_or(""));
        push(&mut pairs, "SYNCEDLYRICS", model.lyrics_synced.as_deref().unwrap_or(""));
    }
    if model.track_number > 0 {
        pairs.push(("TRACKNUMBER".into(), model.track_number.to_string()));
    }
    if model.disc_number > 0 {
        pairs.push(("DISCNUMBER".into(), model.disc_number.to_string()));
    }
    pairs
}

impl TrackTagModel {
    fn embeddable_lyrics(&self) -> bool {
        self.lyrics.is_some() || self.lyrics_synced.is_some()
    }
}

fn comment_block(pairs: &[(String, String)]) -> Vec<u8> {
    let vendor = b"kvasir-core";
    let mut body = Vec::new();
    body.extend_from_slice(&(vendor.len() as u32).to_le_bytes());
    body.extend_from_slice(vendor);
    body.extend_from_slice(&(pairs.len() as u32).to_le_bytes());
    for (key, value) in pairs {
        let entry = format!("{key}={value}");
        body.extend_from_slice(&(entry.len() as u32).to_le_bytes());
        body.extend_from_slice(entry.as_bytes());
    }
    body
}

fn block(kind: u8, last: bool, body: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(4 + body.len());
    let header = if last { 0x80 | kind } else { kind };
    out.push(header);
    out.extend_from_slice(&(body.len() as u32).to_be_bytes()[1..]);
    out.extend_from_slice(body);
    out
}

pub enum Probe {
    NeedMore,
    Ready { audio_offset: usize, flac: bool },
}

pub fn probe_audio_offset(buf: &[u8]) -> Probe {
    if buf.len() >= 4 && &buf[..4] == b"fLaC" {
        let mut offset = 4usize;
        loop {
            if offset + 4 > buf.len() {
                return Probe::NeedMore;
            }
            let is_last = buf[offset] >= 128;
            let length = u32::from_be_bytes([0, buf[offset + 1], buf[offset + 2], buf[offset + 3]]) as usize;
            offset += 4 + length;
            if is_last {
                return if offset <= buf.len() {
                    Probe::Ready { audio_offset: offset, flac: true }
                } else {
                    Probe::NeedMore
                };
            }
        }
    }
    if buf.len() < 10 {
        return Probe::NeedMore;
    }
    if &buf[..3] != b"ID3" {
        return Probe::Ready { audio_offset: 0, flac: false };
    }
    let size = ((buf[6] as usize) << 21) | ((buf[7] as usize) << 14) | ((buf[8] as usize) << 7) | buf[9] as usize;
    Probe::Ready { audio_offset: 10 + size, flac: false }
}

fn flac_header(source_header: &[u8], model: &TrackTagModel) -> Vec<u8> {
    let mut blocks = Vec::new();
    if source_header.len() >= 8 && &source_header[..4] == b"fLaC" {
        let mut offset = 4usize;
        while offset + 4 <= source_header.len() {
            let kind = source_header[offset] & 0x7f;
            let length = u32::from_be_bytes([0, source_header[offset + 1], source_header[offset + 2], source_header[offset + 3]]) as usize;
            let end = (offset + 4 + length).min(source_header.len());
            if kind != 4 && kind != 6 {
                blocks.push((kind, source_header[offset + 4..end].to_vec()));
            }
            let last = source_header[offset] >= 128;
            offset = end;
            if last {
                break;
            }
        }
    }
    if blocks.is_empty() {
        blocks.push((0, vec![0; 34]));
    }
    blocks.push((4, comment_block(&vorbis_comments(model))));
    let mut out = b"fLaC".to_vec();
    for (index, (kind, body)) in blocks.iter().enumerate() {
        out.extend(block(*kind, index + 1 == blocks.len(), body));
    }
    out
}

pub struct TagStream {
    model: TrackTagModel,
    head: Vec<u8>,
    mode: TagMode,
    to_skip: usize,
    embed_synced: bool,
}

enum TagMode {
    Probing,
    Skipping,
    Passthrough,
}

impl TagStream {
    pub fn new(model: TrackTagModel, embed_synced: bool) -> Self {
        Self {
            model,
            head: Vec::new(),
            mode: TagMode::Probing,
            to_skip: 0,
            embed_synced,
        }
    }

    pub fn push(&mut self, chunk: &[u8]) -> Result<Vec<u8>, DeezerError> {
        match self.mode {
            TagMode::Passthrough => Ok(chunk.to_vec()),
            TagMode::Skipping => {
                if chunk.len() <= self.to_skip {
                    self.to_skip -= chunk.len();
                    Ok(Vec::new())
                } else {
                    let rest = chunk[self.to_skip..].to_vec();
                    self.to_skip = 0;
                    self.mode = TagMode::Passthrough;
                    Ok(rest)
                }
            }
            TagMode::Probing => {
                self.head.extend_from_slice(chunk);
                if self.head.len() > 16 * 1024 * 1024 {
                    return Err(DeezerError::Message("metadata header exceeded 16 MiB".into()));
                }
                match probe_audio_offset(&self.head) {
                    Probe::NeedMore => Ok(Vec::new()),
                    Probe::Ready { audio_offset, flac } => {
                        let header = if flac {
                            if !self.embed_synced {
                                self.model.lyrics_synced = None;
                            }
                            flac_header(&self.head[..audio_offset], &self.model)
                        } else {
                            id3_bytes(&self.model)?
                        };
                        let mut out = header;
                        if audio_offset < self.head.len() {
                            out.extend_from_slice(&self.head[audio_offset..]);
                        } else {
                            self.to_skip = audio_offset - self.head.len();
                            self.mode = if self.to_skip == 0 { TagMode::Passthrough } else { TagMode::Skipping };
                            return Ok(out);
                        }
                        self.mode = TagMode::Passthrough;
                        self.head.clear();
                        Ok(out)
                    }
                }
            }
        }
    }

    pub fn finish(&mut self) -> Result<Vec<u8>, DeezerError> {
        if matches!(self.mode, TagMode::Probing) && !self.head.is_empty() {
            self.push(&[])
        } else {
            Ok(Vec::new())
        }
    }
}

pub fn add_track_tags(audio: &[u8], model: &TrackTagModel) -> Result<Vec<u8>, DeezerError> {
    let mut stream = TagStream::new(model.clone(), true);
    let mut output = stream.push(audio)?;
    output.extend(stream.finish()?);
    Ok(output)
}
