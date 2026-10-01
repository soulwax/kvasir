use std::collections::HashMap;
use std::future::Future;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use futures_util::FutureExt;
use serde_json::{json, Value};
use tokio::sync::Mutex as AsyncMutex;

use crate::cache::{CacheStats, TtlCache};
use crate::error::DeezerError;
use crate::http::HttpClient;
use crate::track::Track;

pub const DEFAULT_ARL: &str = "c973964816688562722418b5200c1515dffaad15a42643ebf87cc72824a54612ec51c2ad42d566743f9e424c774e98ccae7737770acff59251328e6cd598c7bcac38ca269adf78bfb88ec5bbad6cd800db3c0b88b2af645bb22b99e71de26416";

pub const RETRY_POLICY: RetryPolicy = RetryPolicy {
    code4_attempts: 6,
    auth_reinits: 3,
    token_refreshes: 15,
    base_ms: 800,
    max_delay_ms: 8000,
    deadline_ms: 30_000,
};

const USER_DATA_TTL: Duration = Duration::from_secs(25 * 60);
const API_KEY: &str = "ZAIVAHCEISOHWAICUQUEXAEPICENGUAFAEZAIPHAELEEVAHPHUCUFONGUAPASUAY";

const SHARED_METHODS: &[&str] = &[
    "album.getData",
    "artist.getData",
    "song.getLyrics",
    "album.getDiscography",
    "playlist.getData",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetryPolicy {
    pub code4_attempts: u32,
    pub auth_reinits: u32,
    pub token_refreshes: u32,
    pub base_ms: u64,
    pub max_delay_ms: u64,
    pub deadline_ms: u64,
}

#[derive(Clone, Debug)]
pub struct SessionUserData {
    pub license_token: String,
    pub country: String,
    pub can_stream_lossless: bool,
    pub can_stream_hq: bool,
    pub offer_id: Option<i64>,
}

#[derive(Clone)]
pub struct Session {
    inner: std::sync::Arc<SessionInner>,
}

struct SessionInner {
    arl: Mutex<String>,
    http: AsyncMutex<HttpClient>,
    cache: AsyncMutex<TtlCache>,
    inflight: AsyncMutex<HashMap<String, futures_util::future::Shared<futures_util::future::BoxFuture<'static, Result<Value, DeezerError>>>>>,
    user: AsyncMutex<Option<(SessionUserData, Instant)>>,
}

fn shared_state() -> &'static AsyncMutex<(TtlCache, HashMap<String, futures_util::future::Shared<futures_util::future::BoxFuture<'static, Result<Value, DeezerError>>>>)> {
    static SHARED: OnceLock<AsyncMutex<(TtlCache, HashMap<String, futures_util::future::Shared<futures_util::future::BoxFuture<'static, Result<Value, DeezerError>>>>)>> = OnceLock::new();
    SHARED.get_or_init(|| {
        AsyncMutex::new((
            TtlCache::new(2000, Duration::from_secs(60 * 60)),
            HashMap::new(),
        ))
    })
}

fn public_cache() -> &'static AsyncMutex<(TtlCache, HashMap<String, futures_util::future::Shared<futures_util::future::BoxFuture<'static, Result<Value, DeezerError>>>>)> {
    static PUBLIC: OnceLock<AsyncMutex<(TtlCache, HashMap<String, futures_util::future::Shared<futures_util::future::BoxFuture<'static, Result<Value, DeezerError>>>>)>> = OnceLock::new();
    PUBLIC.get_or_init(|| {
        AsyncMutex::new((
            TtlCache::new(1000, Duration::from_secs(60 * 60)),
            HashMap::new(),
        ))
    })
}

fn default_session() -> &'static Mutex<Session> {
    static DEFAULT: OnceLock<Mutex<Session>> = OnceLock::new();
    DEFAULT.get_or_init(|| Mutex::new(Session::new(None)))
}

