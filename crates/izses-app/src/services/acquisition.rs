//! Delil edinimi servisi.
//!
//! Kaynak aygıttan dosya düzeyinde, salt okunur edinim yapar:
//! 1. Kaynak dosyayı salt okunur açar.
//! 2. Kaynaktan okunan baytların SHA-256 değerini hesaplar.
//! 3. Dosyayı incelemenin yerel delil klasörüne kopyalar.
//! 4. Kopyanın SHA-256 değerini hesaplar.
//! 5. İki değeri karşılaştırır ve sonucu kaydeder.
//!
//! Kaynak dosyaların üzerine asla yazılmaz. Kopyalama yarım kalırsa hedef
//! dosya `.kismi` uzantısıyla işaretlenir ve doğrulanmış sayılmaz.

use std::io::{BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};

use chrono::Utc;
use izses_core::audit::{ActionKind, AuditEntry};
use izses_core::error::{CoreError, CoreResult};
use izses_core::inventory::{EvidenceFile, VerificationState};
use izses_core::investigation::{safe_relative_path, StorageRoot};
use izses_core::model::Id;
use izses_core::store::Store;
use sha2::{Digest, Sha256};

/// Yarım kalan kopyaların uzantısı.
pub const PARTIAL_SUFFIX: &str = ".kismi";

/// Arabellek boyutu (1 MiB): büyük dosyalarda sabit bellek kullanımı.
const CHUNK: usize = 1024 * 1024;

/// Tek bir dosyanın edinim sonucu.
#[derive(Debug, Clone)]
pub struct AcquisitionOutcome {
    pub file_id: Id,
    pub file_name: String,
    pub verified: bool,
    pub source_sha256: String,
    pub copy_sha256: String,
    pub bytes: u64,
    pub error: Option<String>,
}

/// Edinim ilerleme bildirimi.
#[derive(Debug, Clone)]
pub struct AcquisitionProgress {
    pub index: usize,
    pub total: usize,
    pub current: String,
}

