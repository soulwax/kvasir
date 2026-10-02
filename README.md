# kvasir-core

Self-contained Rust library that joins byte-authoritative audio inspection with Deezer catalogue access, stripe decryption, and MP3/FLAC tagging.

`kvasir-core` is the only crate a caller needs. It re-exports `kvasir-audio` and `kvasir-deezer` and adds `acquire`, which reconciles what the bytes are with what the catalogue says.

For personal and archival use with content you are entitled to access. You are responsible for the terms of each provider and for copyright law where you are.

## Layout

- `kvasir-audio` detects MP3, FLAC, AAC, M4A, Ogg, WAV, and WebM and normalizes tags. Filename and MIME type are untrusted hints.
- `kvasir-deezer` owns the session, gateway, public REST, URL resolution, download, decrypt, and tag writer.
- `kvasir-core` runs acquire and reports container, duration, ISRC, and title disagreements.

Playback stays outside this workspace.

## Example

```rust
let session = kvasir_core::create_session(Some(arl)).await?;
let acquired = kvasir_core::acquire(&session, "3135556", kvasir_core::Quality::Mp3_320).await?;
```

`acquired.analysis` is what the decrypted bytes contain. `acquired.catalogue` is the tag model. Tagging is skipped when the sniffed container disagrees with the resolved FLAC or MP3 format.

Optional features `resolve-spotify`, `resolve-tidal`, `resolve-youtube`, `enrich`, and `lyrics-fallback` are on by default. A Deezer-only build is `cargo build -p kvasir-core --no-default-features`. MusicBrainz requires `configure_musicbrainz` with a descriptive user agent and is never called by `acquire`. Musixmatch runs only when `TagOptions.lyrics_fallback` is set.

No CLI and no disk I/O. Callers write files.
