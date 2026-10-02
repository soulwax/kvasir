# kvasir-deezer

Deezer catalogue access, download, Blowfish stripe decryption, and MP3/FLAC tagging. The library returns bytes and values. It does not write files and it does not inspect containers; that is `kvasir-audio`. `kvasir-core` calls both.

You pass your own 192-character `arl` cookie to `create_session`. Nothing in this crate is a working account.

## Install

```toml
kvasir-deezer = "0.1"
```

Rust 1.85 or newer. Default features enable Spotify, Tidal, and YouTube resolution, plus MusicBrainz and the Musixmatch lyrics fallback. A Deezer-only build:

```toml
kvasir-deezer = { version = "0.1", default-features = false }
```

## Session

```rust
let session = kvasir_deezer::create_session(Some(arl)).await?;
let track = session.get_track_info("3135556").await?;
let urls = kvasir_deezer::resolve_download_urls(&session, &[track], &[kvasir_deezer::Quality::Mp3_320]).await?;
```

`Session` is the account. Gateway calls retry inside `RETRY_POLICY` (per-error caps and a 30 second deadline). Account-scoped responses stay in that session's cache. Album, artist, lyrics, discography, and playlist metadata are shared across sessions for the same country.

## Download and tags

`download_track_bytes` resolves a media URL, downloads, and stripe-decrypts. `resolve_tag_model` builds a `TrackTagModel` from Deezer credits, lyrics, ReplayGain, and cover art. `TagStream` writes ID3v2.3 or FLAC Vorbis comments, including a front-cover picture block, without holding a second copy of the audio.

Favorite and playlist edits live in their own functions and are not called by the download path. `create_playlist` has no delete counterpart.

## License

GNU GPL v3.0 only. See [LICENSE](../../LICENSE). For personal use of music your account is allowed to stream.
