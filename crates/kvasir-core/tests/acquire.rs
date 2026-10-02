use kvasir_audio::wave_fixture;
use kvasir_core::{acquire_from_bytes, ExplicitLabel, Person, ReconcileIssue, TrackTagModel};

fn model(title: &str, isrc: &str, duration_ms: u64) -> TrackTagModel {
    TrackTagModel {
        title: title.into(),
        subtitle: None,
        album: "Album".into(),
        artists: vec!["Ada".into()],
        main_artists: vec!["Ada".into()],
        featured_artists: Vec::new(),
        album_artist: "Ada".into(),
        composers: Vec::new(),
        lyricists: Vec::new(),
        producers: Vec::new(),
        engineers: Vec::<Person>::new(),
        mixers: Vec::new(),
        performers: Vec::new(),
        publishers: Vec::new(),
        track_number: 1,
        track_total: None,
        disc_number: 1,
        disc_total: None,
        isrc: Some(isrc.into()),
        barcode: None,
        duration_ms,
        bpm: None,
        genres: Vec::new(),
        label: None,
        release_type: None,
        is_compilation: false,
        date: None,
        year: None,
        original_date: None,
        original_year: None,
        copyright: None,
        producer_line: None,
        replay_gain_track_gain: None,
        explicit: ExplicitLabel::Unknown,
        itunes_advisory: 0,
        lyrics: None,
        lyrics_synced: None,
        lyrics_writers: None,
        lyrics_copyright: None,
        deezer_track_id: None,
        deezer_album_id: None,
        deezer_artist_id: None,
        slug: None,
        label_id: None,
        provider_id: None,
        rank: None,
        cover: None,
        cover_size: 1000,
        artist_image: None,
    }
}

fn mp3_frame() -> Vec<u8> {
    let mut frame = vec![0u8; 417];
    frame[0] = 0xff;
    frame[1] = 0xfb;
    frame[2] = 0x90;
    let mut bytes = frame.clone();
    bytes.extend_from_slice(&frame);
    bytes
}

#[tokio::test]
async fn matching_mp3_is_tagged() {
    let acquired = acquire_from_bytes(mp3_frame().into(), "MP3_320", model("Song", "USAAA0000001", 52)).await.unwrap();
    assert!(acquired.tagged);
    assert!(acquired.bytes.starts_with(b"ID3"));
    assert!(acquired.issues.is_empty() || acquired.issues.iter().all(|issue| !matches!(issue, ReconcileIssue::ContainerMismatch { .. })));
}

#[tokio::test]
async fn container_mismatch_skips_tagging() {
    let acquired = acquire_from_bytes(wave_fixture().into(), "MP3_320", model("Song", "USAAA0000001", 1_000)).await.unwrap();
    assert!(!acquired.tagged);
    assert!(acquired.bytes.starts_with(b"RIFF"));
    assert!(acquired.issues.iter().any(|issue| matches!(issue, ReconcileIssue::ContainerMismatch { .. })));
}

#[tokio::test]
async fn duration_and_isrc_disagreements_still_tag() {
    let first = acquire_from_bytes(mp3_frame().into(), "MP3_320", model("Other", "USAAA0000001", 52)).await.unwrap();
    let acquired = acquire_from_bytes(first.bytes, "MP3_320", model("Song", "USBBB0000002", 180_000)).await.unwrap();
    assert!(acquired.tagged);
    assert!(acquired.issues.iter().any(|issue| matches!(issue, ReconcileIssue::IsrcMismatch { .. })));
    assert!(acquired.issues.iter().any(|issue| matches!(issue, ReconcileIssue::TitleMismatch { .. })));
    assert!(acquired.issues.iter().any(|issue| matches!(issue, ReconcileIssue::DurationMismatch { .. })));
    assert!(acquired.issues.iter().all(|issue| !matches!(issue, ReconcileIssue::ContainerMismatch { .. })));
}
