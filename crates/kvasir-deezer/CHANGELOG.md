# Changelog

## 0.1.1 — 2026-10-02

- Stripe decryption keeps one Blowfish key for the whole file and decrypts each stripe in place.
- Download streams decrypt on the task that reads the socket, and the output buffer is reserved when Deezer reports a size.
- CDN reads use a five-minute timeout so a long file is not cut off by the API timeout.

## 0.1.0 — 2026-10-02

First release.

- `Session` for the Deezer gateway, public REST, and media API, with bounded retry and split caches.
- Download URL resolution, streaming stripe decryption, and buffered download.
- MP3 ID3v2.3 and FLAC Vorbis tagging, including FLAC cover art, via `TagStream`.
- URL resolution for Deezer, and optionally Spotify, Tidal, and YouTube.
- Search, charts, radios, library reads, podcasts, and previews.
- Favorite and playlist writes kept off the download path.
- Optional MusicBrainz, Cover Art Archive, and Musixmatch fallback.
- No bundled account cookie. Callers pass their own `arl`.
