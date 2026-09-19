//! Multi-buffer engine + SIMD / pair correctness.

use md5_simd::{
    Md5Engine, Md5State, digest, hex_encode, md5_many, pair_path_active, simd_active, simd_name,
};

fn ref_hex(data: &[u8]) -> String {
    use md5::Digest;
    hex_encode(md5::Md5::digest(data).as_slice())
}

fn pattern(len: usize, salt: u8) -> Vec<u8> {
    (0..len)
        .map(|i| (i as u8).wrapping_mul(17).wrapping_add(salt))
        .collect()
}

#[test]
fn hash_many_equal_pairs_match_reference() {
    let engine = Md5Engine::new();
    for len in [64usize, 128, 256, 1024, 4096, 64 * 30] {
        for count in [2usize, 3, 4, 5, 8, 16] {
            let storage: Vec<Vec<u8>> = (0..count).map(|i| pattern(len, i as u8)).collect();
            let inputs: Vec<&[u8]> = storage.iter().map(|v| v.as_slice()).collect();
            let mut outputs = vec![[0u8; 16]; count];
            engine.hash_many(&inputs, &mut outputs);
            for (msg, out) in storage.iter().zip(outputs.iter()) {
                assert_eq!(hex_encode(out), ref_hex(msg), "len={len} count={count}");
            }
        }
    }
}

#[test]
fn hash_many_mixed_lengths_match_reference() {
    let engine = Md5Engine::new();
    let msgs: [&[u8]; 6] = [
        b"",
        b"abc",
        &[0x5au8; 55],
        &[0xa5u8; 64],
        &[0x3cu8; 130],
        &[0x11u8; 200],
    ];
    let mut out = [[0u8; 16]; 6];
    engine.hash_many(&msgs, &mut out);
    for (m, o) in msgs.iter().zip(out.iter()) {
        assert_eq!(hex_encode(o), ref_hex(m), "len={}", m.len());
    }
}

#[test]
fn pair_path_public_api() {
    let engine = Md5Engine::new();
    let lanes = engine.lanes();
    // 1=off, 2=scalar pair, 4/8/16=NEON / AVX2 / AVX-512
    assert!(
        matches!(lanes, 1 | 2 | 4 | 8 | 16),
        "unexpected lanes={lanes}"
    );
    eprintln!(
        "backend={} lanes={} simd_active={} simd_name={} pair={}",
        engine.backend_name(),
        lanes,
        simd_active(),
        simd_name(),
        pair_path_active()
    );

    // Equal-length batch must still match md-5 on this host/kernel.
    let count = lanes.max(4);
    let storage: Vec<Vec<u8>> = (0..count).map(|i| pattern(1024, i as u8)).collect();
    let inputs: Vec<&[u8]> = storage.iter().map(|v| v.as_slice()).collect();
    let mut outputs = vec![[0u8; 16]; count];
    engine.hash_many(&inputs, &mut outputs);
    for (msg, out) in storage.iter().zip(outputs.iter()) {
        assert_eq!(hex_encode(out), ref_hex(msg));
    }

    if pair_path_active() {
        let a = pattern(1024, 1);
        let b = pattern(1024, 2);
        let (d0, d1) = md5_simd::hash_pair(&a, &b);
        assert_eq!(hex_encode(&d0), ref_hex(&a));
        assert_eq!(hex_encode(&d1), ref_hex(&b));
    }
}

#[test]
fn md5_many_free_fn() {
    let inputs: [&[u8]; 4] = [b"one", b"two", b"three", b"four"];
    let mut outputs = [[0u8; 16]; 4];
    md5_many(&inputs, &mut outputs);
    for (i, o) in inputs.iter().zip(outputs.iter()) {
        assert_eq!(*o, digest(i));
    }
}

#[test]
fn update_finalize_many_still_snapshot() {
    let engine = Md5Engine::new();
    let mut states = [Md5State::new(); 2];
    engine.update_many(&mut states, &[b"alpha ", b"beta "]);
    let mut out1 = [[0u8; 16]; 2];
    engine.finalize_many(&states, &mut out1);
    engine.update_many(&mut states, &[b"gamma", b"delta"]);
    let mut out2 = [[0u8; 16]; 2];
    engine.finalize_many(&states, &mut out2);
    assert_eq!(hex_encode(&out1[0]), ref_hex(b"alpha "));
    assert_eq!(hex_encode(&out2[0]), ref_hex(b"alpha gamma"));
    assert_eq!(hex_encode(&out2[1]), ref_hex(b"beta delta"));
}
