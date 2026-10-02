# kvasir-core

One crate for the whole job: resolve a Deezer track, download and decrypt it, read what the bytes actually are, and reconcile that with the catalogue tags.

Depending on `kvasir-core` is enough. It re-exports `kvasir-audio` and `kvasir-deezer`, so you do not add those crates yourself. They are versioned dependencies and work without this repository checked out.

## Install

```toml
kvasir-core = "0.1"
```

Rust 1.85 or newer. Default features follow `kvasir-deezer`: Spotify, Tidal, YouTube, MusicBrainz, and Musixmatch. Turn them off with `default-features = false`.

## Acquire

```rust
let session = kvasir_core::create_session(Some(arl)).await?;
let acquired = kvasir_core::acquire(&session, "3135556", kvasir_core::Quality::Mp3_320).await?;
```

`acquired.analysis` is the container and the tags that were already in the file. `acquired.catalogue` is the `TrackTagModel` Deezer supplied. `acquired.issues` lists container, duration, ISRC, and title disagreements. Tagging is skipped when the sniffed container is not the FLAC or MP3 that was requested. `acquired.bytes` is the tagged file when tagging ran.

`acquire_from_bytes` runs the same join when you already hold the decrypted audio and the tag model.

## License

GNU GPL v3.0 only. See [LICENSE](../../LICENSE). For personal use of music your account is allowed to stream.
