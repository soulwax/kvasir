use kvasir_deezer::{
    build_advanced_query, build_tag_model, decrypt_download, encrypt_download, get_url_parts, normalize_contributors,
    probe_audio_offset, song_file_name, to_lrc, ExplicitLabel, LrcMeta, Probe, Track,
    TrackDecryptor,
};
use serde_json::json;

#[test]
fn stripe_decrypt_matches_verified_track_fixture() {
    let encrypted = include_bytes!("fixtures/stripe-3135556.enc.bin");
    let plain = include_bytes!("fixtures/stripe-3135556.dec.bin");
    assert_eq!(decrypt_download(encrypted, "3135556").as_slice(), plain.as_slice());
}

#[test]
fn stripe_decrypt_round_trips_synthetic_audio() {
    let plain = {
        let mut bytes = vec![0u8; 2048 * 4 + 10];
        for (index, byte) in bytes.iter_mut().enumerate() {
            *byte = (index % 251) as u8;
        }
        bytes
    };
    let encrypted = encrypt_download(&plain, "3135556");
    assert_ne!(encrypted[..2048], plain[..2048]);
    assert_eq!(encrypted[2048..4096], plain[2048..4096]);
    assert_eq!(decrypt_download(&encrypted, "3135556"), plain);

    let mut decryptor = TrackDecryptor::new("3135556", 0);
    let mut out = decryptor.push(&encrypted[..3000]);
    out.extend(decryptor.push(&encrypted[3000..]));
    out.extend(decryptor.finish());
    assert_eq!(out, plain);
}

#[test]
fn legacy_filename_is_stable_hex() {
    let name = song_file_name("0123456789abcdef0123456789abcdef", 3, "42", "1").unwrap();
    assert_eq!(name.len() % 32, 0);
    assert!(name.chars().all(|ch| ch.is_ascii_hexdigit()));
}

#[test]
fn advanced_query_and_url_parts() {
    let query = build_advanced_query(&json!({"artist": "daft punk", "durMin": 200, "bpmMax": 130}));
    assert_eq!(query, "artist:\"daft punk\" dur_min:200 bpm_max:130");
    let parts = futures_executor_block(get_url_parts("https://www.deezer.com/track/3135556"));
    assert_eq!(parts.kind, "track");
    assert_eq!(parts.id, "3135556");
    let spotify = futures_executor_block(get_url_parts("spotify:album:2noRn2Aes5aoNVsU6iWThc"));
    assert_eq!(spotify.kind, "spotify-album");
    let youtube = futures_executor_block(get_url_parts("https://youtu.be/dQw4w9WgXcQ"));
    assert_eq!(youtube.kind, "youtube-track");
    assert_eq!(youtube.id, "dQw4w9WgXcQ");
}

fn futures_executor_block<F, T>(future: F) -> T
where
    F: std::future::Future<Output = Result<T, kvasir_deezer::DeezerError>>,
{
    let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    runtime.block_on(future).unwrap()
}

#[test]
fn contributors_lyrics_and_tag_model() {
    let credits = normalize_contributors(&json!({
        "main_artist": ["Ada"],
        "featuring": ["Bea"],
        "composer": ["Cy"],
        "mastering engineer": ["Dee"]
    }));
    assert_eq!(credits.main_artists, vec!["Ada"]);
    assert_eq!(credits.featuring, vec!["Bea"]);
    assert_eq!(credits.engineers[0].role, "mastering engineer");
    let lrc = to_lrc(Some(&json!([{"line": "hello", "milliseconds": 1500}])), &LrcMeta {
        title: Some("Song".into()),
        artist: Some("Ada".into()),
        album: None,
        writers: None,
        length_seconds: None,
    }).unwrap();
    assert!(lrc.contains("[ar:Ada]"));
    assert!(lrc.contains("[00:01.50]hello"));
    assert!(lrc.contains("[re:kvasir]"));

    let track = Track::new(json!({
        "SNG_ID": "1",
        "SNG_TITLE": "Song",
        "ART_NAME": "Ada",
        "ALB_TITLE": "Album",
        "DURATION": "180",
        "TRACK_NUMBER": "2",
        "DISK_NUMBER": "1",
        "ISRC": "USAAA0000001",
        "GAIN": "-3.2",
        "SNG_CONTRIBUTORS": {"main_artist": ["Ada"]}
    }));
    let model = build_tag_model(&track, None, None, None, None, None, 1000, true, true);
    assert_eq!(model.title, "Song");
    assert_eq!(model.duration_ms, 180_000);
    assert_eq!(model.replay_gain_track_gain.as_deref(), Some("-3.20 dB"));
    assert_eq!(model.explicit, ExplicitLabel::Unknown);
    assert_eq!(model.deezer_track_id.as_deref(), Some("1"));
}

#[test]
fn probe_distinguishes_flac_and_mp3_headers() {
    let mut flac = b"fLaC".to_vec();
    flac.push(0x80);
    flac.extend_from_slice(&34u32.to_be_bytes()[1..]);
    flac.extend(std::iter::repeat(0).take(34));
    match probe_audio_offset(&flac) {
        Probe::Ready { audio_offset, flac: true } => assert_eq!(audio_offset, flac.len()),
        _other => panic!("unexpected probe"),
    }
    let mp3 = [0xff, 0xfb, 0x90, 0x00, 0, 0, 0, 0, 0, 0];
    match probe_audio_offset(&mp3) {
        Probe::Ready { audio_offset: 0, flac: false } => {}
        Probe::NeedMore | Probe::Ready { .. } => panic!("mp3 header should start at byte 0"),
    }
}

#[tokio::test]
#[ignore]
async fn live_session_reads_the_account_when_arl_is_set() {
    let Ok(arl) = std::env::var("KVASIR_ARL") else {
        return;
    };
    let session = kvasir_deezer::create_session(Some(&arl)).await.expect("session");
    let user = session.get_user().await.expect("user");
    assert!(user.get("USER_ID").is_some() || user.get("BLOG_NAME").is_some());
}
