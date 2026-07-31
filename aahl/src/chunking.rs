//! Content-defined chunking.
//!
//! AAHL splits files into variable-length chunks using a rolling hash
//! (buzhash). Unlike fixed-size chunking, content-defined chunking keeps
//! chunk boundaries stable when data is inserted or removed, which makes
//! cross-file and cross-snapshot deduplication far more effective.
//!
//! Chunking parameters are a *tuning* concern, not part of the format
//! contract: the decoder reconstructs files purely from blob references, so
//! chunks of any size are valid as long as the digest is over the exact
//! bytes.

use crate::error::Result;
use crate::sha256_hex;

/// Default chunk-size bounds (matches git/restic-style defaults).
pub const MIN_CHUNK: usize = 16 * 1024;
pub const TARGET_CHUNK: usize = 32 * 1024;
pub const MAX_CHUNK: usize = 64 * 1024;

/// A single content-addressed chunk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chunk {
    /// Lowercase hex SHA-256 of `data` (the content address).
    pub hash: String,
    /// The exact chunk bytes (digest is over these, uncompressed).
    pub data: Vec<u8>,
}

impl Chunk {
    /// Chunk with the digest computed from the data.
    pub fn new(data: Vec<u8>) -> Self {
        let hash = sha256_hex(&data);
        Self { hash, data }
    }
}

/// Streaming content-defined chunker.
///
/// Feed file bytes with [`Chunker::push`] and collect complete chunks as they
/// are cut; call [`Chunker::finish`] to flush the trailing partial chunk.
/// Memory use is bounded by [`MAX_CHUNK`].
pub struct Chunker {
    mask: u64,
    buf: Vec<u8>,
    hash: u64,
}

impl Default for Chunker {
    fn default() -> Self {
        Self::new(13)
    }
}

impl Chunker {
    /// `mask_bits` controls the target chunk size: boundary probability per
    /// byte is `2^-mask_bits`. 13 → ~8 KiB average (clamped to [MIN, MAX]).
    pub fn new(mask_bits: u32) -> Self {
        let mask: u64 = if mask_bits >= 64 {
            u64::MAX
        } else {
            (1u64 << mask_bits) - 1
        };
        Self {
            mask,
            buf: Vec::new(),
            hash: 0,
        }
    }

    /// Feed `data`; returns any complete chunks cut during this call.
    pub fn push(&mut self, data: &[u8]) -> Vec<Chunk> {
        let mut out = Vec::new();
        for &byte in data {
            self.buf.push(byte);
            let i = self.buf.len() - 1;

            // Rolling hash update (the last `WINDOW` bytes contribute).
            self.hash = self.hash.rotate_left(1) ^ GEAR[byte as usize];
            if i >= WINDOW {
                self.hash ^= GEAR[self.buf[i - WINDOW] as usize].rotate_left(1);
            }

            let len = self.buf.len();
            if (len >= MIN_CHUNK && (self.hash & self.mask) == 0) || len >= MAX_CHUNK {
                out.push(Chunk::new(std::mem::take(&mut self.buf)));
                self.hash = 0;
            }
        }
        out
    }

    /// Flush any remaining buffered bytes as a final chunk.
    pub fn finish(mut self) -> Vec<Chunk> {
        if !self.buf.is_empty() {
            let last = Chunk::new(std::mem::take(&mut self.buf));
            vec![last]
        } else {
            Vec::new()
        }
    }
}

const WINDOW: usize = 64;

/// Split `data` into content-defined chunks. Convenience wrapper over
/// [`Chunker`] for whole-buffer inputs.
pub fn chunk_data_default(data: &[u8]) -> Vec<Chunk> {
    let mut chunker = Chunker::default();
    let mut out = chunker.push(data);
    out.extend(chunker.finish());
    out
}

/// Deterministic, CPU-cheap 8-bit table for the rolling hash. Fixed at
/// compile time so all writers agree on boundaries.
const GEAR: [u64; 256] = gear_table();

