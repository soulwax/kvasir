# kvasir

Rust libraries for music files and the Deezer catalogue. Each crate builds and publishes on its own.

| Crate | Role |
| --- | --- |
| `kvasir-audio` | Detect a container and normalize tags. No network. |
| `kvasir-deezer` | Session, search, download, decrypt, and tag. No file inspection. |
| `kvasir-core` | `acquire`: decrypt, inspect, reconcile. Re-exports the other two. |

The command-line program lives in [kvasir-cli](https://github.com/soulwax/kvasir-cli) and depends on the published `kvasir-core` crate.

```toml
kvasir-core = "0.1"
```

Pass your own Deezer `arl`. No account cookie is included.

GNU GPL v3.0 only. See [LICENSE](LICENSE). Version history is in each crate's `CHANGELOG.md`.
