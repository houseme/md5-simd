//! Snapshot finalization, independent clones, and hasher reuse.

use md5_simd::{Md5, digest, hex_encode};

#[test]
fn streaming_snapshot_clone_pattern() {
    let payload = b"0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ";

    let mut h = Md5::new();
    h.update(&payload[..32]);
    let mid = h.clone().finalize();
    assert_eq!(hex_encode(&mid), hex_encode(&digest(&payload[..32])));

    h.update(&payload[32..]);
    let final_digest = h.clone().finalize();
    assert_eq!(hex_encode(&final_digest), hex_encode(&digest(payload)));

    let again = h.clone().finalize();
    assert_eq!(again, final_digest);
}

#[test]
fn finalize_reset_reuse() {
    let mut h = Md5::new();
    h.update(b"abc");
    let d1 = h.finalize_reset();
    assert_eq!(hex_encode(&d1), "900150983cd24fb0d6963f7d28e17f72");
    h.update(b"");
    let d2 = h.finalize_reset();
    assert_eq!(hex_encode(&d2), "d41d8cd98f00b204e9800998ecf8427e");
}

/// Mid-stream clone must not mutate the original hasher's future results.
#[test]
fn clone_does_not_alias_original() {
    let mut h = Md5::new();
    h.update(b"hello ");
    let snapshot = h.clone();
    h.update(b"world");

    assert_eq!(
        hex_encode(&snapshot.finalize()),
        hex_encode(&digest(b"hello "))
    );
    assert_eq!(
        hex_encode(&h.clone().finalize()),
        hex_encode(&digest(b"hello world"))
    );
}

/// Repeated snapshots at the end of a stream are stable.
#[test]
fn repeated_finalize_after_full_update_stable() {
    let payload = b"streaming-etag-payload";
    let mut h = Md5::new();
    h.update(payload);
    let d1 = h.clone().finalize();
    let d2 = h.clone().finalize();
    let d3 = h.finalize();
    assert_eq!(d1, d2);
    assert_eq!(d2, d3);
    assert_eq!(hex_encode(&d1), hex_encode(&digest(payload)));
}
