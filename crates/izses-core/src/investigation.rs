//! İnceleme yerel depolama düzeni.
//!
//! Her inceleme kendi klasöründe saklanır ve farklı incelemelerin verileri
//! hiçbir zaman karışmaz. Klasör adları, Windows ve diğer dosya sistemlerinde
//! sorun çıkarmaması için ASCII güvenli tutulur.

use std::path::{Path, PathBuf};

use crate::error::{CoreError, CoreResult};

/// Kök altındaki incelemeler klasörü adı.
pub const INVESTIGATIONS_DIR: &str = "Incelemeler";

/// Bir incelemenin alt klasörleri.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubDir {
    /// İnceleme üst verisi ve veritabanı dosyaları.
    Meta,
    /// Delil kaynaklarının tanımı.
    EvidenceSources,
    /// İncelemeye alınan orijinal dosyalar (değiştirilmez).
    OriginalFiles,
    /// Türetilmiş çalışma kopyaları ve ara çıktılar.
    WorkingOutputs,
    /// Otomatik deşifre sonuçları.
    AutoTranscripts,
    /// Bilirkişi düzeltmeleri ve revizyonları.
    ExpertCorrections,
    /// Notlar ve bulgular.
    Notes,
    /// İşlem geçmişi (audit) dışa aktarımları.
    History,
    /// Üretilen raporlar.
    Reports,
}

impl SubDir {
    pub fn name(self) -> &'static str {
        match self {
            Self::Meta => "inceleme_verileri",
            Self::EvidenceSources => "delil_kaynaklari",
            Self::OriginalFiles => "orijinal_dosyalar",
            Self::WorkingOutputs => "calisma_ciktilari",
            Self::AutoTranscripts => "otomatik_desifreler",
            Self::ExpertCorrections => "bilirkisi_duzeltmeleri",
            Self::Notes => "notlar",
            Self::History => "islem_gecmisi",
            Self::Reports => "raporlar",
        }
    }

    pub const ALL: [SubDir; 9] = [
        SubDir::Meta,
        SubDir::EvidenceSources,
        SubDir::OriginalFiles,
        SubDir::WorkingOutputs,
        SubDir::AutoTranscripts,
        SubDir::ExpertCorrections,
        SubDir::Notes,
        SubDir::History,
        SubDir::Reports,
    ];
}

/// Kök depolama alanı (varsayılan: kullanıcı belgeleri altında `IZSES`).
#[derive(Debug, Clone)]
pub struct StorageRoot {
    root: PathBuf,
}

impl StorageRoot {
    /// Yeni bir kök tanımlar; klasörleri gerekiyorsa oluşturur.
    pub fn new(root: impl Into<PathBuf>) -> CoreResult<Self> {
        let root = root.into();
        let s = Self { root };
        s.ensure_dirs()?;
        Ok(s)
    }

    pub fn path(&self) -> &Path {
        &self.root
    }

    pub fn investigations_dir(&self) -> PathBuf {
        self.root.join(INVESTIGATIONS_DIR)
    }

    /// Ana veritabanı yolu.
    pub fn database_path(&self) -> PathBuf {
        self.root.join("izses.db")
    }

    /// İnceleme klasörü.
    pub fn investigation_dir(&self, storage_key: &str) -> PathBuf {
        self.investigations_dir().join(storage_key)
    }

    pub fn sub_dir(&self, storage_key: &str, sub: SubDir) -> PathBuf {
        self.investigation_dir(storage_key).join(sub.name())
    }

    fn ensure_dirs(&self) -> CoreResult<()> {
        std::fs::create_dir_all(self.investigations_dir())?;
        Ok(())
    }

    /// Bir incelemenin tüm klasör ağacını oluşturur.
    pub fn create_investigation_layout(&self, storage_key: &str) -> CoreResult<()> {
        let dir = self.investigation_dir(storage_key);
        std::fs::create_dir_all(&dir)?;
        for sub in SubDir::ALL {
            std::fs::create_dir_all(dir.join(sub.name()))?;
        }
        restrict_permissions(&dir)?;
        Ok(())
    }

    /// Bir inceleme klasörünün var olup olmadığını bildirir.
    pub fn investigation_exists(&self, storage_key: &str) -> bool {
        self.investigation_dir(storage_key).is_dir()
    }

