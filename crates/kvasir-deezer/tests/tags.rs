use bytes::Bytes;
use kvasir_audio::{analyze_audio_blocking, AnalyzeOptions, AudioFormatId, AudioHints};
use kvasir_deezer::{ExplicitLabel, TagStream, TrackTagModel};

fn model() -> TrackTagModel {
    TrackTagModel {
        title: "Song".into(),
        subtitle: None,
        album: "Album".into(),
        artists: vec!["Ada".into()],
        main_artists: vec!["Ada".into()],
        featured_artists: Vec::new(),
        album_artist: "Ada".into(),
        composers: Vec::new(),
        lyricists: Vec::new(),
        producers: Vec::new(),
        engineers: Vec::new(),
        mixers: Vec::new(),
        performers: Vec::new(),
        publishers: Vec::new(),
        track_number: 2,
        track_total: Some(8),
        disc_number: 1,
        disc_total: Some(1),
        isrc: Some("USAAA0000001".into()),
        barcode: None,
        duration_ms: 180_000,
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
        replay_gain_track_gain: Some("-3.20 dB".into()),
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
        cover: Some(Bytes::from_static(&[0xff, 0xd8, 0xff, 0xd9])),
        cover_size: 1000,
        artist_image: None,
    }
}

fn push_bits(bits: &mut Vec<bool>, value: u64, width: u8) {
    for shift in (0..width).rev() {
        bits.push(((value >> shift) & 1) == 1);
    }
}

fn streaminfo() -> Vec<u8> {
    let mut bits = Vec::new();
    push_bits(&mut bits, 16, 16);
    push_bits(&mut bits, 16, 16);
    push_bits(&mut bits, 0, 24);
    push_bits(&mut bits, 0, 24);
    push_bits(&mut bits, 44_100, 20);
    push_bits(&mut bits, 0, 3);
    push_bits(&mut bits, 15, 5);
    push_bits(&mut bits, 44_100, 36);
    for _ in 0..128 {
        bits.push(false);
    }
    let mut bytes = vec![0u8; 34];
    for (index, bit) in bits.iter().enumerate() {
        if *bit {
            bytes[index / 8] |= 1 << (7 - (index % 8));
        }
    }
    bytes
}

fn flac_fixture() -> Vec<u8> {
    let info = streaminfo();
    let mut bytes = b"fLaC".to_vec();
    bytes.push(0x80);
    bytes.extend_from_slice(&(info.len() as u32).to_be_bytes()[1..]);
    bytes.extend(info);
    bytes.extend_from_slice(&[0xf8, 0xff, 0xf8]);
    bytes
}

fn mp3_fixture() -> Vec<u8> {
    let mut frame = vec![0u8; 417];
    frame[0] = 0xff;
    frame[1] = 0xfb;
    frame[2] = 0x90;
    let mut bytes = frame.clone();
    bytes.extend_from_slice(&frame);
    bytes
}

fn tag_in_chunks(audio: &[u8], tagged: &TrackTagModel) -> Vec<u8> {
    let mut stream = TagStream::new(tagged.clone(), true);
    let mut output = Vec::new();
    for chunk in audio.chunks(3) {
        output.extend(stream.push(chunk).unwrap());
    }
    output.extend(stream.finish().unwrap());
    output
}

fn read_back(bytes: &[u8]) -> kvasir_audio::AudioAnalysis {
    analyze_audio_blocking(
        bytes,
        &AudioHints::default(),
        &AnalyzeOptions {
            include_artwork: true,
            ..AnalyzeOptions::default()
        },
    )
    .expect("analysis")
}

#[test]
fn mp3_tags_round_trip_through_chunked_stream() {
    let tagged = tag_in_chunks(&mp3_fixture(), &model());
    let analysis = read_back(&tagged);
    assert_eq!(analysis.format.id, AudioFormatId::Mp3);
    assert_eq!(analysis.tags.title.as_deref(), Some("Song"));
    assert_eq!(analysis.tags.album.as_deref(), Some("Album"));
    assert_eq!(analysis.tags.isrc.as_deref(), Some("USAAA0000001"));
    assert_eq!(analysis.tags.track.as_ref().and_then(|pair| pair.number), Some(2));
    assert_eq!(analysis.tags.replay_gain.as_deref(), Some("-3.20 dB"));
}

#[test]
fn flac_tags_and_cover_round_trip() {
    let tagged = tag_in_chunks(&flac_fixture(), &model());
    let analysis = read_back(&tagged);
    assert_eq!(analysis.format.id, AudioFormatId::Flac);
    assert_eq!(analysis.tags.title.as_deref(), Some("Song"));
    assert_eq!(analysis.tags.album.as_deref(), Some("Album"));
    assert_eq!(analysis.tags.isrc.as_deref(), Some("USAAA0000001"));
    assert_eq!(analysis.tags.track.as_ref().and_then(|pair| pair.number), Some(2));
    assert_eq!(analysis.tags.replay_gain.as_deref(), Some("-3.20 dB"));
    assert_eq!(analysis.artwork.len(), 1);
    assert_eq!(analysis.artwork[0].data, [0xff, 0xd8, 0xff, 0xd9]);
}