fn backoff(attempt: u32) -> Duration {
    let window = RETRY_POLICY.base_ms.saturating_mul(1u64 << attempt.min(16)).min(RETRY_POLICY.max_delay_ms);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.subsec_nanos() as u64)
        .unwrap_or(0);
    let half = window / 2;
    Duration::from_millis(half + nanos % half.max(1))
}

fn cache_key(body: &Value) -> String {
    serde_json::to_string(body).unwrap_or_else(|_| body.to_string())
}

fn results_present(results: &Value) -> bool {
    match results {
        Value::Null => false,
        Value::Object(map) => !map.is_empty(),
        Value::Array(items) => !items.is_empty(),
        Value::Bool(_) | Value::Number(_) | Value::String(_) => true,
    }
}

impl Session {
    pub fn new(arl: Option<&str>) -> Self {
        let http = HttpClient::new(
            "https://www.deezer.com/ajax",
            vec![
                ("Accept".into(), "*/*".into()),
                ("Accept-Language".into(), "en-US".into()),
                ("Cache-Control".into(), "no-cache".into()),
                ("User-Agent".into(), "Deezer/8.32.0.2 (iOS; 14.4; Mobile; en; iPhone10_5)".into()),
            ],
            vec![
                ("version".into(), "8.32.0".into()),
                ("api_key".into(), API_KEY.into()),
                ("output".into(), "3".into()),
                ("input".into(), "3".into()),
                ("buildId".into(), "ios12_universal".into()),
                ("screenHeight".into(), "480".into()),
                ("screenWidth".into(), "320".into()),
                ("lang".into(), "en".into()),
            ],
        );
        Self {
            inner: std::sync::Arc::new(SessionInner {
                arl: Mutex::new(arl.unwrap_or(DEFAULT_ARL).to_string()),
                http: AsyncMutex::new(http),
                cache: AsyncMutex::new(TtlCache::new(1000, Duration::from_secs(60 * 60))),
                inflight: AsyncMutex::new(HashMap::new()),
                user: AsyncMutex::new(None),
            }),
        }
    }

    pub fn arl(&self) -> String {
        self.inner.arl.lock().expect("arl").clone()
    }

    pub async fn sid(&self) -> Option<String> {
        self.inner.http.lock().await.param("sid").map(str::to_string)
    }

    pub async fn api_token(&self) -> Option<String> {
        self.inner.http.lock().await.param("api_token").map(str::to_string)
    }

    pub async fn country(&self) -> Option<String> {
        self.inner.user.lock().await.as_ref().map(|(user, _)| user.country.clone())
    }

    pub async fn license_token(&self) -> Option<String> {
        self.inner
            .user
            .lock()
            .await
            .as_ref()
            .map(|(user, _)| user.license_token.clone())
    }

    pub async fn can_stream_lossless(&self) -> bool {
        self.inner
            .user
            .lock()
            .await
            .as_ref()
            .is_some_and(|(user, _)| user.can_stream_lossless)
    }

    pub async fn can_stream_hq(&self) -> bool {
        self.inner
            .user
            .lock()
            .await
            .as_ref()
            .is_some_and(|(user, _)| user.can_stream_hq)
    }

    pub async fn init(&self, arl: Option<&str>) -> Result<String, DeezerError> {
        if let Some(arl) = arl {
            if arl.len() != 192 {
                return Err(DeezerError::Message(format!(
                    "Invalid arl. Length should be 192 characters. You have provided {} characters.",
                    arl.len()
                )));
            }
            *self.inner.arl.lock().expect("arl") = arl.to_string();
            *self.inner.user.lock().await = None;
            self.inner.cache.lock().await.clear();
        }
        let cookie = format!("arl={}", self.arl());
        let response = self
            .inner
            .http
            .lock()
            .await
            .get(
                "https://www.deezer.com/ajax/gw-light.php",
                &[
                    ("method", "deezer.ping".into()),
                    ("api_version", "1.0".into()),
                    ("api_token", String::new()),
                ],
                &[("cookie", cookie)],
            )
            .await?;
        let data = response.json()?;
        let session = data
            .pointer("/results/SESSION")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        if session.is_empty() {
            return Err(DeezerError::gateway(data.pointer("/error").cloned().unwrap_or(json!({}))));
        }
        self.inner.http.lock().await.with_param("sid", session.clone());
        Ok(session)
    }

