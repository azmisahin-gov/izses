//! SHA-256 bütünlük doğrulaması.
//!
//! Büyük dosyalarda bellek kullanımını sabit tutmak için akış hâlinde okunur.
//! Kaynak dosyalar yalnızca okunur biçimde açılır; hiçbir durumda yazılmaz.

use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;

use sha2::{Digest, Sha256};

use crate::error::CoreResult;

/// Arabellek boyutu: 1 MiB.
const CHUNK: usize = 1024 * 1024;

/// Bir dosyanın SHA-256 özetini hesaplar ve küçük harfli onaltılık döndürür.
pub fn sha256_file(path: &Path) -> CoreResult<String> {
    let file = File::open(path)?;
    let mut reader = BufReader::with_capacity(CHUNK, file);
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; CHUNK];
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex::encode(hasher.finalize()))
}

/// Kaynak ve hedef dosyaların özetlerini karşılaştırır.
pub fn verify_identical(source: &Path, copy: &Path) -> CoreResult<bool> {
    let a = sha256_file(source)?;
    let b = sha256_file(copy)?;
    Ok(a == b)
}

/// Bayt dilimi için SHA-256 (testler ve küçük veriler için).
pub fn sha256_bytes(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hex::encode(hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn hash_matches_known_vector() {
        // "abc" için bilinen SHA-256 değeri.
        let digest = sha256_bytes(b"abc");
        assert_eq!(
            digest,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn verify_detects_difference() {
        let tmp = tempfile::tempdir().unwrap();
        let a = tmp.path().join("a.bin");
        let b = tmp.path().join("b.bin");
        std::fs::write(&a, b"hello world").unwrap();
        std::fs::write(&b, b"hello world").unwrap();
        assert!(verify_identical(&a, &b).unwrap());

        let mut f = std::fs::File::create(&b).unwrap();
        f.write_all(b"hello worlD").unwrap();
        assert!(!verify_identical(&a, &b).unwrap());
    }

    #[test]
    fn hashes_large_file_without_loading_fully() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("big.bin");
        let data = vec![7u8; 3 * 1024 * 1024 + 123];
        std::fs::write(&p, &data).unwrap();
        let digest = sha256_file(&p).unwrap();
        assert_eq!(digest, sha256_bytes(&data));
    }
}
