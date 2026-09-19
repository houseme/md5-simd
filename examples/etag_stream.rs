//! Read a chunked message while inspecting non-consuming MD5 hex snapshots.
//!
//! These hex strings are not Base64 Content-MD5 headers or multipart ETags.
//! Run with `cargo run --example etag_stream`.

use md5_simd::{Md5, digest, hex_encode};

fn main() {
    let chunks: [&[u8]; 4] = [
        b"object part one -- ",
        b"part two with padding ",
        &[0xAB; 80],
        b"tail",
    ];
    let mut hasher = Md5::new();
    let mut received = Vec::new();

    for (index, chunk) in chunks.iter().enumerate() {
        hasher.update(chunk);
        received.extend_from_slice(chunk);

        // A snapshot must leave the stream available for the next chunk.
        let snapshot = hasher.finalize_snapshot();
        assert_eq!(snapshot, hasher.clone().finalize());
        assert_eq!(snapshot, digest(&received));
        println!("after chunk {index}: md5={}", hex_encode(&snapshot));
    }

    let checksum = hasher.finalize();
    assert_eq!(checksum, digest(&received));
    println!("complete: md5={}", hex_encode(&checksum));
}
