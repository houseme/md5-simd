//! Padding / block-boundary tests vs RustCrypto `md-5`.

use md5_simd::{Md5, Md5State, digest, hex_encode};

fn md5_crate_hex(data: &[u8]) -> String {
    use md5::Digest;
    let out = md5::Md5::digest(data);
    hex_encode(out.as_slice())
}

fn pattern(len: usize) -> Vec<u8> {
    (0..len)
        .map(|i| (i as u8).wrapping_mul(17).wrapping_add(3))
        .collect()
}

#[test]
fn boundaries_match_reference() {
    for len in [
        0usize, 1, 55, 56, 57, 63, 64, 65, 119, 120, 127, 128, 129, 255, 256, 257, 4096, 4097,
    ] {
        let data = pattern(len);
        let expect = hex_encode(&digest(&data));
        assert_eq!(expect, md5_crate_hex(&data), "oneshot len={len}");

        for step in [1usize, 7, 64, 65] {
            let mut h = Md5::new();
            for chunk in data.chunks(step) {
                h.update(chunk);
            }
            assert_eq!(hex_encode(&h.finalize()), expect, "step={step} len={len}");
        }

        let mut st = Md5State::new();
        st.update(&data[..len / 3]);
        st.update(&data[len / 3..]);
        assert_eq!(hex_encode(&st.finalize()), expect, "state len={len}");
    }
}

#[cfg(feature = "digest")]
#[test]
fn digest_wrapper_matches_reference_across_splits_and_resets() {
    use md5_simd::{Digest, DigestMd5};
    for len in [0, 1, 31, 55, 56, 63, 64, 65, 119, 120, 127, 128, 4097] {
        let data = pattern(len);
        let expected = md5_crate_hex(&data);
        for step in [1, 7, 63, 64, 65, 4096] {
            let mut hasher = DigestMd5::new();
            for chunk in data.chunks(step) {
                Digest::update(&mut hasher, chunk);
            }
            assert_eq!(hex_encode(&hasher.clone().finalize()), expected);
            assert_eq!(
                hex_encode(&hasher.finalize_reset()),
                expected,
                "len={len}, step={step}"
            );
            assert_eq!(hex_encode(&hasher.finalize_reset()), md5_crate_hex(b""));
            Digest::update(&mut hasher, b"reuse");
            assert_eq!(hex_encode(&hasher.finalize()), md5_crate_hex(b"reuse"));
        }
    }
}

#[test]
fn resumed_state_and_wrapping_length_match_snapshot() {
    use md5_simd::{STATE_INIT, hazmat};
    let data = pattern(256);
    let mut state = STATE_INIT;
    hazmat::compress_blocks(&mut state, &data[..128]);
    let mut h = Md5::from_parts(state, 128);
    h.update(&data[128..]);
    assert_eq!(h.finalize(), digest(&data));

    // MD5 encodes length modulo 2^64 bits. These two counters have the same
    // encoded bit length, including after the byte counter wraps.
    let mut near_wrap = Md5::from_parts(state, u64::MAX - 63);
    let mut equivalent = Md5::from_parts(state, (1u64 << 61) - 64);
    for chunk in data.chunks(31) {
        near_wrap.update(chunk);
        equivalent.update(chunk);
        assert_eq!(
            near_wrap.finalize_snapshot(),
            equivalent.finalize_snapshot()
        );
    }
    assert_eq!(near_wrap.bytes_hashed(), 192);
}

#[test]
#[should_panic(expected = "input must contain complete MD5 blocks")]
fn hazmat_rejects_partial_blocks() {
    let mut state = md5_simd::STATE_INIT;
    md5_simd::hazmat::compress_blocks(&mut state, &[0; 65]);
}
