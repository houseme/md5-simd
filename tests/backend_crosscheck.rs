//! Backend dispatch and differential checks across feature profiles.

use md5_simd::{
    Md5, Md5Engine, Md5State, available_backends, backend_name, digest, hex_encode,
    is_portable_only,
};

fn ref_hex(data: &[u8]) -> String {
    use md5::Digest;
    hex_encode(md5::Md5::digest(data).as_slice())
}

#[test]
fn backend_name_is_nonempty_and_logged() {
    let name = backend_name();
    assert!(!name.is_empty());
    eprintln!("active backend   = {name}");
    eprintln!("portable_only    = {}", is_portable_only());
    eprintln!("available        = {:?}", available_backends());
}

#[test]
fn backend_name_matches_cfg() {
    let name = backend_name();
    if cfg!(feature = "force-portable") {
        assert!(name.contains("portable"), "got {name}");
        assert!(is_portable_only());
        assert!(!available_backends().iter().any(|s| s.contains("asm")));
        return;
    }
    assert!(!is_portable_only());
    let expect_asm = cfg!(feature = "opt");
    let is_asm = name.contains("single-asm");
    let expect_asm = expect_asm && cfg!(target_arch = "x86_64");
    assert_eq!(is_asm, expect_asm, "backend name={name}");
    if !expect_asm {
        assert!(name.contains("in-tree"), "got {name}");
    }
}

/// Only meaningful when the `opt` profile is enabled.
#[cfg(all(
    feature = "opt",
    not(feature = "force-portable"),
    target_arch = "x86_64"
))]
#[test]
fn opt_profile_reports_single_asm_backend() {
    let name = backend_name();
    assert!(
        name.contains("single-asm"),
        "opt profile should select vendored single-asm, got {name}"
    );
    assert!(!is_portable_only());
    eprintln!("opt backend={name} simd={}", md5_simd::simd_name());
}

#[test]
fn digest_stable_across_invocation() {
    let a = digest(b"cross-check");
    let b = digest(b"cross-check");
    assert_eq!(a, b);
    assert_eq!(hex_encode(&a), hex_encode(&b));
}

/// Golden digests — must stay byte-identical under every backend.
#[test]
fn golden_workload_invariant() {
    assert_eq!(hex_encode(&digest(b"")), "d41d8cd98f00b204e9800998ecf8427e");
    assert_eq!(
        hex_encode(&digest(b"abc")),
        "900150983cd24fb0d6963f7d28e17f72"
    );
    assert_eq!(
        hex_encode(&digest(b"message digest")),
        "f96b697d7cb7938d525a2f31aaf161d0"
    );

    for len in [0usize, 1, 55, 56, 63, 64, 65, 128, 129, 1024, 4097] {
        let data: Vec<u8> = (0..len)
            .map(|i| (i as u8).wrapping_mul(19).wrapping_add(3))
            .collect();
        let expect = ref_hex(&data);
        assert_eq!(hex_encode(&digest(&data)), expect, "oneshot len={len}");

        let mut h = Md5::new();
        h.update(&data);
        assert_eq!(hex_encode(&h.finalize()), expect, "stream len={len}");

        let mut st = Md5State::new();
        st.update(&data);
        assert_eq!(hex_encode(&st.finalize()), expect, "state len={len}");
    }

    let engine = Md5Engine::new();
    let mut outs = [[0u8; 16]; 3];
    let inputs: [&[u8]; 3] = [b"alpha", &[1u8; 200], b""];
    engine.hash_many(&inputs, &mut outs);
    for (input, output) in inputs.iter().zip(outs.iter()) {
        assert_eq!(hex_encode(output), ref_hex(input));
    }
}

/// Active backend must produce the same digests as RustCrypto `md-5`.
#[test]
fn active_backend_matches_md5_crate() {
    for len in [0usize, 1, 55, 56, 64, 100, 256, 4096, 65_536] {
        let data: Vec<u8> = (0..len).map(|i| (i as u8).wrapping_add(7)).collect();
        assert_eq!(hex_encode(&digest(&data)), ref_hex(&data), "len={len}");
    }
}

/// Forced-portable builds must never advertise a compiled asm backend.
#[test]
fn reported_backends_match_portable_override() {
    let name = backend_name();
    if is_portable_only() {
        assert!(
            name.contains("portable"),
            "portable_only=true but backend_name={name}"
        );
    }
    assert!(!available_backends().is_empty());
}
