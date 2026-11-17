//! Dosya envanteri: kaynak taraması, ses dosyası tanıma ve üst veri çıkarımı.
//!
//! Tarama salt okunurdur. Hiçbir dosya değiştirilmez, taşınmaz veya silinmez.

use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::error::{CoreError, CoreResult};
use crate::hash;
use crate::model::Id;

/// Desteklenen ses biçimleri (ilk sürüm).
pub const SUPPORTED_AUDIO_EXTENSIONS: &[&str] = &[
    "wav", "wave", "mp3", "m4a", "mp4", "aac", "flac", "ogg", "oga", "opus", "wma", "aiff", "aif",
    "caf", "amr", "3gp",
];

/// İnceleme için yararlı olabilecek belge biçimleri.
pub const DOCUMENT_EXTENSIONS: &[&str] = &[
    "pdf", "doc", "docx", "txt", "rtf", "odt", "xls", "xlsx", "csv", "jpg", "jpeg", "png", "tif",
    "tiff", "bmp", "heic",
];

/// Tarama sırasında bulunan bir dosyanın tanımı.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScannedFile {
    /// Dosyanın tam yolu (kaynak aygıtta).
    pub source_path: PathBuf,
    /// Kaynak köke göre göreli yol.
    pub relative_path: PathBuf,
    pub size_bytes: u64,
    /// Küçük harfli uzantı.
    pub extension: String,
    /// Kaynak dosya sisteminin bildirdiği değişiklik zamanı.
    pub modified: Option<DateTime<Utc>>,
    /// Kaynak dosya sisteminin bildirdiği oluşturma zamanı.
    pub created: Option<DateTime<Utc>>,
    /// Dosya türü sınıflandırması.
    pub category: FileCategory,
}

impl ScannedFile {
    pub fn file_name(&self) -> String {
        self.source_path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default()
    }
}

/// Dosya sınıfı.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileCategory {
    /// Desteklenen ses dosyası.
    Audio,
    /// İnceleme için yararlı belge/görsel.
    Document,
    /// İlgisiz veya desteklenmeyen dosya.
    Other,
}

impl FileCategory {
    pub fn label_tr(self) -> &'static str {
        match self {
            Self::Audio => "Ses dosyası",
            Self::Document => "Belge",
            Self::Other => "Diğer",
        }
    }
}

/// Dosya uzantısına göre sınıf belirler.
pub fn categorize(path: &Path) -> (String, FileCategory) {
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    let category = if SUPPORTED_AUDIO_EXTENSIONS.contains(&ext.as_str()) {
        FileCategory::Audio
    } else if DOCUMENT_EXTENSIONS.contains(&ext.as_str()) {
        FileCategory::Document
    } else {
        FileCategory::Other
    };
    (ext, category)
}

/// Tarama ayarları.
#[derive(Debug, Clone)]
pub struct ScanOptions {
    /// Alt klasörlere in.
    pub recursive: bool,
    /// Ses dosyalarını listele.
    pub include_audio: bool,
    /// Belge ve görselleri listele.
    pub include_documents: bool,
    /// Diğer dosyaları da listele.
    pub include_other: bool,
    /// En fazla taranacak dosya sayısı (kontrolsüz taramayı önler).
    pub max_files: usize,
}

impl Default for ScanOptions {
    fn default() -> Self {
        Self {
            recursive: true,
            include_audio: true,
            include_documents: true,
            include_other: false,
            max_files: 500_000,
        }
    }
}

