//! Varsayılan dizinler ve uygulama yapılandırması.
//!
//! Uygulama tüm verisini kullanıcının belgeleri altındaki `IZSES`
//! klasöründe tutar. Kurulum sırasında bu klasörler oluşturulur; kullanıcı
//! ayarlardan farklı bir konum seçebilir.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Uygulama kök klasör adı.
pub const APP_DIR: &str = "IZSES";

/// Kullanıcının belge klasörünü bulur.
pub fn documents_dir() -> PathBuf {
    #[cfg(windows)]
    {
        if let Ok(profile) = std::env::var("USERPROFILE") {
            let p = PathBuf::from(&profile).join("Documents");
            if p.is_dir() {
                return p;
            }
            return PathBuf::from(profile);
        }
    }
    if let Ok(home) = std::env::var("HOME") {
        let docs = PathBuf::from(&home).join("Documents");
        if docs.is_dir() {
            return docs;
        }
        return PathBuf::from(home);
    }
    PathBuf::from(".")
}

/// Varsayılan uygulama kökü: `<Belgeler>/IZSES`.
pub fn default_root() -> PathBuf {
    documents_dir().join(APP_DIR)
}

/// Uygulama ayarları.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// İnceleme verilerinin kök klasörü.
    pub storage_root: PathBuf,
    /// Whisper model dosyalarının bulunduğu klasör.
    pub model_root: PathBuf,
    /// GPU hızlandırma tercih edilsin mi.
    ///
    /// GPU gerçekten kullanılamıyorsa uygulama otomatik olarak CPU'ya döner.
    pub prefer_gpu: bool,
    /// Varsayılan dil.
    pub language: String,
    /// Deşifre için zaman damgası üret.
    pub timestamps: bool,
    /// Arayüzde son açılan incelemelerin listesi.
    pub recent_investigations: Vec<String>,
    /// Son açılan incelemelerin tutulacağı azami sayı.
    pub max_recent: usize,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            storage_root: default_root(),
            model_root: default_root().join("modeller"),
            prefer_gpu: false,
            language: "tr".to_string(),
            timestamps: true,
            recent_investigations: Vec::new(),
            max_recent: 10,
        }
    }
}

impl Settings {
    /// Ayar dosyasının yolu.
    pub fn config_path(storage_root: &Path) -> PathBuf {
        storage_root.join("ayarlar.json")
    }

    /// Ayarları yükler; yoksa varsayılanları oluşturup kaydeder.
    pub fn load_or_default() -> Self {
        let root = default_root();
        let path = Self::config_path(&root);
        match std::fs::read_to_string(&path) {
            Ok(text) => serde_json::from_str(&text).unwrap_or_default(),
            Err(_) => {
                let s = Self::default();
                let _ = s.save();
                s
            }
        }
    }

    /// Ayarları diske yazar.
    pub fn save(&self) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.storage_root)?;
        let path = Self::config_path(&self.storage_root);
        let text = serde_json::to_string_pretty(self)?;
        std::fs::write(path, text)
    }

    /// Son açılan incelemeler listesine bir kaydı ekler.
    pub fn push_recent(&mut self, storage_key: &str) {
        self.recent_investigations.retain(|k| k != storage_key);
        self.recent_investigations
            .insert(0, storage_key.to_string());
        self.recent_investigations.truncate(self.max_recent);
    }
}

/// Gerekli klasörleri oluşturur.
pub fn ensure_dirs(settings: &Settings) -> std::io::Result<()> {
    std::fs::create_dir_all(&settings.storage_root)?;
    std::fs::create_dir_all(
        settings
            .storage_root
            .join(izses_core::investigation::INVESTIGATIONS_DIR),
    )?;
    std::fs::create_dir_all(&settings.model_root)?;
    Ok(())
}

#[cfg(test)]
#[allow(clippy::field_reassign_with_default)]
mod tests {
    use super::*;

    #[test]
    fn settings_roundtrip() {
        let tmp = tempfile::tempdir().unwrap();
        let mut s = Settings::default();
        s.storage_root = tmp.path().to_path_buf();
        s.prefer_gpu = true;
        s.save().unwrap();

        let path = Settings::config_path(tmp.path());
        let text = std::fs::read_to_string(path).unwrap();
        let loaded: Settings = serde_json::from_str(&text).unwrap();
        assert!(loaded.prefer_gpu);
        assert_eq!(loaded.language, "tr");
    }

    #[test]
    fn recent_list_is_bounded_and_deduplicated() {
        let mut s = Settings::default();
        s.max_recent = 3;
        for k in ["a", "b", "c", "d"] {
            s.push_recent(k);
        }
        assert_eq!(s.recent_investigations, vec!["d", "c", "b"]);
        s.push_recent("c");
        assert_eq!(s.recent_investigations[0], "c");
        assert_eq!(s.recent_investigations.len(), 3);
    }

    #[test]
    fn ensure_dirs_creates_structure() {
        let tmp = tempfile::tempdir().unwrap();
        let mut s = Settings::default();
        s.storage_root = tmp.path().join("IZSES");
        s.model_root = tmp.path().join("modeller");
        ensure_dirs(&s).unwrap();
        assert!(s.storage_root.is_dir());
        assert!(s.storage_root.join("Incelemeler").is_dir());
        assert!(s.model_root.is_dir());
    }
}