/// Kaynak dosyayı incelemeye aktarır ve bütünlüğünü doğrular.
///
/// `local_relative`, dosyanın inceleme içindeki göreli hedef yoludur
/// (kaynak adı ve klasör yapısı korunur).
pub fn acquire_file(
    root: &StorageRoot,
    store: &Store,
    investigation_id: &Id,
    storage_key: &str,
    source_id: &Id,
    source_path: &Path,
    local_relative: &Path,
) -> CoreResult<AcquisitionOutcome> {
    let safe_rel = safe_relative_path(local_relative)?;
    let dest_dir = root.sub_dir(storage_key, izses_core::investigation::SubDir::OriginalFiles);
    let dest_path = dest_dir.join(&safe_rel);

    if let Some(parent) = dest_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let file_name = source_path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();

    // Disk alanı denetimi: kopyalama başlamadan önce yer olduğundan emin ol.
    let size = std::fs::metadata(source_path)?.len();
    check_free_space(&dest_dir, size)?;

    // Kaynak dosyayı salt okunur aç.
    let source_file = std::fs::File::open(source_path)?;
    let mut reader = BufReader::with_capacity(CHUNK, source_file);

    // Geçici hedef: kopyalama yarıda kalırsa tamamlanmış gibi görünmez.
    let temp_path = dest_path.with_extension(format!(
        "{}kismi",
        dest_path
            .extension()
            .map(|e| format!("{}.", e.to_string_lossy()))
            .unwrap_or_default()
    ));

    let source_digest = {
        let temp_file = std::fs::File::create(&temp_path)?;
        let mut writer = BufWriter::with_capacity(CHUNK, temp_file);
        let mut hasher = Sha256::new();
        let mut buf = vec![0u8; CHUNK];
        let mut written: u64 = 0;

        loop {
            let n = reader.read(&mut buf)?;
            if n == 0 {
                break;
            }
            hasher.update(&buf[..n]);
            writer.write_all(&buf[..n])?;
            written += n as u64;
        }
        writer.flush()?;
        writer
            .into_inner()
            .map_err(|e| CoreError::Io(e.into_error()))?
            .sync_all()?;

        if written != size {
            // Kaynak dosya okuma sırasında değişmiş veya erişim kesilmiş.
            let _ = std::fs::remove_file(&temp_path);
            return Err(CoreError::Integrity(format!(
                "{file_name}: okunan bayt sayısı beklenenle uyuşmuyor"
            )));
        }
        hex::encode(hasher.finalize())
    };

    // Kopyanın özetini hesapla.
    let copy_digest = izses_core::hash::sha256_file(&temp_path)?;

    let verified = source_digest == copy_digest;
    if verified {
        std::fs::rename(&temp_path, &dest_path)?;
    } else {
        // Uyuşmayan kopya yerinde bırakılmaz; doğrulanmamış olarak işaretlenir.
        let _ = std::fs::remove_file(&temp_path);
    }

    let now = Utc::now();
    let mut file = EvidenceFile {
        id: Id::new(),
        investigation_id: investigation_id.clone(),
        source_id: source_id.clone(),
        relative_path: safe_rel.clone(),
        local_relative_path: PathBuf::from(izses_core::investigation::SubDir::OriginalFiles.name())
            .join(&safe_rel),
        size_bytes: size,
        extension: source_path
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase())
            .unwrap_or_default(),
        category: izses_core::inventory::categorize(source_path).1,
        source_modified: std::fs::metadata(source_path)
            .ok()
            .and_then(|m| m.modified().ok())
            .map(chrono::DateTime::<Utc>::from),
        source_created: std::fs::metadata(source_path)
            .ok()
            .and_then(|m| m.created().ok())
            .map(chrono::DateTime::<Utc>::from),
        acquired_at: Some(now),
        source_sha256: Some(source_digest.clone()),
        copy_sha256: Some(copy_digest.clone()),
        verification: if verified {
            VerificationState::Verified
        } else {
            VerificationState::Mismatch
        },
        audio: None,
        issue: None,
    };

    // Teknik envanteri hemen çıkar (ses dosyaları için).
    if file.is_audio() {
        let info = izses_audio::probe(&dest_path);
        file.issue = info.probe_error.clone();
        file.audio = Some(info);
    }

    store.insert_file(&file)?;

    store.insert_audit(&AuditEntry::new(
        investigation_id,
        if verified {
            ActionKind::HashVerified
        } else {
            ActionKind::HashMismatch
        },
        format!(
            "{}: kaynak ve kopya SHA-256 {}",
            file_name,
            if verified { "eşleşti" } else { "eşleşmedi" }
        ),
    ))?;

    Ok(AcquisitionOutcome {
        file_id: file.id,
        file_name,
        verified,
        source_sha256: source_digest,
        copy_sha256: copy_digest,
        bytes: size,
        error: if verified {
            None
        } else {
            Some("SHA-256 uyuşmazlığı: aktarım doğrulanmadı".to_string())
        },
    })
}

/// Hedef klasörde yeterli boş alan olup olmadığını denetler.
///
/// Platform desteklemiyorsa denetim atlanır; kopyalama yine de güvenlidir
/// çünkü hata durumunda kısmi dosya korunmaz.
fn check_free_space(dir: &Path, needed: u64) -> CoreResult<()> {
    // Kopyalama sırasında küçük bir pay bırak.
    let required = needed.saturating_add(16 * 1024 * 1024);
    if let Some(available) = free_space(dir) {
        if available < required {
            return Err(CoreError::Storage(format!(
                "yetersiz boş alan: gereken {}, mevcut {}",
                izses_core::device::format_size(required),
                izses_core::device::format_size(available)
            )));
        }
    }
    Ok(())
}

#[cfg(unix)]
fn free_space(dir: &Path) -> Option<u64> {
    // statvfs olmadan güvenilir bir yol yok; libc bağımlılığı eklememek için
    // denetim atlanır. Kopyalama hatası yine de güvenli biçimde ele alınır.
    let _ = dir;
    None
}

