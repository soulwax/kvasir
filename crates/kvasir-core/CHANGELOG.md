# Changelog

## 0.1.0 — 2026-10-02

First release.

- `acquire` downloads, decrypts, analyzes, and tags one Deezer track.
- `acquire_from_bytes` runs the same reconcile step on audio you already have.
- Disagreements: container mismatch stops tagging; duration, ISRC, and title mismatches are reported and tagging continues.
- Re-exports the public API of `kvasir-audio` 0.1 and `kvasir-deezer` 0.1.