    /// Mevcut inceleme klasörlerini listeler.
    pub fn list_investigation_keys(&self) -> CoreResult<Vec<String>> {
        let dir = self.investigations_dir();
        if !dir.exists() {
            return Ok(Vec::new());
        }
        let mut keys = Vec::new();
        for entry in std::fs::read_dir(&dir)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                if let Some(name) = entry.file_name().to_str() {
                    keys.push(name.to_string());
                }
            }
        }
        keys.sort();
        Ok(keys)
    }

    /// İnceleme klasörünü ve içeriğini kaldırır.
    ///
    /// Kullanıcı onayı arayüz katmanında alınmalıdır.
    pub fn remove_investigation(&self, storage_key: &str) -> CoreResult<()> {
        let dir = self.investigation_dir(storage_key);
        if !dir.exists() {
            return Err(CoreError::not_found(format!(
                "inceleme klasörü: {storage_key}"
            )));
        }
        std::fs::remove_dir_all(&dir)?;
        Ok(())
    }
}

/// Klasör izinlerini yalnızca sahibine açacak biçimde kısıtlar (Unix).
///
/// Windows'ta NTFS erişim denetimi işletim sistemi varsayılanlarına ve
/// kurulum adımlarına bırakılır; bu işlev orada sessizce başarılı olur.
#[cfg(unix)]
pub fn restrict_permissions(path: &Path) -> CoreResult<()> {
    use std::os::unix::fs::PermissionsExt;
    let perms = std::fs::Permissions::from_mode(0o700);
    std::fs::set_permissions(path, perms)?;
    Ok(())
}

#[cfg(not(unix))]
pub fn restrict_permissions(_path: &Path) -> CoreResult<()> {
    Ok(())
}

/// Bir kaynak dosyanın inceleme içindeki göreli hedef yolunu üretir.
///
/// Kaynak dosyanın adı ve göreli klasör yapısı korunur. Yol bileşenleri
/// temizlenir; `..` gibi üst dizine çıkan bileşenler kabul edilmez.
pub fn safe_relative_path(rel: &Path) -> CoreResult<PathBuf> {
    let mut out = PathBuf::new();
    for comp in rel.components() {
        use std::path::Component;
        match comp {
            Component::Normal(part) => out.push(part),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(CoreError::invalid(
                    "göreli yol güvenli değil (üst dizin veya kök içeriyor)",
                ));
            }
        }
    }
    if out.as_os_str().is_empty() {
        return Err(CoreError::invalid("boş göreli yol"));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_creates_all_subdirs() {
        let tmp = tempfile::tempdir().unwrap();
        let root = StorageRoot::new(tmp.path()).unwrap();
        root.create_investigation_layout("2026-125").unwrap();
        for sub in SubDir::ALL {
            assert!(root.sub_dir("2026-125", sub).is_dir(), "{:?}", sub);
        }
        assert!(root.investigation_exists("2026-125"));
    }

    #[test]
    fn separate_investigations_do_not_mix() {
        let tmp = tempfile::tempdir().unwrap();
        let root = StorageRoot::new(tmp.path()).unwrap();
        root.create_investigation_layout("a").unwrap();
        root.create_investigation_layout("b").unwrap();
        let mut keys = root.list_investigation_keys().unwrap();
        keys.sort();
        assert_eq!(keys, vec!["a".to_string(), "b".to_string()]);
    }

    #[test]
    fn relative_path_rejects_traversal() {
        assert!(safe_relative_path(Path::new("../evil.wav")).is_err());
        assert!(safe_relative_path(Path::new("ses/a.wav")).is_ok());
        assert_eq!(
            safe_relative_path(Path::new("ses/./a.wav")).unwrap(),
            PathBuf::from("ses/a.wav")
        );
    }

    #[test]
    fn remove_investigation_deletes_tree() {
        let tmp = tempfile::tempdir().unwrap();
        let root = StorageRoot::new(tmp.path()).unwrap();
        root.create_investigation_layout("x").unwrap();
        root.remove_investigation("x").unwrap();
        assert!(!root.investigation_exists("x"));
        assert!(root.remove_investigation("x").is_err());
    }
}
