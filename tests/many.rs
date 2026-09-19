//! Multi-buffer engine API consistency.

use md5_simd::{Md5Engine, Md5State, digest, hex_encode};

#[test]
fn hash_many_matches_single() {
    let engine = Md5Engine::new();
    for n in 1..=16usize {
        let storage: Vec<Vec<u8>> = (0..n)
            .map(|lane| {
                let len = 64 + lane * 37;
                (0..len).map(|i| (lane * 13 + i) as u8).collect()
            })
            .collect();
        let inputs: Vec<&[u8]> = storage.iter().map(|v| v.as_slice()).collect();
        let mut outputs = vec![[0u8; 16]; n];
        engine.hash_many(&inputs, &mut outputs);
        for (input, output) in inputs.iter().zip(outputs.iter()) {
            assert_eq!(*output, digest(input));
        }
    }
}

#[test]
fn hash_many_mixed_lengths() {
    let engine = Md5Engine::new();
    let storage: [&[u8]; 6] = [
        b"",
        b"abc",
        &[0x5au8; 55],
        &[0xa5u8; 64],
        &[0x3cu8; 129],
        &[0x7eu8; 500],
    ];
    let mut outputs = [[0u8; 16]; 6];
    engine.hash_many(&storage, &mut outputs);
    for (input, output) in storage.iter().zip(outputs.iter()) {
        assert_eq!(*output, digest(input), "len={}", input.len());
    }
}

#[test]
fn update_finalize_many_snapshot() {
    let engine = Md5Engine::new();
    let messages: [Vec<u8>; 4] = [
        b"alpha beta gamma".to_vec(),
        b"".to_vec(),
        vec![1u8; 200],
        b"object-etag".to_vec(),
    ];
    let mut states = [Md5State::new(); 4];

    engine.update_many(
        &mut states,
        &[
            &messages[0][..5],
            &messages[1],
            &messages[2][..64],
            &messages[3][..4],
        ],
    );
    engine.update_many(
        &mut states,
        &[
            &messages[0][5..],
            &messages[1],
            &messages[2][64..],
            &messages[3][4..],
        ],
    );

    let mut finals = [[0u8; 16]; 4];
    engine.finalize_many(&states, &mut finals);
    for (msg, out) in messages.iter().zip(finals.iter()) {
        assert_eq!(*out, digest(msg));
        assert_eq!(hex_encode(out), hex_encode(&digest(msg)));
    }

    // Snapshot finalize must not consume state: append more data and re-finalize.
    engine.update_many(&mut states, &[b"!", b"x", b"yy", b"zzz"]);
    let mut finals2 = [[0u8; 16]; 4];
    engine.finalize_many(&states, &mut finals2);
    let expect: Vec<Vec<u8>> = messages
        .iter()
        .zip([
            b"!".as_slice(),
            b"x".as_slice(),
            b"yy".as_slice(),
            b"zzz".as_slice(),
        ])
        .map(|(msg, extra)| {
            let mut v = msg.clone();
            v.extend_from_slice(extra);
            v
        })
        .collect();
    for (msg, out) in expect.iter().zip(finals2.iter()) {
        assert_eq!(*out, digest(msg));
    }
}

#[test]
#[should_panic]
fn hash_many_short_outputs_panics() {
    let engine = Md5Engine::new();
    let mut outputs = [[0u8; 16]; 1];
    engine.hash_many(&[b"a", b"b"], &mut outputs);
}

#[test]
fn equal_runs_cover_lane_group_padding_and_alignment_boundaries() {
    use md5::Digest;
    let engine = Md5Engine::new();
    for count in [
        0, 1, 3, 4, 5, 7, 8, 9, 15, 16, 17, 31, 32, 33, 63, 64, 65, 129,
    ] {
        for len in [0, 31, 32, 55, 56, 63, 64, 65, 119, 120, 127, 128, 129, 1025] {
            let storage: Vec<Vec<u8>> = (0..count)
                .map(|lane| {
                    (0..len + 1)
                        .map(|i| (i as u8).wrapping_mul(31).wrapping_add(lane as u8))
                        .collect()
                })
                .collect();
            // Offset each slice by one byte; vector reads must allow this.
            let inputs: Vec<&[u8]> = storage.iter().map(|msg| &msg[1..]).collect();
            let sentinel = [0xa5; 16];
            let mut outputs = vec![sentinel; count + 2];
            engine.hash_many(&inputs, &mut outputs);
            for (input, output) in inputs.iter().zip(&outputs) {
                assert_eq!(
                    output.as_slice(),
                    md5::Md5::digest(input).as_slice(),
                    "count={count}, len={len}"
                );
            }
            assert_eq!(&outputs[count..], &[sentinel; 2]);
        }
    }
}

#[test]
fn mixed_batch_preserves_equal_run_order_and_tail() {
    use md5::Digest;
    let lengths = [55, 56, 63, 64, 65, 127, 128, 129];
    let storage: Vec<Vec<u8>> = lengths
        .into_iter()
        .enumerate()
        .flat_map(|(run, len)| {
            (0..(run * 9 + 1)).map(move |lane| vec![(run * 13 + lane) as u8; len])
        })
        .collect();
    let inputs: Vec<&[u8]> = storage.iter().map(Vec::as_slice).collect();
    let mut outputs = vec![[0; 16]; inputs.len()];
    Md5Engine::new().hash_many(&inputs, &mut outputs);
    for (input, output) in inputs.iter().zip(outputs) {
        assert_eq!(output.as_slice(), md5::Md5::digest(input).as_slice());
    }
}
