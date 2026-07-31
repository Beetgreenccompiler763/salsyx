# AAHL — ArchiveHub Archive Layer

A lossless, chunked, content-addressed archive format for long-term software
preservation.

AAHL is a reusable Rust crate that converts a directory tree (a repository
snapshot, a set of files, anything) into a small manifest plus a set of
deduplicated, checksummed chunks stored in any content-addressed object store.

## Design goals

- **Lossless** — bytes, permissions, symlinks, and directory structure are
  preserved exactly.
- **Deduplicated** — content-defined chunking (buzhash rolling hash) means
  identical data is stored once and referenced many times, across files,
  across snapshots, and across repositories sharing a chunk store.
- **Efficient** — chunk compression via Zstandard (optional feature), with
  the digest computed over the *uncompressed* chunk so dedup is
  compression-independent.
- **Streaming** — the decoder reads chunk-by-chunk from the store; a file can
  be reassembled without materializing the whole archive.
- **Verifiable** — every chunk is SHA-256 content-addressed, the manifest is
  hashed, and (with the `sign` feature) the manifest can be signed with
  Ed25519.
- **Versioned** — `format_version` in the manifest, designed so a future
  format bump can still read old manifests.
- **Incremental** — a new snapshot can reference a parent manifest; unchanged
  chunks are referenced, not re-stored.

## Format summary

```
Manifest (JSON, versioned, sha256-hashed, optionally signed)
 └─ blobs[]:  sha256 of each unique chunk
 └─ files[]:  path · kind · mode · size · blob indices · symlink target
Chunk store (content-addressed): key = sha256(chunk) → bytes
```

Chunking parameters are **not** part of the format contract — the decoder is
chunk-agnostic and reconstructs purely from blob references.

## Example

```rust
use aahl::{encode, decode, store::MemoryStore, Manifest};

let store = MemoryStore::default();
let manifest = encode::encode_dir("/path/to/repo", &store).expect("encode");
let bytes = serde_json::to_vec(&manifest).expect("json");
let hash = aahl::sha256_hex(&bytes);

let decoded = decode::decode_dir(manifest.clone(), &store).expect("decode");
assert_eq!(decoded, manifest); // structure round-trips
```

## Features

- `zstd` — per-chunk Zstandard compression (default **on**; set
  `aahl = { version = "0.1", default-features = false, features = ["zstd"] }`
  to opt out).
- `sign` — Ed25519 manifest signing/verification (default off).
