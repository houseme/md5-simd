//! Basic one-shot / streaming usage.

fn main() {
    let payload = b"The quick brown fox jumps over the lazy dog";
    let d = md5_simd::digest(payload);
    println!("backend  = {}", md5_simd::backend_name());
    println!("available= {:?}", md5_simd::available_backends());
    println!("digest   = {}", md5_simd::hex_encode(&d));

    let mut h = md5_simd::Md5::new();
    h.update(b"The quick brown fox ");
    let mid = h.clone().finalize();
    println!("mid      = {}", md5_simd::hex_encode(&mid));
    h.update(b"jumps over the lazy dog");
    println!("stream   = {}", md5_simd::hex_encode(&h.finalize()));

    let engine = md5_simd::Md5Engine::new();
    let mut outs = [[0u8; 16]; 2];
    engine.hash_many(&[b"alpha", b"beta"], &mut outs);
    println!(
        "many     = {} {}",
        md5_simd::hex_encode(&outs[0]),
        md5_simd::hex_encode(&outs[1])
    );
}
