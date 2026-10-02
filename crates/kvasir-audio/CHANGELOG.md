# Changelog

## 0.1.0 — 2026-10-02

First release.

- Magic-byte detection for MP3, FLAC, AAC ADTS, M4A/MP4, Ogg, WAV, and WebM.
- `analyze_audio` and `analyze_stream`, with size limits, hint checks, and optional artwork.
- Tag normalization: control characters stripped, lengths capped, artists de-duplicated.
- ReplayGain track gain surfaced when the container carries it.