    pub async fn refresh_api_token(&self) -> Result<String, DeezerError> {
        let data = self
            .inner
            .http
            .lock()
            .await
            .get(
                "https://www.deezer.com/ajax/gw-light.php",
                &[
                    ("method", "deezer.getUserData".into()),
                    ("api_version", "1.0".into()),
                    ("api_token", "null".into()),
                ],
                &[],
            )
            .await?
            .json()?;
        let token = data.pointer("/results/checkForm").and_then(Value::as_str).unwrap_or("").to_string();
        let sid = data.pointer("/results/SESSION_ID").and_then(Value::as_str).unwrap_or("").to_string();
        let mut http = self.inner.http.lock().await;
        if !token.is_empty() {
            http.with_param("api_token", token.clone());
        }
        if !sid.is_empty() {
            http.with_param("sid", sid);
        }
        Ok(token)
    }

    pub async fn load_user_data(&self, force: bool) -> Result<SessionUserData, DeezerError> {
        if !force {
            if let Some((user, at)) = self.inner.user.lock().await.clone() {
                if at.elapsed() < USER_DATA_TTL {
                    return Ok(user);
                }
            }
        }
        let data = self
            .inner
            .http
            .lock()
            .await
            .get(
                "https://www.deezer.com/ajax/gw-light.php",
                &[
                    ("method", "deezer.getUserData".into()),
                    ("api_version", "1.0".into()),
                    ("api_token", "null".into()),
                ],
                &[],
            )
            .await?
            .json()?;
        let options = data.pointer("/results/USER/OPTIONS").cloned().unwrap_or(Value::Null);
        let user = SessionUserData {
            license_token: options.get("license_token").and_then(Value::as_str).unwrap_or("").to_string(),
            country: data.pointer("/results/COUNTRY").and_then(Value::as_str).unwrap_or("").to_string(),
            can_stream_lossless: options.get("web_lossless").and_then(Value::as_bool).unwrap_or(false)
                || options.get("mobile_loseless").and_then(Value::as_bool).unwrap_or(false),
            can_stream_hq: options.get("web_hq").and_then(Value::as_bool).unwrap_or(false)
                || options.get("mobile_hq").and_then(Value::as_bool).unwrap_or(false),
            offer_id: data.pointer("/results/OFFER_ID").and_then(Value::as_i64),
        };
        if let Some(token) = data.pointer("/results/checkForm").and_then(Value::as_str) {
            self.inner.http.lock().await.with_param("api_token", token);
        }
        if let Some(sid) = data.pointer("/results/SESSION_ID").and_then(Value::as_str) {
            self.inner.http.lock().await.with_param("sid", sid);
        }
        *self.inner.user.lock().await = Some((user.clone(), Instant::now()));
        Ok(user)
    }

    pub async fn invalidate_user_data(&self) {
        *self.inner.user.lock().await = None;
    }

