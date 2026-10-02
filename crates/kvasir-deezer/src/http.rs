use std::sync::OnceLock;
use std::time::Duration;

use bytes::Bytes;
use futures_core::Stream;
use futures_util::StreamExt;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use serde_json::Value;
use std::pin::Pin;
use tokio::sync::Semaphore;

use crate::error::DeezerError;

fn shared_client() -> reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT
        .get_or_init(|| {
            reqwest::Client::builder()
                .pool_max_idle_per_host(64)
                .connect_timeout(Duration::from_secs(15))
                .timeout(Duration::from_secs(30))
                .gzip(true)
                .brotli(true)
                .deflate(true)
                .redirect(reqwest::redirect::Policy::limited(10))
                .build()
                .expect("http client")
        })
        .clone()
}

fn slots() -> std::sync::Arc<Semaphore> {
    static SLOTS: OnceLock<std::sync::Arc<Semaphore>> = OnceLock::new();
    SLOTS.get_or_init(|| std::sync::Arc::new(Semaphore::new(64))).clone()
}

#[derive(Clone, Debug)]
pub struct HttpResponse {
    pub status: u16,
    pub headers: HeaderMap,
    pub final_url: String,
    pub bytes: Bytes,
}

impl HttpResponse {
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.bytes).into_owned()
    }

    pub fn json(&self) -> Result<Value, DeezerError> {
        serde_json::from_slice(&self.bytes).map_err(|err| DeezerError::Message(err.to_string()))
    }
}

#[derive(Clone)]
pub struct HttpClient {
    client: reqwest::Client,
    base_url: String,
    headers: Vec<(String, String)>,
    params: Vec<(String, String)>,
}

impl HttpClient {
    pub fn new(base_url: impl Into<String>, headers: Vec<(String, String)>, params: Vec<(String, String)>) -> Self {
        Self {
            client: shared_client(),
            base_url: base_url.into(),
            headers,
            params,
        }
    }

    pub fn with_param(&mut self, key: impl Into<String>, value: impl Into<String>) {
        let key = key.into();
        self.params.retain(|(existing, _)| existing != &key);
        self.params.push((key, value.into()));
    }

    pub fn param(&self, key: &str) -> Option<&str> {
        self.params
            .iter()
            .rev()
            .find(|(existing, _)| existing == key)
            .map(|(_, value)| value.as_str())
    }

    pub async fn request(
        &self,
        method: reqwest::Method,
        url: &str,
        query: &[(&str, String)],
        headers: &[(&str, String)],
        body: Option<Vec<u8>>,
        content_type: Option<&str>,
    ) -> Result<HttpResponse, DeezerError> {
        let pool = slots();
        let _permit = pool
            .acquire()
            .await
            .map_err(|_| DeezerError::Message("http pool closed".into()))?;
        let resolved = if url.starts_with("http://") || url.starts_with("https://") {
            url.to_string()
        } else {
            format!(
                "{}/{}",
                self.base_url.trim_end_matches('/'),
                url.trim_start_matches('/')
            )
        };
        let mut request = self.client.request(method, &resolved);
        let mut header_map = HeaderMap::new();
        for (key, value) in &self.headers {
            if let (Ok(name), Ok(value)) = (
                HeaderName::try_from(key.as_str()),
                HeaderValue::from_str(value),
            ) {
                header_map.insert(name, value);
            }
        }
        for (key, value) in headers {
            if let (Ok(name), Ok(value)) = (HeaderName::try_from(*key), HeaderValue::from_str(value)) {
                header_map.insert(name, value);
            }
        }
        if let Some(content_type) = content_type {
            header_map.insert(
                reqwest::header::CONTENT_TYPE,
                HeaderValue::from_str(content_type).unwrap_or(HeaderValue::from_static("application/json")),
            );
        }
        request = request.headers(header_map);
        let mut pairs = self.params.clone();
        for (key, value) in query {
            pairs.retain(|(existing, _)| existing != key);
            pairs.push(((*key).to_string(), value.clone()));
        }
        for (key, value) in pairs {
            request = request.query(&[(key, value)]);
        }
        if let Some(body) = body {
            request = request.body(body);
        }
        let response = request
            .send()
            .await
            .map_err(|err| DeezerError::Message(err.to_string()))?;
        let status = response.status().as_u16();
        let final_url = response.url().to_string();
        let headers = response.headers().clone();
        let bytes = response
            .bytes()
            .await
            .map_err(|err| DeezerError::Message(err.to_string()))?;
        if !(200..300).contains(&status) {
            return Err(DeezerError::HttpStatus {
                status,
                body: String::from_utf8_lossy(&bytes).chars().take(512).collect(),
            });
        }
        Ok(HttpResponse {
            status,
            headers,
            final_url,
            bytes,
        })
    }

    pub async fn get(&self, url: &str, query: &[(&str, String)], headers: &[(&str, String)]) -> Result<HttpResponse, DeezerError> {
        self.request(reqwest::Method::GET, url, query, headers, None, None).await
    }

    pub async fn post_json(&self, url: &str, body: &Value, query: &[(&str, String)]) -> Result<HttpResponse, DeezerError> {
        self.request(
            reqwest::Method::POST,
            url,
            query,
            &[],
            Some(serde_json::to_vec(body).unwrap_or_default()),
            Some("application/json; charset=UTF-8"),
        )
        .await
    }

    pub async fn head(&self, url: &str) -> Result<HttpResponse, DeezerError> {
        self.request(reqwest::Method::HEAD, url, &[], &[], None, None).await
    }
}

pub async fn get_bytes(url: &str, headers: &[(&str, String)]) -> Result<Bytes, DeezerError> {
    Ok(HttpClient::new("", Vec::new(), Vec::new())
        .get(url, &[], headers)
        .await?
        .bytes)
}

pub async fn get_json(url: &str, headers: &[(&str, String)]) -> Result<Value, DeezerError> {
    HttpClient::new("", Vec::new(), Vec::new()).get(url, &[], headers).await?.json()
}

pub async fn get_text(url: &str, headers: &[(&str, String)]) -> Result<String, DeezerError> {
    Ok(HttpClient::new("", Vec::new(), Vec::new()).get(url, &[], headers).await?.text())
}

pub fn content_length(headers: &HeaderMap) -> u64 {
    headers
        .get(reqwest::header::CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse().ok())
        .unwrap_or(0)
}

pub async fn byte_stream(
    url: &str,
    range_start: u64,
) -> Result<(Pin<Box<dyn Stream<Item = Result<Bytes, DeezerError>> + Send>>, u64), DeezerError> {
    let permit = slots()
        .acquire_owned()
        .await
        .map_err(|_| DeezerError::Message("http pool closed".into()))?;
    let client = shared_client();
    let mut request = client.get(url);
    if range_start > 0 {
        request = request.header("range", format!("bytes={range_start}-"));
    }
    let response = request
        .send()
        .await
        .map_err(|err| DeezerError::Message(err.to_string()))?;
    let status = response.status().as_u16();
    if status != 200 && status != 206 {
        let body = response.text().await.unwrap_or_default();
        return Err(DeezerError::HttpStatus {
            status,
            body: body.chars().take(512).collect(),
        });
    }
    let length = content_length(response.headers());
    let stream = async_stream::stream! {
        let _permit = permit;
        let mut incoming = response.bytes_stream();
        while let Some(item) = incoming.next().await {
            yield item.map_err(|err| DeezerError::Message(err.to_string()));
        }
    };
    Ok((Box::pin(stream), length))
}
