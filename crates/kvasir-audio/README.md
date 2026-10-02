# kvasir-audio

Byte-authoritative audio inspection. Give it a buffer or a stream and it tells you the container, the codec details it can see without decoding, and a normalized set of music tags.

This crate has no network access and no Deezer types. Use it on a file you already have, a preview clip, or the bytes `kvasir-deezer` decrypted. `kvasir-core` is the crate that joins the two.

## Install

```toml
kvasir-audio = "0.1"
```

Rust 1.85 or newer.

## Detect

Magic bytes win. Filename and MIME type are hints: a mismatch is a warning, or `AudioMetadataError::HintMismatch` when `strict_hints` is set.

Supported containers: MP3, FLAC, AAC ADTS, M4A/MP4, Ogg, WAV, and WebM. Identification does not mean a particular device can decode the codec inside.

## Analyze

```rust
let analysis = kvasir_audio::analyze_audio_blocking(&bytes, &Default::default(), &Default::default())?;
println!("{} {}", analysis.format.id.as_str(), analysis.tags.title.unwrap_or_default());
```

Defaults: 128 MiB maximum, artwork omitted, 4 KiB used for detection. A stream must come with a trusted size. That size is the admission limit, not a guess.

## License

GNU GPL v3.0 only. See [LICENSE](../../LICENSE).