    pub async fn request(&self, method: &str, url: &str, body: Option<Value>, query: &[(&str, String)]) -> Result<Value, DeezerError> {
        let started = Instant::now();
        let mut auth_reinits = 0u32;
        let mut token_refreshes = 0u32;
        let mut code4_attempts = 0u32;
        loop {
            let response = if method.eq_ignore_ascii_case("POST") {
                self.inner
                    .http
                    .lock()
                    .await
                    .post_json(url, body.as_ref().unwrap_or(&json!({})), query)
                    .await?
            } else {
                self.inner.http.lock().await.get(url, query, &[]).await?
            };
            let data = response.json()?;
            let error = data.get("error").cloned().unwrap_or(Value::Null);
            let empty = error.as_object().is_none_or(|map| map.is_empty()) || error.is_null();
            if empty {
                return Ok(data);
            }
            let gateway = DeezerError::gateway(error.clone());
            let over_deadline = started.elapsed() > Duration::from_millis(RETRY_POLICY.deadline_ms);
            let keys = error.as_object().map(|map| map.keys().cloned().collect::<Vec<_>>()).unwrap_or_default();
            if keys.iter().any(|key| key == "NEED_API_AUTH_REQUIRED") && auth_reinits < RETRY_POLICY.auth_reinits && !over_deadline {
                auth_reinits += 1;
                self.init(None).await?;
                continue;
            }
            if keys.iter().any(|key| key == "GATEWAY_ERROR" || key == "VALID_TOKEN_REQUIRED")
                && token_refreshes < RETRY_POLICY.token_refreshes
                && !over_deadline
            {
                token_refreshes += 1;
                self.refresh_api_token().await?;
                tokio::time::sleep(backoff(token_refreshes - 1)).await;
                continue;
            }
            if error.get("code").and_then(Value::as_i64) == Some(4) && code4_attempts < RETRY_POLICY.code4_attempts && !over_deadline {
                tokio::time::sleep(backoff(code4_attempts)).await;
                code4_attempts += 1;
                continue;
            }
            return Err(gateway);
        }
    }

    async fn coalesce<F, Fut>(&self, key: String, shared: bool, build: F) -> Result<Value, DeezerError>
    where
        F: FnOnce(Session) -> Fut,
        Fut: Future<Output = Result<Value, DeezerError>> + Send + 'static,
    {
        if shared {
            let mut guard = shared_state().lock().await;
            if let Some(hit) = guard.0.get(&key) {
                return Ok(hit);
            }
            let pending = if let Some(existing) = guard.1.get(&key) {
                existing.clone()
            } else {
                let future = build(self.clone()).boxed().shared();
                guard.1.insert(key.clone(), future.clone());
                future
            };
            drop(guard);
            let result = pending.await;
            let mut guard = shared_state().lock().await;
            guard.1.remove(&key);
            if let Ok(value) = &result {
                guard.0.insert(key, value.clone());
            }
            result
        } else {
            if let Some(hit) = self.inner.cache.lock().await.get(&key) {
                return Ok(hit);
            }
            let pending = {
                let mut inflight = self.inner.inflight.lock().await;
                if let Some(existing) = inflight.get(&key) {
                    existing.clone()
                } else {
                    let future = build(self.clone()).boxed().shared();
                    inflight.insert(key.clone(), future.clone());
                    future
                }
            };
            let result = pending.await;
            self.inner.inflight.lock().await.remove(&key);
            if let Ok(value) = &result {
                self.inner.cache.lock().await.insert(key, value.clone());
            }
            result
        }
    }

    pub async fn gw(&self, body: Value, method: &str) -> Result<Value, DeezerError> {
        let shared = SHARED_METHODS.contains(&method);
        let country = self.country().await.unwrap_or_else(|| "XX".into());
        let key = if shared {
            format!("{country}:gw:{method}:{}", cache_key(&body))
        } else {
            format!("gw:{method}:{}", cache_key(&body))
        };
        let method = method.to_string();
        self.coalesce(key, shared, move |session| async move {
            let data = session
                .request("POST", "https://api.deezer.com/1.0/gateway.php", Some(body), &[("method", method)])
                .await?;
            let results = data.get("results").cloned().unwrap_or(Value::Null);
            if results_present(&results) {
                Ok(results)
            } else {
                Err(DeezerError::gateway(data.get("error").cloned().unwrap_or(json!({}))))
            }
        })
        .await
    }

