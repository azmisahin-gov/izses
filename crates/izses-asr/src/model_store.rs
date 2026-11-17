//! Whisper model dosyalarının yönetimi.
//!
//! Model ağırlıkları, tokenizer ve yapılandırma dosyası kurulum sırasında
//! yerel bilgisayara yerleştirilir. Uygulama çalışırken hiçbir dosya
//! internetten indirilmez. Model bütünlüğü SHA-256 ile doğrulanır.

use std::path::{Path, PathBuf};

use izses_core::error::{CoreError, CoreResult};
use izses_core::hash;

/// Kullanılacak modelin tanımı.
#[derive(Debug, Clone)]
pub struct ModelSpec {
    /// Kullanıcıya gösterilecek ad.
    pub display_name: &'static str,
    /// Hugging Face üzerindeki depo adı (yalnızca kurulum betiği kullanır).
    pub repo: &'static str,
    /// Beklenen dosya adları.
    pub weights_file: &'static str,
    pub tokenizer_file: &'static str,
    pub config_file: &'static str,
    /// Mel kanal sayısı.
    pub num_mel_bins: usize,
}

/// Whisper large-v3-turbo modeli.
pub const LARGE_V3_TURBO: ModelSpec = ModelSpec {
    display_name: "Whisper large-v3-turbo",
    repo: "openai/whisper-large-v3-turbo",
    weights_file: "model.safetensors",
    tokenizer_file: "tokenizer.json",
    config_file: "config.json",
    num_mel_bins: 128,
};

/// Yerel model deposu.
#[derive(Debug, Clone)]
pub struct ModelStore {
    root: PathBuf,
}

/// Model dosyalarının durumu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModelState {
    /// Tüm dosyalar var ve doğrulandı.
    Ready,
    /// Dosyalar eksik.
    Missing { missing: Vec<String> },
    /// Dosyalar var ama özet uyuşmuyor.
    Corrupt { detail: String },
}

impl ModelState {
    pub fn is_ready(&self) -> bool {
        matches!(self, ModelState::Ready)
    }

    pub fn label_tr(&self) -> String {
        match self {
            ModelState::Ready => "Hazır".to_string(),
            ModelState::Missing { missing } => {
                format!("Eksik dosyalar: {}", missing.join(", "))
            }
            ModelState::Corrupt { detail } => format!("Bozuk model: {detail}"),
        }
    }
}

impl ModelStore {
    /// Model deposunu tanımlar.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Model dosyalarının beklenen konumu.
    pub fn model_dir(&self, spec: &ModelSpec) -> PathBuf {
        // Depo adının son parçası klasör adı olur.
        let name = spec.repo.rsplit('/').next().unwrap_or("whisper");
        self.root.join(name)
    }

    pub fn weights_path(&self, spec: &ModelSpec) -> PathBuf {
        self.model_dir(spec).join(spec.weights_file)
    }

    pub fn tokenizer_path(&self, spec: &ModelSpec) -> PathBuf {
        self.model_dir(spec).join(spec.tokenizer_file)
    }

    pub fn config_path(&self, spec: &ModelSpec) -> PathBuf {
        self.model_dir(spec).join(spec.config_file)
    }

    /// Model dosyalarının var olup olmadığını denetler.
    pub fn check_present(&self, spec: &ModelSpec) -> ModelState {
        let mut missing = Vec::new();
        for p in [
            self.weights_path(spec),
            self.tokenizer_path(spec),
            self.config_path(spec),
        ] {
            if !p.is_file() {
                missing.push(
                    p.file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_default(),
                );
            }
        }
        if missing.is_empty() {
            ModelState::Ready
        } else {
            ModelState::Missing { missing }
        }
    }

    /// Model ağırlık dosyasının SHA-256 değerini hesaplar.
    ///
    /// Bu değer rapora ve işlem geçmişine yazılır.
    pub fn weights_digest(&self, spec: &ModelSpec) -> CoreResult<String> {
        let path = self.weights_path(spec);
        if !path.is_file() {
            return Err(CoreError::not_found("model ağırlık dosyası"));
        }
        hash::sha256_file(&path)
    }

