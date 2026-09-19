//! RFC 1321 §A.5 known-answer tests + million-'a' vector.
//!
//! RFC 1321 vectors and the million-a stress case.

use md5_simd::{Md5, Md5State, digest, hex_encode};

const VECTORS: &[(&[u8], &str)] = &[
    (b"", "d41d8cd98f00b204e9800998ecf8427e"),
    (b"a", "0cc175b9c0f1b6a831c399e269772661"),
    (b"abc", "900150983cd24fb0d6963f7d28e17f72"),
    (b"message digest", "f96b697d7cb7938d525a2f31aaf161d0"),
    (
        b"abcdefghijklmnopqrstuvwxyz",
        "c3fcd3d76192e4007dfb496cca67e13b",
    ),
    (
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789",
        "d174ab98d277d9f5a5611c2c9f419d9f",
    ),
    (
        b"12345678901234567890123456789012345678901234567890123456789012345678901234567890",
        "57edf4a22be3c955ac49da2e2107b67a",
    ),
];

fn assert_feed_variants(input: &[u8], want: &str, label: &str) {
    assert_eq!(&hex_encode(&digest(input)), want, "oneshot {label}");

    let mut h = Md5::new();
    h.update(input);
    assert_eq!(&hex_encode(&h.finalize()), want, "stream-whole {label}");

    for step in [1usize, 13, 64] {
        let mut h = Md5::new();
        for chunk in input.chunks(step) {
            h.update(chunk);
        }
        assert_eq!(
            &hex_encode(&h.finalize()),
            want,
            "stream step={step} {label}"
        );
    }

    let mut st = Md5State::new();
    if !input.is_empty() {
        let cut = input.len() / 2;
        st.update(&input[..cut]);
        st.update(&input[cut..]);
    }
    assert_eq!(&hex_encode(&st.finalize()), want, "state {label}");
}

#[test]
fn oneshot_matches_rfc() {
    for (input, want) in VECTORS {
        assert_feed_variants(input, want, &format!("len={}", input.len()));
    }
}

#[test]
fn streaming_whole_matches_rfc() {
    for (input, want) in VECTORS {
        let mut h = Md5::new();
        h.update(input);
        assert_eq!(&hex_encode(&h.finalize()), want);
    }
}

#[test]
fn streaming_odd_chunks_match_rfc() {
    for (input, want) in VECTORS {
        let mut h = Md5::new();
        for chunk in input.chunks(13) {
            h.update(chunk);
        }
        assert_eq!(&hex_encode(&h.finalize()), want);
    }
}

#[test]
fn million_a() {
    let mut h = Md5::new();
    let chunk = [b'a'; 1000];
    for _ in 0..1000 {
        h.update(&chunk);
    }
    assert_eq!(
        hex_encode(&h.finalize()),
        "7707d6ae4e027c70eea2a935c2296f21"
    );
}