    pub async fn gw_light(&self, body: Value, method: &str) -> Result<Value, DeezerError> {
        let key = format!("gwl:{method}:{}", cache_key(&body));
        let method = method.to_string();
        self.coalesce(key, false, move |session| async move {
            let data = session
                .request(
                    "POST",
                    "https://www.deezer.com/ajax/gw-light.php",
                    Some(body),
                    &[("method", method), ("api_version", "1.0".into())],
                )
                .await?;
            let results = data.get("results").cloned().unwrap_or(Value::Null);
            if results_present(&results) {
                Ok(results)
            } else {
                Err(DeezerError::gateway(data.get("error").cloned().unwrap_or(json!({}))))
            }
        })
        .await
    }

    pub async fn gw_get(&self, method: &str, params: Vec<(String, String)>) -> Result<Value, DeezerError> {
        let key = format!("gwget:{method}:{}", params.iter().map(|(k, v)| format!("{k}={v}")).collect::<Vec<_>>().join("&"));
        let method = method.to_string();
        self.coalesce(key, false, move |session| async move {
            let mut query = vec![("method".to_string(), method)];
            query.extend(params);
            let query_ref: Vec<(&str, String)> = query.iter().map(|(key, value)| (key.as_str(), value.clone())).collect();
            let data = session
                .request("GET", "https://api.deezer.com/1.0/gateway.php", None, &query_ref)
                .await?;
            let results = data.get("results").cloned().unwrap_or(Value::Null);
            if results_present(&results) {
                Ok(results)
            } else {
                Err(DeezerError::gateway(data.get("error").cloned().unwrap_or(json!({}))))
            }
        })
        .await
    }

    pub async fn get_user(&self) -> Result<Value, DeezerError> {
        self.gw_get("user_getInfo", Vec::new()).await
    }

    pub async fn get_track_info(&self, song_id: &str) -> Result<Track, DeezerError> {
        Ok(Track::new(self.gw(json!({"sng_id": song_id}), "song.getData").await?))
    }

    pub async fn get_lyrics(&self, song_id: &str) -> Result<Value, DeezerError> {
        self.gw(json!({"sng_id": song_id}), "song.getLyrics").await
    }

    pub async fn get_album_info(&self, album_id: &str) -> Result<Value, DeezerError> {
        self.gw(json!({"alb_id": album_id}), "album.getData").await
    }

    pub async fn get_album_tracks(&self, album_id: &str) -> Result<Value, DeezerError> {
        self.gw(json!({"alb_id": album_id, "lang": "us", "nb": -1}), "song.getListByAlbum").await
    }

    pub async fn get_playlist_info(&self, playlist_id: &str) -> Result<Value, DeezerError> {
        self.gw(json!({"playlist_id": playlist_id, "lang": "en"}), "playlist.getData").await
    }

    pub async fn get_playlist_tracks(&self, playlist_id: &str) -> Result<Value, DeezerError> {
        let mut results = self
            .gw(
                json!({"playlist_id": playlist_id, "lang": "en", "nb": -1, "start": 0, "tab": 0, "tags": true, "header": true}),
                "playlist.getSongs",
            )
            .await?;
        if let Some(items) = results.get_mut("data").and_then(Value::as_array_mut) {
            for (index, track) in items.iter_mut().enumerate() {
                if let Some(object) = track.as_object_mut() {
                    object.insert("TRACK_POSITION".into(), json!(index + 1));
                }
            }
        }
        Ok(results)
    }

    pub async fn get_artist_info(&self, artist_id: &str) -> Result<Value, DeezerError> {
        self.gw(
            json!({"art_id": artist_id, "filter_role_id": [0], "lang": "en", "tab": 0, "nb": -1, "start": 0}),
            "artist.getData",
        )
        .await
    }