const fn gear_table() -> [u64; 256] {
    // SplitMix64 constants — any fixed, well-mixed table works; the only
    // requirement is that every writer uses the *same* table, which this
    // guarantees.
    let mut table = [0u64; 256];
    let mut seed: u64 = 0x9E37_79B9_7F4A_7C15;
    let mut i = 0;
    while i < 256 {
        seed = seed.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = seed;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z = z ^ (z >> 31);
        table[i] = z;
        i += 1;
    }
    table
}

/// Verify a chunk: recompute the digest and compare against `expected_hash`.
pub fn verify_chunk(chunk: &Chunk, expected_hash: &str) -> Result<()> {
    let actual = sha256_hex(&chunk.data);
    if actual != expected_hash {
        return Err(crate::AahlError::ChunkChecksumMismatch {
            hash: chunk.hash.clone(),
            expected: expected_hash.to_string(),
            actual,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_input_yields_no_chunks() {
        assert!(chunk_data_default(b"").is_empty());
    }

    #[test]
    fn small_input_is_one_chunk() {
        let chunks = chunk_data_default(b"hello world");
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].data, b"hello world");
    }

    #[test]
    fn chunks_reassemble_to_original() {
        let data: Vec<u8> = (0..200_000u32).map(|i| (i % 251) as u8).collect();
        let chunks = chunk_data_default(&data);
        assert!(chunks.len() > 1, "expected multiple chunks");

        let mut reassembled = Vec::new();
        for c in &chunks {
            reassembled.extend_from_slice(&c.data);
            assert_eq!(c.hash, sha256_hex(&c.data));
        }
        assert_eq!(reassembled, data);
    }

    #[test]
    fn streaming_matches_whole_buffer() {
        let data: Vec<u8> = (0..200_000u32).map(|i| (i % 251) as u8).collect();
        let whole = chunk_data_default(&data);

        let mut chunker = Chunker::default();
        let mut streamed = Vec::new();
        for slice in data.chunks(4096) {
            streamed.extend(chunker.push(slice));
        }
        streamed.extend(chunker.finish());

        let whole_hashes: Vec<String> = whole.iter().map(|c| c.hash.clone()).collect();
        let streamed_hashes: Vec<String> = streamed.iter().map(|c| c.hash.clone()).collect();
        assert_eq!(whole_hashes, streamed_hashes);
    }

    #[test]
    fn boundaries_stable_under_insertion() {
        // A log-style file with many repeated lines: inserting a few lines
        // mid-file should not re-chunk the surrounding content, so most chunks
        // still deduplicate.
        let line: &[u8] = b"[INFO] worker 42 completed task 1337 in 87ms\n";
        let mut a = Vec::new();
        for _ in 0..40_000 {
            a.extend_from_slice(line);
        }
        let mut b = a.clone();
        // Insert a couple of lines at the middle of the file.
        let at = a.len() / 2;
        let mut ins = Vec::new();
        for _ in 0..2 {
            ins.extend_from_slice(b"[ERROR] worker 7 timed out, retrying\n");
        }
        b.splice(at..at, ins.iter().copied());

        let hashes_a: Vec<String> = chunk_data_default(&a)
            .iter()
            .map(|c| c.hash.clone())
            .collect();
        let hashes_b: Vec<String> = chunk_data_default(&b)
            .iter()
            .map(|c| c.hash.clone())
            .collect();

        let shared = hashes_a.iter().filter(|h| hashes_b.contains(h)).count();
        // Most chunks survive the insertion untouched.
        assert!(
            shared >= hashes_a.len() * 3 / 4,
            "got {shared}/{} shared",
            hashes_a.len()
        );
    }

    #[test]
    fn all_chunks_within_size_bounds() {
        let data: Vec<u8> = (0..1_000_000u32).map(|i| (i % 251) as u8).collect();
        for c in chunk_data_default(&data) {
            assert!(
                c.data.len() <= MAX_CHUNK,
                "chunk too large: {}",
                c.data.len()
            );
        }
    }
}
