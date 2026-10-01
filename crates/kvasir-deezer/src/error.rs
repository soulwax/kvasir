use serde_json::Value;
use thiserror::Error;

const RETRYABLE_KEYS: &[&str] = &[
    "NEED_API_AUTH_REQUIRED",
    "GATEWAY_ERROR",
    "VALID_TOKEN_REQUIRED",
];

#[derive(Debug, Clone, PartialEq, Error)]
pub enum DeezerError {
    #[error("{message}")]
    Gateway {
        message: String,
        code: Option<i64>,
        keys: Vec<String>,
        retryable: bool,
        payload: Value,
    },
    #[error("Your account can't stream {0} tracks")]
    WrongLicense(String),
    #[error("This track is not available in your country ({0})")]
    GeoBlocked(String),
    #[error("Track token for {0} has expired — re-fetch the track and retry")]
    ExpiredTrackToken(String),
    #[error("HTTP {status}")]
    HttpStatus { status: u16, body: String },
    #[error("{0}")]
    Message(String),
}

impl DeezerError {
    pub fn gateway(payload: Value) -> Self {
        let object = payload.as_object();
        let keys = object
            .map(|map| map.keys().cloned().collect::<Vec<_>>())
            .unwrap_or_default();
        let code = object
            .and_then(|map| map.get("code"))
            .and_then(Value::as_i64);
        let message = if keys.is_empty() {
            "Deezer request failed".into()
        } else {
            keys.iter()
                .map(|key| format!("{key}: {}", payload.get(key).unwrap_or(&Value::Null)))
                .collect::<Vec<_>>()
                .join(", ")
        };
        let retryable = code == Some(4) || keys.iter().any(|key| RETRYABLE_KEYS.contains(&key.as_str()));
        Self::Gateway {
            message,
            code,
            keys,
            retryable,
            payload,
        }
    }

    pub fn is_retryable(&self) -> bool {
        matches!(self, Self::Gateway { retryable: true, .. })
    }
}
