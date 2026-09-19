//! Batch hashing with software pair-interleave.

fn main() {
    let engine = md5_simd::Md5Engine::new();
    println!("backend = {}", engine.backend_name());
    println!("lanes   = {}", engine.lanes());
    println!("pair    = {}", md5_simd::pair_path_active());

    let storage: Vec<Vec<u8>> = (0..4)
        .map(|lane| {
            (0..4096)
                .map(|i| (i as u8).wrapping_add(lane as u8))
                .collect()
        })
        .collect();
    let inputs: Vec<&[u8]> = storage.iter().map(|v| v.as_slice()).collect();
    let mut outputs = [[0u8; 16]; 4];
    engine.hash_many(&inputs, &mut outputs);

    for (msg, out) in storage.iter().zip(outputs.iter()) {
        let expect = md5_simd::digest(msg);
        assert_eq!(*out, expect);
        println!("len={} digest={}", msg.len(), md5_simd::hex_encode(out));
    }
    println!("pair path OK");
}
