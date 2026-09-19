//! Property-based differential tests against RustCrypto `md-5`.
//!
//! Covers random message lengths, random streaming splits, multi-stream
//! states, and batch engine hashing against an independent reference.

use md5_simd::{Md5, Md5Engine, Md5State, digest, hex_encode};

fn ref_hex(data: &[u8]) -> String {
    use md5::Digest;
    hex_encode(md5::Md5::digest(data).as_slice())
}

proptest::proptest! {
    #![proptest_config(proptest::test_runner::Config::with_cases(64))]

    /// One-shot + arbitrary streaming splits + Md5State + engine must all
    /// match RustCrypto `md-5` on the same bytes.
    #[test]
    fn random_messages_match_md5_crate(
        data in proptest::collection::vec(proptest::prelude::any::<u8>(), 0..2048),
        splits in proptest::collection::vec(1usize..300usize, 0..8),
    ) {
        let expect = ref_hex(&data);
        proptest::prop_assert_eq!(&hex_encode(&digest(&data)), &expect);

        // Streaming with irregular splits (may not cover the whole buffer in
        // the loop; finish the remainder explicitly).
        let mut h = Md5::new();
        let mut offset = 0usize;
        for s in splits.iter().cycle() {
            if offset >= data.len() {
                break;
            }
            let end = (offset + *s).min(data.len());
            h.update(&data[offset..end]);
            offset = end;
        }
        if offset < data.len() {
            h.update(&data[offset..]);
        }
        proptest::prop_assert_eq!(&hex_encode(&h.finalize()), &expect);

        // Clone-then-finalize mid-stream must equal the prefix digest.
        if data.len() >= 2 {
            let cut = data.len() / 2;
            let mut h2 = Md5::new();
            h2.update(&data[..cut]);
            let mid = h2.clone().finalize();
            proptest::prop_assert_eq!(&hex_encode(&mid), &ref_hex(&data[..cut]));
            h2.update(&data[cut..]);
            proptest::prop_assert_eq!(&hex_encode(&h2.finalize()), &expect);
        }

        // Multi-stream state in two uneven halves.
        let mut st = Md5State::new();
        let mid = data.len() / 3;
        st.update(&data[..mid]);
        st.update(&data[mid..]);
        proptest::prop_assert_eq!(&hex_encode(&st.finalize()), &expect);

        // Snapshot finalize is non-destructive: digest again after more data.
        let mut st2 = Md5State::new();
        st2.update(&data);
        let snap = st2.finalize();
        proptest::prop_assert_eq!(&hex_encode(&snap), &expect);
        st2.update(b"x");
        proptest::prop_assert_eq!(&hex_encode(&st2.finalize()), &ref_hex(&{
            let mut v = data.clone();
            v.push(b'x');
            v
        }));

        // Batch engine agrees with single-stream.
        let engine = Md5Engine::new();
        let mut outs = [[0u8; 16]; 2];
        engine.hash_many(&[data.as_slice(), b""], &mut outs);
        proptest::prop_assert_eq!(&hex_encode(&outs[0]), &expect);
        proptest::prop_assert_eq!(
            &hex_encode(&outs[1]),
            "d41d8cd98f00b204e9800998ecf8427e"
        );
    }

    /// Mixed-length multi-message batches must match per-lane reference digests.
    #[test]
    fn random_batches_match_reference(
        messages in proptest::collection::vec(
            proptest::collection::vec(proptest::prelude::any::<u8>(), 0..513),
            0..17,
        )
    ) {
        let engine = Md5Engine::new();
        let inputs: Vec<&[u8]> = messages.iter().map(|v| v.as_slice()).collect();
        let mut outputs = vec![[0u8; 16]; inputs.len()];
        if !inputs.is_empty() {
            engine.hash_many(&inputs, &mut outputs);
            for (input, output) in inputs.iter().zip(outputs.iter()) {
                proptest::prop_assert_eq!(&hex_encode(output), &ref_hex(input));
            }
        }

        // Incremental many with random per-lane chunk cuts.
        let mut states = vec![Md5State::new(); inputs.len()];
        let mut offsets = vec![0usize; inputs.len()];
        let mut round = 0usize;
        while offsets.iter().zip(&messages).any(|(&off, msg)| off < msg.len()) {
            let chunks: Vec<&[u8]> = messages
                .iter()
                .enumerate()
                .map(|(lane, msg)| {
                    let start = offsets[lane];
                    let remaining = msg.len() - start;
                    let proposed = 1 + ((round * 17 + lane * 29) % 97);
                    let take = remaining.min(proposed);
                    &msg[start..start + take]
                })
                .collect();
            engine.update_many(&mut states, &chunks);
            for (lane, chunk) in chunks.iter().enumerate() {
                offsets[lane] += chunk.len();
            }
            round += 1;
        }
        let mut finals = vec![[0u8; 16]; inputs.len()];
        engine.finalize_many(&states, &mut finals);
        for (input, output) in inputs.iter().zip(finals.iter()) {
            proptest::prop_assert_eq!(&hex_encode(output), &ref_hex(input));
        }
    }

    /// Padding-boundary stress: lengths around 55/56/63/64/119/120/127/128
    /// with random tail bytes must still match `md-5`.
    #[test]
    fn padding_boundary_lengths_match(
        len in proptest::prop_oneof![
            proptest::prelude::Just(0usize),
            1usize..8,
            50usize..70,
            110usize..135,
            250usize..270,
        ],
        seed in proptest::prelude::any::<u8>(),
    ) {
        let data: Vec<u8> = (0..len)
            .map(|i| (i as u8).wrapping_mul(seed).wrapping_add(3))
            .collect();
        let expect = ref_hex(&data);
        proptest::prop_assert_eq!(&hex_encode(&digest(&data)), &expect);

        for step in [1usize, 13, 64, 65] {
            let mut h = Md5::new();
            for chunk in data.chunks(step) {
                h.update(chunk);
            }
            proptest::prop_assert_eq!(&hex_encode(&h.finalize()), &expect);
        }
    }
}
