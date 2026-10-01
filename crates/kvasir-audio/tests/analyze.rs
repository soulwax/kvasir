use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use kvasir_audio::{
    analyze_audio, analyze_audio_blocking, analyze_stream, detect_audio_format, normalize_metadata, text,
    wave_fixture, AnalyzeOptions, AudioFormatId, AudioHints, AudioMetadataErrorCode, AudioWarningCode,
    NormalizeOptions, ParserMetadata, ParserPicture, StreamAudioHints, DEFAULT_MAX_FILE_BYTES,
};

#[test]
fn detects_containers_from_magic_bytes() {
    assert_eq!(detect_audio_format(b"fLaC....").unwrap().id, AudioFormatId::Flac);
    assert_eq!(detect_audio_format(b"OggS....").unwrap().id, AudioFormatId::Ogg);
    assert_eq!(detect_audio_format(b"ID3.....").unwrap().id, AudioFormatId::Mp3);
    assert_eq!(detect_audio_format(&[0xff, 0xf1]).unwrap().id, AudioFormatId::Aac);
    assert_eq!(detect_audio_format(&[0xff, 0xfb]).unwrap().id, AudioFormatId::Mp3);
    let mut webm = vec![0x1a, 0x45, 0xdf, 0xa3];
    webm.extend(b"....webm");
    assert_eq!(detect_audio_format(&webm).unwrap().id, AudioFormatId::Webm);
    let mut m4a = vec![0, 0, 0, 0];
    m4a.extend(b"ftyp");
    assert_eq!(detect_audio_format(&m4a).unwrap().id, AudioFormatId::M4a);
    assert!(detect_audio_format(b"not audio").is_none());
}

#[test]
fn analyzes_a_synthetic_wave() {
    let bytes = wave_fixture();
    let analysis = analyze_audio_blocking(&bytes, &AudioHints::default(), &AnalyzeOptions::default()).unwrap();
    assert_eq!(analysis.format.id, AudioFormatId::Wav);
    assert_eq!(analysis.format.content_type, "audio/wav");
    assert_eq!(analysis.format.sample_rate, Some(8000.0));
    assert_eq!(analysis.format.channels, Some(1));
    assert!(analysis.tags.artists.is_empty());
    assert!(analysis.artwork.is_empty());
}

#[test]
fn rejects_empty_and_oversized_input() {
    let empty = analyze_audio_blocking(&[], &AudioHints::default(), &AnalyzeOptions::default()).unwrap_err();
    assert_eq!(empty.code(), AudioMetadataErrorCode::EmptyInput);

    let options = AnalyzeOptions {
        max_file_bytes: Some(4),
        ..AnalyzeOptions::default()
    };
    let too_large = analyze_audio_blocking(&wave_fixture(), &AudioHints::default(), &options).unwrap_err();
    assert_eq!(too_large.code(), AudioMetadataErrorCode::FileTooLarge);
    assert_eq!(DEFAULT_MAX_FILE_BYTES, 128 * 1024 * 1024);
}

#[test]
fn strict_hints_and_declared_size() {
    let bytes = wave_fixture();
    let hints = AudioHints {
        file_name: Some("track.flac".into()),
        mime_type: Some("audio/flac".into()),
        size: None,
    };
    let warning = analyze_audio_blocking(&bytes, &hints, &AnalyzeOptions::default()).unwrap();
    assert!(warning
        .warnings
        .iter()
        .any(|item| item.code == AudioWarningCode::ExtensionMismatch));
    assert!(warning
        .warnings
        .iter()
        .any(|item| item.code == AudioWarningCode::MimeMismatch));

    let strict = AnalyzeOptions {
        strict_hints: true,
        ..AnalyzeOptions::default()
    };
    let mismatch = analyze_audio_blocking(&bytes, &hints, &strict).unwrap_err();
    assert_eq!(mismatch.code(), AudioMetadataErrorCode::HintMismatch);

    let size = AudioHints {
        size: Some(bytes.len() as u64 + 1),
        ..AudioHints::default()
    };
    let declared = analyze_audio_blocking(&bytes, &size, &AnalyzeOptions::default()).unwrap_err();
    assert_eq!(declared.code(), AudioMetadataErrorCode::HintMismatch);
}

#[test]
fn artwork_limits_are_warnings() {
    let picture = ParserPicture {
        format: "image/png".into(),
        data: vec![1, 2, 3, 4],
        description: Some("cover".into()),
    };
    let omitted = normalize_metadata(
        kvasir_audio::AUDIO_FORMATS[5],
        &ParserMetadata {
            pictures: vec![picture.clone()],
            sample_rate: Some(8000.0),
            ..ParserMetadata::default()
        },
        NormalizeOptions {
            include_artwork: false,
            max_artwork_bytes: 8,
            max_artwork_count: 4,
        },
        Vec::new(),
    );
    assert!(omitted.artwork.is_empty());
    assert_eq!(omitted.warnings[0].code, AudioWarningCode::ArtworkOmitted);

    let limited = normalize_metadata(
        kvasir_audio::AUDIO_FORMATS[5],
        &ParserMetadata {
            pictures: vec![picture],
            ..ParserMetadata::default()
        },
        NormalizeOptions {
            include_artwork: true,
            max_artwork_bytes: 2,
            max_artwork_count: 4,
        },
        Vec::new(),
    );
    assert!(limited.artwork.is_empty());
    assert_eq!(limited.warnings[0].code, AudioWarningCode::ArtworkLimit);
}

#[test]
fn text_strips_controls_and_caps_length() {
    assert_eq!(text("a\u{0001}  b", 512).as_deref(), Some("a b"));
    assert_eq!(text("abcdef", 3).as_deref(), Some("abc"));
    assert_eq!(text("   ", 8), None);
}

#[tokio::test]
async fn stream_uses_the_declared_size_as_the_admission_limit() {
    let bytes = wave_fixture();
    let stream = futures_util::stream::iter(bytes.chunks(3).map(|chunk| Ok::<_, std::io::Error>(bytes::Bytes::copy_from_slice(chunk))));
    let analysis = analyze_stream(
        stream,
        StreamAudioHints {
            file_name: None,
            mime_type: None,
            size: bytes.len() as u64,
        },
        AnalyzeOptions::default(),
    )
    .await
    .unwrap();
    assert_eq!(analysis.format.id, AudioFormatId::Wav);

    let aborted = Arc::new(AtomicBool::new(true));
    let err = analyze_audio(
        bytes::Bytes::from(bytes),
        AudioHints::default(),
        AnalyzeOptions {
            aborted: Some(aborted),
            ..AnalyzeOptions::default()
        },
    )
    .await
    .unwrap_err();
    assert_eq!(err.code(), AudioMetadataErrorCode::Aborted);
}

#[test]
fn abort_flag_is_observable() {
    let flag = Arc::new(AtomicBool::new(false));
    flag.store(true, Ordering::Relaxed);
    assert!(flag.load(Ordering::Relaxed));
}