/// Bir kaynak kökü tarar ve dosya listesini döndürür.
///
/// Erişilemeyen alt klasörler sessizce atlanır; tarama tümden başarısız olmaz.
pub fn scan_source(root: &Path, opts: &ScanOptions) -> CoreResult<Vec<ScannedFile>> {
    if !root.is_dir() {
        return Err(CoreError::invalid("kaynak bir klasör değil"));
    }
    let mut out = Vec::new();
    let walker = if opts.recursive {
        walkdir::WalkDir::new(root).follow_links(false)
    } else {
        walkdir::WalkDir::new(root)
            .max_depth(1)
            .follow_links(false)
    };

    for entry in walker.into_iter().filter_map(Result::ok) {
        if out.len() >= opts.max_files {
            break;
        }
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path();
        let (ext, category) = categorize(path);
        let include = match category {
            FileCategory::Audio => opts.include_audio,
            FileCategory::Document => opts.include_documents,
            FileCategory::Other => opts.include_other,
        };
        if !include {
            continue;
        }
        let meta = match entry.metadata() {
            Ok(m) => m,
            Err(_) => continue,
        };
        let relative = path.strip_prefix(root).unwrap_or(path).to_path_buf();
        out.push(ScannedFile {
            source_path: path.to_path_buf(),
            relative_path: relative,
            size_bytes: meta.len(),
            extension: ext,
            modified: meta.modified().ok().map(DateTime::<Utc>::from),
            created: meta.created().ok().map(DateTime::<Utc>::from),
            category,
        });
    }

    out.sort_by(|a, b| a.relative_path.cmp(&b.relative_path));
    Ok(out)
}

/// Kaynaktan gelen bir dosyanın incelemeye alınmış kaydı.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceFile {
    pub id: Id,
    pub investigation_id: Id,
    pub source_id: Id,
    /// Kaynak köke göre göreli yol (orijinal ad ve yapı korunur).
    pub relative_path: PathBuf,
    /// İnceleme içindeki yerel kopyanın yolu (depolama anahtarına göreli).
    pub local_relative_path: PathBuf,
    pub size_bytes: u64,
    pub extension: String,
    pub category: FileCategory,
    /// Kaynak dosya sisteminin bildirdiği zaman bilgileri.
    pub source_modified: Option<DateTime<Utc>>,
    pub source_created: Option<DateTime<Utc>>,
    /// Yerel kopyanın oluşturulma zamanı.
    pub acquired_at: Option<DateTime<Utc>>,
    /// Kaynaktan okunan baytların SHA-256 değeri.
    pub source_sha256: Option<String>,
    /// Yerel kopyanın SHA-256 değeri.
    pub copy_sha256: Option<String>,
    /// Aktarım doğrulama durumu.
    pub verification: VerificationState,
    /// Dosyanın teknik ses bilgileri (ses dosyaları için).
    pub audio: Option<AudioTechnicalInfo>,
    /// Dosya hasarlı veya desteklenmiyorsa açıklama.
    pub issue: Option<String>,
}

impl EvidenceFile {
    pub fn file_name(&self) -> String {
        self.relative_path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default()
    }

    pub fn is_audio(&self) -> bool {
        self.category == FileCategory::Audio
    }

    /// Aktarımın doğrulanmış olup olmadığı.
    pub fn is_verified(&self) -> bool {
        self.verification == VerificationState::Verified
    }
}

/// Aktarım doğrulama durumu.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VerificationState {
    /// Henüz aktarılmadı.
    NotAcquired,
    /// Aktarım sürüyor.
    InProgress,
    /// Kaynak ve kopya özetleri eşleşti.
    Verified,
    /// Özetler eşleşmedi.
    Mismatch,
    /// Aktarım tamamlanmadı veya hata oluştu.
    Failed,
}

impl VerificationState {
    pub fn label_tr(self) -> &'static str {
        match self {
            Self::NotAcquired => "Aktarılmadı",
            Self::InProgress => "Aktarılıyor",
            Self::Verified => "Doğrulandı",
            Self::Mismatch => "Uyuşmazlık",
            Self::Failed => "Başarısız",
        }
    }
}

