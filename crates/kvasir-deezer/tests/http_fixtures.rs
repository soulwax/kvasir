use kvasir_deezer::{
    clear_shared_caches, current_session, get_track_download_url, request_public_api, set_default_session, ApiRoots,
    DeezerError, Quality, Session, Track,
};
use serde_json::json;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn roots(server: &MockServer) -> ApiRoots {
    let base = server.uri();
    ApiRoots {
        gateway: format!("{base}/1.0/gateway.php"),
        light: format!("{base}/ajax/gw-light.php"),
        media: format!("{base}/v1/get_url"),
        public_api: base,
    }
}

#[tokio::test]
async fn gateway_reinitializes_once_then_returns_the_track() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/1.0/gateway.php"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"results": {"SNG_ID": "1", "SNG_TITLE": "Song"}})))
        .with_priority(5)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/ajax/gw-light.php"))
        .and(query_param("method", "deezer.ping"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"results": {"SESSION": "sid-1"}})))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/1.0/gateway.php"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"error": {"NEED_API_AUTH_REQUIRED": "auth"}})))
        .up_to_n_times(1)
        .with_priority(1)
        .mount(&server)
        .await;

    let session = Session::with_roots(None, roots(&server));
    let track = session.gw(json!({"sng_id": "1"}), "song.getData").await.unwrap();
    assert_eq!(track["SNG_TITLE"], "Song");
    assert_eq!(session.sid().await.as_deref(), Some("sid-1"));
}

#[tokio::test]
async fn token_refresh_then_succeeds() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/1.0/gateway.php"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"results": {"ok": true}})))
        .with_priority(5)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/ajax/gw-light.php"))
        .and(query_param("method", "deezer.getUserData"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"results": {"checkForm": "fresh", "SESSION_ID": "sid-2"}})))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/1.0/gateway.php"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"error": {"VALID_TOKEN_REQUIRED": "stale"}})))
        .up_to_n_times(1)
        .with_priority(1)
        .mount(&server)
        .await;

    let session = Session::with_roots(None, roots(&server));
    let value = session.gw(json!({"sng_id": "9"}), "song.getData").await.unwrap();
    assert_eq!(value["ok"], true);
    assert_eq!(session.api_token().await.as_deref(), Some("fresh"));
}

#[tokio::test]
async fn geo_blocked_media_entry_is_typed() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/ajax/gw-light.php"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "results": {
                "COUNTRY": "FR",
                "USER": {"OPTIONS": {"license_token": "lic", "web_hq": true, "web_lossless": true}}
            }
        })))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/get_url"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": [{"errors": [{"code": 2002}]}]
        })))
        .mount(&server)
        .await;

    let session = Session::with_roots(None, roots(&server));
    let track = Track::new(json!({"SNG_ID": "1", "TRACK_TOKEN": "tok", "TRACK_TOKEN_EXPIRE": 4_000_000_000i64}));
    let err = get_track_download_url(&session, &track, &Quality::Mp3_128).await.unwrap_err();
    assert!(matches!(err, DeezerError::GeoBlocked(country) if country == "FR"));
}

#[tokio::test]
async fn public_rest_error_becomes_deezer_error() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/track/missing"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"error": {"code": 800, "message": "no data"}})))
        .mount(&server)
        .await;
    let previous = current_session();
    set_default_session(Session::with_roots(None, roots(&server)));
    clear_shared_caches().await;
    let err = request_public_api("/track/missing").await.unwrap_err();
    set_default_session(previous);
    match err {
        DeezerError::Gateway { code, .. } => assert_eq!(code, Some(800)),
        other => panic!("unexpected {other}"),
    }
}