    pub async fn get_discography(&self, artist_id: &str, nb: i64) -> Result<Value, DeezerError> {
        self.gw(
            json!({"art_id": artist_id, "filter_role_id": [0], "lang": "en", "nb": nb, "nb_songs": -1, "start": 0}),
            "album.getDiscography",
        )
        .await
    }

    pub async fn get_profile(&self, user_id: &str) -> Result<Value, DeezerError> {
        self.gw(json!({"user_id": user_id, "tab": "loved", "nb": -1}), "mobile.pageUser").await
    }

    pub async fn search_music(&self, query: &str, types: &[&str], nb: i64) -> Result<Value, DeezerError> {
        self.gw_light(
            json!({
                "query": query,
                "start": 0,
                "nb": nb,
                "suggest": true,
                "artist_suggest": true,
                "top_tracks": true,
                "types": types,
            }),
            "deezer.pageSearch",
        )
        .await
    }

    pub async fn suggest(&self, query: &str, nb: i64) -> Result<Value, DeezerError> {
        self.gw_light(
            json!({
                "QUERY": query,
                "NB": nb,
                "TYPES": {"ALBUM": true, "ARTIST": true, "TRACK": true, "PLAYLIST": true, "RADIO": true, "SHOW": true},
            }),
            "deezer.suggest",
        )
        .await
    }

    pub async fn search_alternative(&self, artist: &str, song: &str, nb: i64) -> Result<Value, DeezerError> {
        self.gw(
            json!({"query": format!("artist:'{artist}' track:'{song}'"), "types": ["TRACK"], "nb": nb}),
            "mobile_suggest",
        )
        .await
    }
}

pub fn current_session() -> Session {
    default_session().lock().expect("default session").clone()
}

pub async fn init_deezer_api(arl: &str) -> Result<String, DeezerError> {
    let session = current_session();
    let sid = session.init(Some(arl)).await?;
    *default_session().lock().expect("default session") = session;
    Ok(sid)
}

pub async fn create_session(arl: Option<&str>) -> Result<Session, DeezerError> {
    let session = Session::new(arl);
    session.init(None).await?;
    Ok(session)
}

pub fn set_default_session(session: Session) {
    *default_session().lock().expect("default session") = session;
}

pub fn configure_cache(shared_max: usize, shared_ttl: Duration, public_max: usize, public_ttl: Duration) {
    if let Ok(mut guard) = shared_state().try_lock() {
        guard.0 = TtlCache::new(shared_max, shared_ttl);
    }
    if let Ok(mut guard) = public_cache().try_lock() {
        guard.0 = TtlCache::new(public_max, public_ttl);
    }
}

pub async fn clear_shared_caches() {
    shared_state().lock().await.0.clear();
    public_cache().lock().await.0.clear();
}

pub async fn cache_stats() -> CacheStats {
    CacheStats {
        shared: shared_state().lock().await.0.stats(),
        public_api: public_cache().lock().await.0.stats(),
    }
}

pub async fn request_public_api(slug: &str) -> Result<Value, DeezerError> {
    let key = slug.to_string();
    {
        let mut guard = public_cache().lock().await;
        if let Some(hit) = guard.0.get(&key) {
            return Ok(hit);
        }
        let pending = if let Some(existing) = guard.1.get(&key) {
            existing.clone()
        } else {
            let url = format!("https://api.deezer.com{slug}");
            let future = async move {
                let value = crate::http::get_json(&url, &[]).await?;
                if value.get("error").is_some() && !value.get("error").unwrap().is_null() {
                    return Err(DeezerError::gateway(value.get("error").cloned().unwrap_or(json!({}))));
                }
                Ok(value)
            }
            .boxed()
            .shared();
            guard.1.insert(key.clone(), future.clone());
            future
        };
        drop(guard);
        let result = pending.await;
        let mut guard = public_cache().lock().await;
        guard.1.remove(&key);
        if let Ok(value) = &result {
            guard.0.insert(key, value.clone());
        }
        result
    }
}
