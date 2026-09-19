//! Differential tests against RustCrypto `md-5` (dev-dependency).

use md5_simd::{Md5, Md5Engine, Md5State, digest, hex_encode};

fn ref_hex(data: &[u8]) -> String {
    use md5::Digest;
    hex_encode(md5::Md5::digest(data).as_slice())
}

#[test]
fn oneshot_matches_md5_crate() {
    for len in [0usize, 1, 16, 55, 56, 64, 65, 1000, 4097] {
        let data: Vec<u8> = (0..len).map(|i| (i as u8).wrapping_mul(19)).collect();
        assert_eq!(hex_encode(&digest(&data)), ref_hex(&data), "len={len}");
    }
}

#[test]
fn streaming_split_matches_md5_crate() {
    let data: Vec<u8> = (0..5000).map(|i| (i % 251) as u8).collect();
    let expect = ref_hex(&data);
    for step in [1usize, 13, 64, 65, 1024] {
        let mut h = Md5::new();
        for chunk in data.chunks(step) {
            h.update(chunk);
        }
        assert_eq!(hex_encode(&h.finalize()), expect, "step={step}");
    }
}

#[test]
fn multi_stream_matches_md5_crate() {
    let engine = Md5Engine::new();
    let messages: [&[u8]; 5] = [b"", b"x", b"hello md5-simd", &[7u8; 130], &[9u8; 4096]];

    let mut states = [Md5State::new(); 5];
    let first: Vec<&[u8]> = messages.iter().map(|m| &m[..m.len() / 3]).collect();
    let second: Vec<&[u8]> = messages.iter().map(|m| &m[m.len() / 3..]).collect();
    engine.update_many(&mut states, &first);
    engine.update_many(&mut states, &second);

    let mut outs = [[0u8; 16]; 5];
    engine.finalize_many(&states, &mut outs);
    for (msg, out) in messages.iter().zip(outs.iter()) {
        assert_eq!(hex_encode(out), ref_hex(msg), "msg_len={}", msg.len());
    }
}