/// Ses dosyasının teknik bilgileri.
///
/// Değerler yalnızca dosyadan gerçekten okunabildiğinde doldurulur;
/// tahmin yoluyla doldurulmaz.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AudioTechnicalInfo {
    /// Kapsayıcı biçim (ör. WAV, MP3).
    pub container: Option<String>,
    /// Kodek adı (ör. pcm_s16le, mp3, aac).
    pub codec: Option<String>,
    /// Süre (milisaniye).
    pub duration_ms: Option<u64>,
    /// Kanal sayısı.
    pub channels: Option<u32>,
    /// Örnekleme frekansı (Hz).
    pub sample_rate: Option<u32>,
    /// Bit derinliği, belirlenebiliyorsa.
    pub bit_depth: Option<u32>,
    /// Bit hızı (bit/sn), belirlenebiliyorsa.
    pub bit_rate: Option<u32>,
    /// Dosya incelenirken oluşan hata (hasarlı dosya vb.).
    pub probe_error: Option<String>,
}

impl AudioTechnicalInfo {
    pub fn duration_label(&self) -> String {
        match self.duration_ms {
            Some(ms) => crate::timefmt::format_ms(ms),
            None => "—".to_string(),
        }
    }
}

/// Kaynak kökü doğrular ve taranabilir olduğunu bildirir.
pub fn validate_source(root: &Path) -> CoreResult<()> {
    let meta = std::fs::metadata(root)?;
    if !meta.is_dir() {
        return Err(CoreError::invalid("seçilen kaynak bir klasör değil"));
    }
    std::fs::read_dir(root)?;
    Ok(())
}

/// Bir dosyanın SHA-256 özetini hesaplar (envanter ve doğrulama için).
pub fn file_digest(path: &Path) -> CoreResult<String> {
    hash::sha256_file(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn categorizes_known_extensions() {
        assert_eq!(categorize(Path::new("a.wav")).1, FileCategory::Audio);
        assert_eq!(categorize(Path::new("a.MP3")).1, FileCategory::Audio);
        assert_eq!(categorize(Path::new("a.flac")).1, FileCategory::Audio);
        assert_eq!(categorize(Path::new("a.pdf")).1, FileCategory::Document);
        assert_eq!(categorize(Path::new("a.bin")).1, FileCategory::Other);
        assert_eq!(categorize(Path::new("noext")).1, FileCategory::Other);
    }

    #[test]
    fn scans_only_requested_categories() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("a.wav"), b"x").unwrap();
        std::fs::write(tmp.path().join("b.pdf"), b"x").unwrap();
        std::fs::write(tmp.path().join("c.bin"), b"x").unwrap();

        let opts = ScanOptions::default();
        let files = scan_source(tmp.path(), &opts).unwrap();
        assert_eq!(files.len(), 2);
        assert!(files.iter().all(|f| f.category != FileCategory::Other));

        let mut audio_only = ScanOptions::default();
        audio_only.include_documents = false;
        let files = scan_source(tmp.path(), &audio_only).unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].extension, "wav");
    }

    #[test]
    fn scan_preserves_relative_structure() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join("SES_KAYITLARI")).unwrap();
        std::fs::write(tmp.path().join("SES_KAYITLARI/gorusme_01.wav"), b"x").unwrap();

        let files = scan_source(tmp.path(), &ScanOptions::default()).unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(
            files[0].relative_path,
            PathBuf::from("SES_KAYITLARI/gorusme_01.wav")
        );
    }

    #[test]
    fn scan_rejects_missing_source() {
        let tmp = tempfile::tempdir().unwrap();
        let missing = tmp.path().join("yok");
        assert!(scan_source(&missing, &ScanOptions::default()).is_err());
    }

    #[test]
    fn scan_is_non_recursive_when_requested() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join("alt")).unwrap();
        std::fs::write(tmp.path().join("kok.wav"), b"x").unwrap();
        std::fs::write(tmp.path().join("alt/derin.wav"), b"x").unwrap();

        let mut opts = ScanOptions::default();
        opts.recursive = false;
        let files = scan_source(tmp.path(), &opts).unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].file_name(), "kok.wav");
    }
}
