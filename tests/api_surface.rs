//! Public API contracts: Write / chain_update / digest_reader / hazmat / hex batch.

use md5_simd::{
    BLOCK_SIZE, DIGEST_LEN, STATE_INIT, available_backends, backend_name, digest, digest_hex,
    digest_opt_scalar, digest_portable, hazmat, hex_encode,
};

#[cfg(any(feature = "std", feature = "zeroize"))]
use md5_simd::Md5;
#[cfg(feature = "std")]
use md5_simd::Md5Engine;

#[test]
fn hex_encodes_the_entire_input() {
    for len in [0, 16, 32, 33, 64, 257] {
        let input: Vec<u8> = (0..len).map(|i| i as u8).collect();
        let expected: String = input.iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(hex_encode(&input), expected, "len={len}");
        let mut out = vec![0xcc; len * 2 + 3];
        assert_eq!(md5_simd::hex_encode_into(&input, &mut out), len * 2);
        assert_eq!(&out[..len * 2], expected.as_bytes());
        assert_eq!(&out[len * 2..], &[0xcc; 3]);
    }
}

#[cfg(feature = "std")]
#[test]
fn readers_retry_interruptions_and_propagate_errors() {
    use std::io::{self, Read};
    struct Reader {
        remaining: &'static [u8],
        interrupt: bool,
        fail: bool,
    }
    impl Read for Reader {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            if self.interrupt {
                self.interrupt = false;
                return Err(io::ErrorKind::Interrupted.into());
            }
            if self.remaining.is_empty() && self.fail {
                return Err(io::ErrorKind::PermissionDenied.into());
            }
            let n = self.remaining.len().min(buf.len()).min(7);
            buf[..n].copy_from_slice(&self.remaining[..n]);
            self.remaining = &self.remaining[n..];
            self.interrupt = n != 0;
            Ok(n)
        }
    }
    let reader = |fail| Reader {
        remaining: b"interrupted checksum reads",
        interrupt: true,
        fail,
    };
    assert_eq!(
        md5_simd::digest_reader(reader(false)).unwrap(),
        digest(reader(false).remaining)
    );
    for size in [0, 7, 65536, usize::MAX] {
        assert_eq!(
            md5_simd::etag_hex_streaming(reader(false), size).unwrap(),
            digest_hex(reader(false).remaining)
        );
    }
    assert_eq!(
        md5_simd::digest_reader(reader(true)).unwrap_err().kind(),
        io::ErrorKind::PermissionDenied
    );
    assert_eq!(
        md5_simd::etag_hex_streaming(reader(true), 4096)
            .unwrap_err()
            .kind(),
        io::ErrorKind::PermissionDenied
    );
}

#[test]
fn constants_and_backend_info() {
    assert_eq!(BLOCK_SIZE, 64);
    assert_eq!(DIGEST_LEN, 16);
    assert_eq!(STATE_INIT, [0x67452301, 0xefcdab89, 0x98badcfe, 0x10325476]);
    assert!(!backend_name().is_empty());
    assert!(!available_backends().is_empty());
}

#[test]
fn digest_hex_matches_hex_encode() {
    assert_eq!(digest_hex(b"abc"), hex_encode(&digest(b"abc")));
    assert_eq!(digest_hex(b"abc"), "900150983cd24fb0d6963f7d28e17f72");
}

#[test]
fn portable_and_opt_scalar_oracles_agree() {
    for len in [0usize, 1, 55, 56, 64, 100, 1024, 10_000] {
        let data: Vec<u8> = (0..len).map(|i| (i as u8).wrapping_mul(13)).collect();
        assert_eq!(
            digest_portable(&data),
            digest_opt_scalar(&data),
            "len={len}"
        );
        assert_eq!(digest_portable(&data), digest(&data), "active len={len}");
    }
}

#[test]
fn hazmat_compress_blocks_empty_ok() {
    let mut s = STATE_INIT;
    hazmat::compress_blocks(&mut s, &[]);
    assert_eq!(s, STATE_INIT);
}

#[cfg(feature = "std")]
#[test]
fn engine_hash_many_hex() {
    let engine = Md5Engine::new();
    let mut hexes = [String::new(), String::new()];
    engine.hash_many_hex(&[b"alpha", b"beta"], &mut hexes);
    assert_eq!(hexes[0], hex_encode(&digest(b"alpha")));
    assert_eq!(hexes[1], hex_encode(&digest(b"beta")));
}

#[cfg(feature = "std")]
#[test]
fn write_and_reader_surface() {
    use std::io::{Cursor, Write};

    let payload: Vec<u8> = (0..50_000).map(|i| (i % 256) as u8).collect();

    let mut h = Md5::new();
    h.write_all(&payload).unwrap();
    assert_eq!(hex_encode(&h.finalize()), hex_encode(&digest(&payload)));

    let via_reader = md5_simd::digest_reader(Cursor::new(&payload)).unwrap();
    assert_eq!(hex_encode(&via_reader), hex_encode(&digest(&payload)));

    let chained = Md5::new()
        .chain_update(&payload[..1000])
        .chain_update(&payload[1000..])
        .finalize();
    assert_eq!(hex_encode(&chained), hex_encode(&digest(&payload)));
}

#[cfg(feature = "zeroize")]
#[test]
fn zeroize_feature_drop_compiles() {
    let mut h = Md5::new();
    h.update(b"abc");
    h.zeroize();
    assert_eq!(h.bytes_hashed(), 0);
}
