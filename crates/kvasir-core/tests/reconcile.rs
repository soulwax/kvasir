use kvasir_core::{
    reconcile, AudioAnalysis, AudioFormatId, AudioTags, AudioTechnicalMetadata, ExplicitLabel, Person, ReconcileIssue,
    TrackTagModel,
};

fn analysis(id: AudioFormatId, title: Option<&str>, isrc: Option<&str>, duration: Option<f64>) -> AudioAnalysis {
    AudioAnalysis {
        format: AudioTechnicalMetadata {
            id,
            content_type: "audio/mpeg".into(),
            container: None,
            codec: Some("MP3".into()),
            duration_seconds: duration,
            bitrate: None,
            sample_rate: Some(44100.0),
            channels: Some(2),
            bits_per_sample: None,
            lossless: Some(false),
        },
        tags: AudioTags {
            title: title.map(str::to_string),
            artists: Vec::new(),
            album: None,
            album_artists: Vec::new(),
            track: None,
            disc: None,
            date: None,
            year: None,
            genres: Vec::new(),
            composers: Vec::new(),
            isrc: isrc.map(str::to_string),
            copyright: None,
            replay_gain: None,
        },
        artwork: Vec::new(),
        warnings: Vec::new(),
    }
}

fn catalogue(title: &str, isrc: Option<&str>, duration_ms: u64) -> TrackTagModel {
    TrackTagModel {
        title: title.into(),
        subtitle: None,
        album: "Album".into(),
        artists: vec!["Artist".into()],
        main_artists: vec!["Artist".into()],
        featured_artists: Vec::new(),
        album_artist: "Artist".into(),
        composers: Vec::new(),
        lyricists: Vec::new(),
        producers: Vec::new(),
        engineers: Vec::<Person>::new(),
        mixers: Vec::new(),
        performers: Vec::new(),
        publishers: Vec::new(),
        track_number: 1,
        track_total: Some(1),
        disc_number: 1,
        disc_total: Some(1),
        isrc: isrc.map(str::to_string),
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

#[test]
fn container_mismatch_stops_agreement() {
    let issues = reconcile(&analysis(AudioFormatId::Aac, Some("Song"), None, Some(180.0)), &catalogue("Song", None, 180_000), "MP3_320");
    assert!(matches!(issues[0], ReconcileIssue::ContainerMismatch { .. }));
}

#[test]
fn duration_isrc_and_title_disagreements_are_reported() {
    let issues = reconcile(
        &analysis(AudioFormatId::Mp3, Some("Other"), Some("USAAA0000001"), Some(10.0)),
        &catalogue("Song", Some("us-aaa-0000002"), 180_000),
        "MP3_320",
    );
    assert!(issues.iter().any(|issue| matches!(issue, ReconcileIssue::DurationMismatch { .. })));
    assert!(issues.iter().any(|issue| matches!(issue, ReconcileIssue::IsrcMismatch { .. })));
    assert!(issues.iter().any(|issue| matches!(issue, ReconcileIssue::TitleMismatch { .. })));
}

#[test]
fn matching_bytes_produce_no_issues() {
    let issues = reconcile(
        &analysis(AudioFormatId::Flac, Some("Song"), Some("USAAA0000001"), Some(180.2)),
        &catalogue("song", Some("US-AAA-0000001"), 180_000),
        "FLAC",
    );
    assert!(issues.is_empty());
}