    /// Model dosyalarını, beklenen özet ile karşılaştırarak doğrular.
    ///
    /// `expected_weights_sha256` verilmezse yalnızca varlık denetimi yapılır.
    pub fn verify(
        &self,
        spec: &ModelSpec,
        expected_weights_sha256: Option<&str>,
    ) -> CoreResult<ModelState> {
        match self.check_present(spec) {
            ModelState::Ready => {}
            other => return Ok(other),
        }
        if let Some(expected) = expected_weights_sha256 {
            let actual = self.weights_digest(spec)?;
            if !actual.eq_ignore_ascii_case(expected) {
                return Ok(ModelState::Corrupt {
                    detail: format!(
                        "ağırlık dosyası özeti beklenenden farklı (beklenen: {}…)",
                        &expected[..expected.len().min(12)]
                    ),
                });
            }
        }
        Ok(ModelState::Ready)
    }

    /// Model dosyalarını başka bir konuma kopyalar (kurulum/yedekleme).
    pub fn install_from(&self, spec: &ModelSpec, source_dir: &Path) -> CoreResult<()> {
        let dest = self.model_dir(spec);
        std::fs::create_dir_all(&dest)?;
        for name in [spec.weights_file, spec.tokenizer_file, spec.config_file] {
            let src = source_dir.join(name);
            if !src.is_file() {
                return Err(CoreError::not_found(format!(
                    "kaynak model dosyası: {name}"
                )));
            }
            std::fs::copy(&src, dest.join(name))?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_missing_model_files() {
        let tmp = tempfile::tempdir().unwrap();
        let store = ModelStore::new(tmp.path());
        let state = store.check_present(&LARGE_V3_TURBO);
        match state {
            ModelState::Missing { missing } => assert_eq!(missing.len(), 3),
            other => panic!("beklenen Missing, gelen {other:?}"),
        }
    }

    #[test]
    fn ready_when_all_files_present() {
        let tmp = tempfile::tempdir().unwrap();
        let store = ModelStore::new(tmp.path());
        let dir = store.model_dir(&LARGE_V3_TURBO);
        std::fs::create_dir_all(&dir).unwrap();
        for name in [
            LARGE_V3_TURBO.weights_file,
            LARGE_V3_TURBO.tokenizer_file,
            LARGE_V3_TURBO.config_file,
        ] {
            std::fs::write(dir.join(name), b"x").unwrap();
        }
        assert!(store.check_present(&LARGE_V3_TURBO).is_ready());
    }

    #[test]
    fn detects_corrupt_weights() {
        let tmp = tempfile::tempdir().unwrap();
        let store = ModelStore::new(tmp.path());
        let dir = store.model_dir(&LARGE_V3_TURBO);
        std::fs::create_dir_all(&dir).unwrap();
        for name in [
            LARGE_V3_TURBO.weights_file,
            LARGE_V3_TURBO.tokenizer_file,
            LARGE_V3_TURBO.config_file,
        ] {
            std::fs::write(dir.join(name), b"x").unwrap();
        }
        let wrong = "0".repeat(64);
        let state = store.verify(&LARGE_V3_TURBO, Some(&wrong)).unwrap();
        assert!(matches!(state, ModelState::Corrupt { .. }));
    }

    #[test]
    fn verifies_matching_digest() {
        let tmp = tempfile::tempdir().unwrap();
        let store = ModelStore::new(tmp.path());
        let dir = store.model_dir(&LARGE_V3_TURBO);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(LARGE_V3_TURBO.weights_file), b"weights").unwrap();
        for name in [LARGE_V3_TURBO.tokenizer_file, LARGE_V3_TURBO.config_file] {
            std::fs::write(dir.join(name), b"x").unwrap();
        }
        let digest = hash::sha256_bytes(b"weights");
        assert!(store
            .verify(&LARGE_V3_TURBO, Some(&digest))
            .unwrap()
            .is_ready());
    }

    #[test]
    fn install_copies_all_files() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src");
        std::fs::create_dir_all(&src).unwrap();
        for name in [
            LARGE_V3_TURBO.weights_file,
            LARGE_V3_TURBO.tokenizer_file,
            LARGE_V3_TURBO.config_file,
        ] {
            std::fs::write(src.join(name), b"data").unwrap();
        }
        let store = ModelStore::new(tmp.path().join("models"));
        store.install_from(&LARGE_V3_TURBO, &src).unwrap();
        assert!(store.check_present(&LARGE_V3_TURBO).is_ready());
    }
}
