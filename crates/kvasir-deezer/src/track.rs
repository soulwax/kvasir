use serde_json::{json, Map, Value};

#[derive(Clone, Debug)]
pub struct Track {
    pub raw: Value,
}

impl Track {
    pub fn new(raw: Value) -> Self {
        Self { raw }
    }

    fn field(&self, key: &str) -> String {
        match self.raw.get(key) {
            Some(Value::String(value)) => value.clone(),
            Some(Value::Number(value)) => value.to_string(),
            Some(Value::Bool(value)) => value.to_string(),
            _ => String::new(),
        }
    }

    fn set(&mut self, key: &str, value: Value) {
        if let Value::Object(map) = &mut self.raw {
            map.insert(key.to_string(), value);
        }
    }

    pub fn sng_id(&self) -> String {
        self.field("SNG_ID")
    }
    pub fn title(&self) -> String {
        self.field("SNG_TITLE")
    }
    pub fn artist_name(&self) -> String {
        self.field("ART_NAME")
    }
    pub fn artist_id(&self) -> String {
        self.field("ART_ID")
    }
    pub fn album_id(&self) -> String {
        self.field("ALB_ID")
    }
    pub fn album_title(&self) -> String {
        self.field("ALB_TITLE")
    }
    pub fn duration_seconds(&self) -> u64 {
        self.field("DURATION").parse().unwrap_or(0)
    }
    pub fn track_number(&self) -> u32 {
        self.field("TRACK_NUMBER").parse().unwrap_or(0)
    }
    pub fn disk_number(&self) -> u32 {
        self.field("DISK_NUMBER").parse().unwrap_or(0)
    }
    pub fn isrc(&self) -> String {
        self.field("ISRC")
    }
    pub fn gain(&self) -> String {
        self.field("GAIN")
    }
    pub fn rank(&self) -> String {
        self.field("RANK")
    }
    pub fn version(&self) -> String {
        self.field("VERSION")
    }
    pub fn url_rewriting(&self) -> String {
        self.field("URL_REWRITING")
    }
    pub fn provider_id(&self) -> String {
        self.field("PROVIDER_ID")
    }
    pub fn md5_origin(&self) -> String {
        self.field("MD5_ORIGIN")
    }
    pub fn media_version(&self) -> String {
        self.field("MEDIA_VERSION")
    }
    pub fn track_token(&self) -> String {
        self.field("TRACK_TOKEN")
    }
    pub fn track_token_expire(&self) -> i64 {
        self.field("TRACK_TOKEN_EXPIRE").parse().unwrap_or(0)
    }
    pub fn album_picture(&self) -> String {
        self.field("ALB_PICTURE")
    }
    pub fn artist_picture(&self) -> String {
        if let Some(picture) = self.raw.get("ART_PICTURE").and_then(Value::as_str) {
            if !picture.is_empty() {
                return picture.to_string();
            }
        }
        self.artists()
            .into_iter()
            .find_map(|artist| artist.picture.filter(|value| !value.is_empty()))
            .unwrap_or_default()
    }
    pub fn filesize(&self, format: &str) -> u64 {
        let key = match format {
            "FLAC" => "FILESIZE_FLAC",
            "MP3_320" => "FILESIZE_MP3_320",
            "MP3_256" => "FILESIZE_MP3_256",
            "MP3_128" => "FILESIZE_MP3_128",
            "MP3_64" => "FILESIZE_MP3_64",
            "AAC_64" => "FILESIZE_AAC_64",
            "MP4_RA1" => "FILESIZE_MP4_RA1",
            "MP4_RA2" => "FILESIZE_MP4_RA2",
            "MP4_RA3" => "FILESIZE_MP4_RA3",
            _ => return 0,
        };
        self.field(key).parse().unwrap_or(0)
    }
    pub fn contributors(&self) -> Value {
        self.raw.get("SNG_CONTRIBUTORS").cloned().unwrap_or(Value::Null)
    }
    pub fn explicit_status(&self) -> Option<i64> {
        self.raw
            .pointer("/EXPLICIT_TRACK_CONTENT/EXPLICIT_LYRICS_STATUS")
            .and_then(Value::as_i64)
            .or_else(|| {
                self.raw
                    .pointer("/EXPLICIT_TRACK_CONTENT/EXPLICIT_LYRICS_STATUS")
                    .and_then(Value::as_str)
                    .and_then(|value| value.parse().ok())
            })
    }
    pub fn artists(&self) -> Vec<ArtistCredit> {
        self.raw
            .get("ARTISTS")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .map(|item| ArtistCredit {
                        name: item.get("ART_NAME").and_then(Value::as_str).unwrap_or("").to_string(),
                        id: item.get("ART_ID").map(|value| match value {
                            Value::String(text) => text.clone(),
                            Value::Number(number) => number.to_string(),
                            _ => String::new(),
                        }).unwrap_or_default(),
                        picture: item.get("ART_PICTURE").and_then(Value::as_str).map(str::to_string),
                    })
                    .collect()
            })
            .unwrap_or_default()
    }
    pub fn preview_href(&self) -> Option<String> {
        self.raw.get("MEDIA").and_then(Value::as_array).and_then(|media| {
            media.iter().find_map(|item| {
                let kind = item.get("TYPE").and_then(Value::as_str).unwrap_or("preview");
                let href = item.get("HREF").and_then(Value::as_str)?;
                (kind == "preview" && !href.is_empty()).then(|| href.to_string())
            })
        })
    }
    pub fn with_tokens(mut self, token: &str, expire: i64) -> Self {
        self.set("TRACK_TOKEN", Value::String(token.to_string()));
        self.set("TRACK_TOKEN_EXPIRE", json!(expire));
        self
    }
    pub fn merge_missing(&mut self, full: &Track, keys: &[&str]) {
        let Value::Object(source) = &full.raw else {
            return;
        };
        if !self.raw.is_object() {
            self.raw = Value::Object(Map::new());
        }
        let Value::Object(dest) = &mut self.raw else {
            return;
        };
        for key in keys {
            let missing = match dest.get(*key) {
                None | Some(Value::Null) => true,
                Some(Value::String(value)) => value.is_empty(),
                _ => false,
            };
            if missing {
                if let Some(value) = source.get(*key) {
                    dest.insert((*key).to_string(), value.clone());
                }
            }
        }
    }
}

#[derive(Clone, Debug)]
pub struct ArtistCredit {
    pub name: String,
    pub id: String,
    pub picture: Option<String>,
}

pub fn tracks_from_list(value: &Value) -> Vec<Track> {
    value
        .get("data")
        .and_then(Value::as_array)
        .or_else(|| value.as_array())
        .map(|items| items.iter().cloned().map(Track::new).collect())
        .unwrap_or_default()
}