#[cfg(windows)]
fn free_space(dir: &Path) -> Option<u64> {
    use std::os::windows::ffi::OsStrExt;
    let wide: Vec<u16> = dir
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let mut free: u64 = 0;
    let ok = unsafe {
        windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW(
            wide.as_ptr(),
            &mut free as *mut u64,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };
    if ok == 0 {
        None
    } else {
        Some(free)
    }
}

#[cfg(not(any(unix, windows)))]
fn free_space(dir: &Path) -> Option<u64> {
    let _ = dir;
    None
}

/// Yarım kalmış kopyaları temizler.
///
/// Uygulama ani kapandığında kalan `.kismi` dosyaları silinir; kayıtları
/// `InProgress` durumunda bırakılmaz.
pub fn clean_partial_files(root: &StorageRoot, storage_key: &str) -> CoreResult<usize> {
    let dir = root.sub_dir(storage_key, izses_core::investigation::SubDir::OriginalFiles);
    if !dir.is_dir() {
        return Ok(0);
    }
    let mut removed = 0;
    for entry in walkdir::WalkDir::new(&dir).into_iter().filter_map(Result::ok) {
        let path = entry.path();
        if path.is_file() {
            let name = path.to_string_lossy();
            if name.ends_with(PARTIAL_SUFFIX) || name.ends_with(".kismi") {
                if std::fs::remove_file(path).is_ok() {
                    removed += 1;
                }
            }
        }
    }
    Ok(removed)
}

/// Yarım kalmış aktarım kayıtlarını bulur ve temizler.
///
/// Bu işlev uygulama açılışında çağrılır. `InProgress` durumundaki kayıtlar
/// `Failed` olarak işaretlenir; böylece başarılı aktarım gibi görünmezler.
pub fn recover_incomplete_acquisitions(
    store: &Store,
    investigation_id: &Id,
) -> CoreResult<usize> {
    let pending = store.find_incomplete_acquisitions(investigation_id)?;
    let mut count = 0;
    for mut file in pending {
        file.verification = VerificationState::Failed;
        file.issue = Some("Aktarım tamamlanmadı; dosya yeniden alınmalı.".to_string());
        store.update_file_verification(&file)?;
        count += 1;
    }
    if count > 0 {
        store.insert_audit(&AuditEntry::new(
            investigation_id,
            ActionKind::AcquisitionFailed,
            format!("{count} yarım kalan aktarım tespit edildi ve geçersiz sayıldı"),
        ))?;
    }
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use izses_core::model::Investigation;
    use izses_core::transcript::EvidenceSource;

    fn setup() -> (tempfile::TempDir, StorageRoot, Store, Id, Id) {
        let tmp = tempfile::tempdir().unwrap();
        let root = StorageRoot::new(tmp.path()).unwrap();
        let store = Store::open_in_memory().unwrap();
        let mut inv = Investigation::new();
        inv.case_number = "2026/125".into();
        store.insert_investigation(&inv).unwrap();
        root.create_investigation_layout(&inv.storage_key()).unwrap();
        let src = EvidenceSource::new(
            &inv.id,
            "USB",
            tmp.path().join("kaynak"),
            izses_core::device::DeviceKind::Removable,
            1,
        );
        store.insert_source(&src).unwrap();
        (tmp, root, store, inv.id, src.id)
    }

    #[test]
    fn acquires_and_verifies_file() {
        let (tmp, root, store, inv_id, src_id) = setup();
        let source_dir = tmp.path().join("kaynak");
        std::fs::create_dir_all(&source_dir).unwrap();
        let src_file = source_dir.join("gorusme_01.wav");
        let data = vec![0x41u8; 100_000];
        std::fs::write(&src_file, &data).unwrap();

        let outcome = acquire_file(
            &root,
            &store,
            &inv_id,
            "2026-125",
            &src_id,
            &src_file,
            Path::new("gorusme_01.wav"),
        )
        .unwrap();

        assert!(outcome.verified);
        assert_eq!(outcome.source_sha256, outcome.copy_sha256);
        assert_eq!(outcome.source_sha256, izses_core::hash::sha256_bytes(&data));

        let files = store.list_files(&inv_id).unwrap();
        assert_eq!(files.len(), 1);
        assert!(files[0].is_verified());

        // Yerel kopya gerçekten var ve içeriği aynı.
        let local = root
            .sub_dir("2026-125", izses_core::investigation::SubDir::OriginalFiles)
            .join("gorusme_01.wav");
        assert!(local.is_file());
        assert_eq!(std::fs::read(&local).unwrap(), data);
    }

    #[test]
    fn source_file_is_never_modified() {
        let (tmp, root, store, inv_id, src_id) = setup();
        let source_dir = tmp.path().join("kaynak");
        std::fs::create_dir_all(&source_dir).unwrap();
        let src_file = source_dir.join("a.wav");
        let data = vec![7u8; 5000];
        std::fs::write(&src_file, &data).unwrap();
        let before = std::fs::metadata(&src_file).unwrap().modified().unwrap();

        acquire_file(
            &root,
            &store,
            &inv_id,
            "2026-125",
            &src_id,
            &src_file,
            Path::new("a.wav"),
        )
        .unwrap();

        // Kaynak dosya değişmemeli.
        assert_eq!(std::fs::read(&src_file).unwrap(), data);
        let after = std::fs::metadata(&src_file).unwrap().modified().unwrap();
        assert_eq!(before, after, "kaynak dosyanın değişiklik zamanı değişti");
    }

    #[test]
    fn preserves_relative_folder_structure() {
        let (tmp, root, store, inv_id, src_id) = setup();
        let source_dir = tmp.path().join("kaynak");
        std::fs::create_dir_all(source_dir.join("SES_KAYITLARI")).unwrap();
        let src_file = source_dir.join("SES_KAYITLARI/gorusme_02.mp3");
        std::fs::write(&src_file, b"fake mp3 data").unwrap();

        acquire_file(
            &root,
            &store,
            &inv_id,
            "2026-125",
            &src_id,
            &src_file,
            Path::new("SES_KAYITLARI/gorusme_02.mp3"),
        )
        .unwrap();

        let local = root
            .sub_dir("2026-125", izses_core::investigation::SubDir::OriginalFiles)
            .join("SES_KAYITLARI/gorusme_02.mp3");
        assert!(local.is_file(), "klasör yapısı korunmadı");
    }

    #[test]
    fn rejects_unsafe_relative_path() {
        let (tmp, root, store, inv_id, src_id) = setup();
        let src_file = tmp.path().join("kaynak/a.wav");
        std::fs::create_dir_all(src_file.parent().unwrap()).unwrap();
        std::fs::write(&src_file, b"x").unwrap();

        let err = acquire_file(
            &root,
            &store,
            &inv_id,
            "2026-125",
            &src_id,
            &src_file,
            Path::new("../kacis.wav"),
        )
        .unwrap_err();
        assert!(matches!(err, CoreError::Invalid(_)));
    }

    #[test]
    fn missing_source_file_reports_error() {
        let (tmp, root, store, inv_id, src_id) = setup();
        let missing = tmp.path().join("kaynak/yok.wav");
        assert!(acquire_file(
            &root,
            &store,
            &inv_id,
            "2026-125",
            &src_id,
            &missing,
            Path::new("yok.wav"),
        )
        .is_err());
    }

    #[test]
    fn recovery_marks_pending_as_failed() {
        let (_tmp, _root, store, inv_id, src_id) = setup();
        // Elle yarım kalmış kayıt oluştur.
        let mut file = EvidenceFile {
            id: Id::new(),
            investigation_id: inv_id.clone(),
            source_id: src_id.clone(),
            relative_path: PathBuf::from("a.wav"),
            local_relative_path: PathBuf::from("orijinal_dosyalar/a.wav"),
            size_bytes: 10,
            extension: "wav".into(),
            category: izses_core::inventory::FileCategory::Audio,
            source_modified: None,
            source_created: None,
            acquired_at: None,
            source_sha256: None,
            copy_sha256: None,
            verification: VerificationState::InProgress,
            audio: None,
            issue: None,
        };
        store.insert_file(&file).unwrap();

        let count = recover_incomplete_acquisitions(&store, &inv_id).unwrap();
        assert_eq!(count, 1);

        file = store.get_file(&file.id).unwrap();
        assert_eq!(file.verification, VerificationState::Failed);
        assert!(file.issue.is_some());
    }

    #[test]
    fn clean_partial_files_removes_kismi() {
        let (tmp, root, _store, _inv_id, _src_id) = setup();
        let dir = root.sub_dir("2026-125", izses_core::investigation::SubDir::OriginalFiles);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a.wav.kismi"), b"x").unwrap();
        std::fs::write(dir.join("b.wav"), b"y").unwrap();
        let _ = tmp;

        let removed = clean_partial_files(&root, "2026-125").unwrap();
        assert_eq!(removed, 1);
        assert!(dir.join("b.wav").is_file());
    }
}
