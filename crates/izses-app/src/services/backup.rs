//! Yedekleme ve geri yükleme servisi.
//!
//! Bir incelemenin tüm verisi (veritabanı kayıtları + dosyalar) tek bir ZIP
//! arşivine alınır. Geri yükleme, arşivi yeni bir inceleme klasörüne açar.
//!
//! Yedekleme veritabanı kilitlenmeden yapılabilmesi için önce SQLite'ın
//! tutarlı bir kopyası çıkarılır, sonra dosyalar eklenir.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use izses_core::audit::{ActionKind, AuditEntry};
use izses_core::error::{CoreError, CoreResult};
use izses_core::investigation::{SubDir, StorageRoot};
use izses_core::model::Id;
use izses_core::store::Store;

/// Yedek arşivindeki üst veri dosyası.
const MANIFEST: &str = "yedek_bilgisi.json";

/// Yedek üst verisi.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct BackupManifest {
    investigation_id: String,
    storage_key: String,
    case_number: String,
    created_at: String,
    app_version: String,
    /// Arşivdeki dosya sayısı.
    file_count: usize,
}

/// Bir incelemeyi ZIP arşivine yedekler.
pub fn backup_investigation(
    store: &Store,
    root: &StorageRoot,
    investigation_id: &Id,
    dest: &Path,
) -> CoreResult<PathBuf> {
    let investigation = store.get_investigation(investigation_id)?;
    let storage_key = investigation.storage_key();
    let inv_dir = root.investigation_dir(&storage_key);
    if !inv_dir.is_dir() {
        return Err(CoreError::not_found("inceleme klasörü"));
    }

    // Veritabanı kayıtlarını JSON olarak dışa aktar (taşınabilir yedek).
    let export = export_investigation_data(store, investigation_id)?;
    let export_json = serde_json::to_string_pretty(&export)?;

    let file = std::fs::File::create(dest)?;
    let mut zip = zip::ZipWriter::new(file);
    let options: zip::write::FileOptions<'_, ()> =
        zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    let mut file_count = 0usize;
    for entry in walkdir::WalkDir::new(&inv_dir).into_iter().filter_map(Result::ok) {
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path();
        let rel = match path.strip_prefix(&inv_dir) {
            Ok(r) => r,
            Err(_) => continue,
        };
        let name = format!("dosyalar/{}", rel.to_string_lossy().replace('\\', "/"));
        zip.start_file(name, options)
            .map_err(|e| CoreError::Io(std::io::Error::other(e.to_string())))?;
        let mut f = std::fs::File::open(path)?;
        let mut buf = Vec::new();
        f.read_to_end(&mut buf)?;
        zip.write_all(&buf)?;
        file_count += 1;
    }

    // Veri dışa aktarımı.
    zip.start_file("veritabani.json", options)
        .map_err(|e| CoreError::Io(std::io::Error::other(e.to_string())))?;
    zip.write_all(export_json.as_bytes())?;

    // Üst veri.
    let manifest = BackupManifest {
        investigation_id: investigation.id.0.clone(),
        storage_key: storage_key.clone(),
        case_number: investigation.case_number.clone(),
        created_at: chrono::Utc::now().to_rfc3339(),
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        file_count,
    };
    zip.start_file(MANIFEST, options)
        .map_err(|e| CoreError::Io(std::io::Error::other(e.to_string())))?;
    zip.write_all(serde_json::to_string_pretty(&manifest)?.as_bytes())?;

    zip.finish()
        .map_err(|e| CoreError::Io(std::io::Error::other(e.to_string())))?;

    store.insert_audit(&AuditEntry::new(
        investigation_id,
        ActionKind::BackupCreated,
        format!("Yedek oluşturuldu: {file_count} dosya"),
    ))?;

    Ok(dest.to_path_buf())
}

/// Bir yedeği yeni bir inceleme klasörüne geri yükler.
///
/// Depolama anahtarı çakışırsa sonuna sayı eklenir. Geri yüklenen inceleme
/// yeni bir kimlik alır; böylece mevcut inceleme üzerine yazılmaz.
pub fn restore_backup(
    store: &Store,
    root: &StorageRoot,
    archive: &Path,
) -> CoreResult<Id> {
    let file = std::fs::File::open(archive)?;
    let mut zip = zip::ZipArchive::new(file)
        .map_err(|e| CoreError::invalid(format!("yedek arşivi açılamadı: {e}")))?;

    // Üst veriyi oku.
    let manifest: BackupManifest = {
        let mut m = zip
            .by_name(MANIFEST)
            .map_err(|_| CoreError::invalid("yedek dosyası geçersiz: üst veri yok"))?;
        let mut text = String::new();
        m.read_to_string(&mut text)?;
        serde_json::from_str(&text)?
    };

    // Veri dışa aktarımını oku.
    let export: InvestigationExport = {
        let mut m = zip
            .by_name("veritabani.json")
            .map_err(|_| CoreError::invalid("yedek dosyası geçersiz: veri yok"))?;
        let mut text = String::new();
        m.read_to_string(&mut text)?;
        serde_json::from_str(&text)?
    };

    // Yeni depolama anahtarı.
    let mut key = manifest.storage_key.clone();
    let mut suffix = 2;
    while store.storage_key_taken(&key)? || root.investigation_exists(&key) {
        key = format!("{}-{suffix}", manifest.storage_key);
        suffix += 1;
        if suffix > 1000 {
            return Err(CoreError::invalid("benzersiz depolama anahtarı üretilemedi"));
        }
    }

    // Kayıtları yeni kimliklerle içe aktar.
    let new_id = import_investigation_data(store, &export, &key)?;
    root.create_investigation_layout(&key)?;

    // Dosyaları çıkar.
    let inv_dir = root.investigation_dir(&key);
    for i in 0..zip.len() {
        let mut entry = zip
            .by_index(i)
            .map_err(|e| CoreError::invalid(format!("arşiv girdisi okunamadı: {e}")))?;
        let name = entry.name().to_string();
        let Some(rel) = name.strip_prefix("dosyalar/") else {
            continue;
        };
        if rel.is_empty() {
            continue;
        }
        // Güvenlik: arşiv yolları üst dizine çıkamaz.
        let rel_path = std::path::Path::new(rel);
        let safe = izses_core::investigation::safe_relative_path(rel_path)?;
        let dest = inv_dir.join(&safe);
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut out = std::fs::File::create(&dest)?;
        std::io::copy(&mut entry, &mut out)?;
        out.sync_all()?;
    }

    store.insert_audit(&AuditEntry::new(
        &new_id,
        ActionKind::BackupRestored,
        format!("Yedekten geri yüklendi ({} dosya)", manifest.file_count),
    ))?;

    Ok(new_id)
}

/// Bir incelemenin veritabanı kayıtlarının taşınabilir dışa aktarımı.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct InvestigationExport {
    pub investigation: izses_core::model::Investigation,
    pub sources: Vec<izses_core::transcript::EvidenceSource>,
    pub files: Vec<izses_core::inventory::EvidenceFile>,
    pub transcripts: Vec<izses_core::transcript::Transcript>,
    pub segments: Vec<izses_core::transcript::TranscriptSegment>,
    pub notes: Vec<izses_core::transcript::Note>,
    pub audit: Vec<AuditEntry>,
}

/// İnceleme verisini dışa aktarır.
pub fn export_investigation_data(
    store: &Store,
    investigation_id: &Id,
) -> CoreResult<InvestigationExport> {
    let investigation = store.get_investigation(investigation_id)?;
    let sources = store.list_sources(investigation_id)?;
    let files = store.list_files(investigation_id)?;
    let transcripts = store.list_transcripts(investigation_id)?;
    let mut segments = Vec::new();
    for t in &transcripts {
        segments.extend(store.list_segments(&t.id)?);
    }
    let notes = store.list_notes(investigation_id)?;
    let audit = store.list_audit(investigation_id)?;

    Ok(InvestigationExport {
        investigation,
        sources,
        files,
        transcripts,
        segments,
        notes,
        audit,
    })
}

/// Dışa aktarılmış veriyi veritabanına yeni kimliklerle yazar.
fn import_investigation_data(
    store: &Store,
    export: &InvestigationExport,
    new_storage_key: &str,
) -> CoreResult<Id> {
    let mut inv = export.investigation.clone();
    inv.id = Id::new();
    // Depolama anahtarını yeni klasörle eşleştir; dosya numarasını koru.
    inv.assign_storage_key(new_storage_key.to_string());
    let new_id = inv.id.clone();
    store.insert_investigation(&inv)?;

    let mut source_map: std::collections::HashMap<String, Id> = std::collections::HashMap::new();
    for src in &export.sources {
        let mut s = src.clone();
        s.id = Id::new();
        s.investigation_id = new_id.clone();
        source_map.insert(src.id.0.clone(), s.id.clone());
        store.insert_source(&s)?;
    }

    let mut file_map: std::collections::HashMap<String, Id> = std::collections::HashMap::new();
    for f in &export.files {
        let mut nf = f.clone();
        nf.id = Id::new();
        nf.investigation_id = new_id.clone();
        if let Some(sid) = source_map.get(&f.source_id.0) {
            nf.source_id = sid.clone();
        }
        file_map.insert(f.id.0.clone(), nf.id.clone());
        store.insert_file(&nf)?;
    }

    let mut transcript_map: std::collections::HashMap<String, Id> =
        std::collections::HashMap::new();
    for t in &export.transcripts {
        let mut nt = t.clone();
        nt.id = Id::new();
        nt.investigation_id = new_id.clone();
        if let Some(fid) = file_map.get(&t.evidence_file_id.0) {
            nt.evidence_file_id = fid.clone();
        }
        transcript_map.insert(t.id.0.clone(), nt.id.clone());
        store.upsert_transcript(&nt)?;
    }

    for seg in &export.segments {
        let mut ns = seg.clone();
        ns.id = Id::new();
        if let Some(tid) = transcript_map.get(&seg.transcript_id.0) {
            ns.transcript_id = tid.clone();
        }
        store.insert_segment(&ns)?;
    }

    for note in &export.notes {
        let mut nn = note.clone();
        nn.id = Id::new();
        nn.investigation_id = new_id.clone();
        if let Some(fid) = nn
            .evidence_file_id
            .as_ref()
            .and_then(|i| file_map.get(&i.0))
        {
            nn.evidence_file_id = Some(fid.clone());
        }
        store.insert_note(&nn)?;
    }

    Ok(new_id)
}

/// İnceleme klasörünün toplam boyutunu hesaplar.
pub fn investigation_size(root: &StorageRoot, storage_key: &str) -> u64 {
    let dir = root.investigation_dir(storage_key);
    walkdir::WalkDir::new(&dir)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file())
        .filter_map(|e| e.metadata().ok())
        .map(|m| m.len())
        .sum()
}

/// Tüm verinin yedeklenmesi gereken alt klasörler.
pub fn backed_up_subdirs() -> Vec<SubDir> {
    SubDir::ALL.to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;
    use izses_core::inventory::{EvidenceFile, FileCategory, VerificationState};
    use izses_core::model::Investigation;
    use izses_core::transcript::{EvidenceSource, Note, NoteKind, Transcript, TranscriptSegment};
    use std::path::PathBuf;

    fn setup() -> (tempfile::TempDir, StorageRoot, Store, Id, String) {
        let tmp = tempfile::tempdir().unwrap();
        let root = StorageRoot::new(tmp.path()).unwrap();
        let store = Store::open_in_memory().unwrap();
        let mut inv = Investigation::new();
        inv.case_number = "2026/125".into();
        inv.subject = "Ses kaydı".into();
        store.insert_investigation(&inv).unwrap();
        let key = inv.storage_key();
        root.create_investigation_layout(&key).unwrap();

        // Gerçek bir dosya koy.
        let dir = root.sub_dir(&key, SubDir::OriginalFiles);
        std::fs::write(dir.join("a.wav"), b"ses verisi").unwrap();

        let src = EvidenceSource::new(
            &inv.id,
            "USB",
            "/tmp/usb",
            izses_core::device::DeviceKind::Removable,
            1,
        );
        store.insert_source(&src).unwrap();
        let file = EvidenceFile {
            id: Id::new(),
            investigation_id: inv.id.clone(),
            source_id: src.id.clone(),
            relative_path: PathBuf::from("a.wav"),
            local_relative_path: PathBuf::from("orijinal_dosyalar/a.wav"),
            size_bytes: 10,
            extension: "wav".into(),
            category: FileCategory::Audio,
            source_modified: None,
            source_created: None,
            acquired_at: None,
            source_sha256: Some("aa".into()),
            copy_sha256: Some("aa".into()),
            verification: VerificationState::Verified,
            audio: None,
            issue: None,
        };
        store.insert_file(&file).unwrap();

        let t = Transcript::new(&inv.id, &file.id);
        let t = store.upsert_transcript(&t).unwrap();
        store
            .insert_segment(&TranscriptSegment::new(&t.id, 0, 0, 1000, "merhaba".into()))
            .unwrap();
        store
            .insert_note(&Note::new(&inv.id, NoteKind::General, "N", "B"))
            .unwrap();

        (tmp, root, store, inv.id, key)
    }

    #[test]
    fn backup_and_restore_roundtrip() {
        let (tmp, root, store, inv_id, _key) = setup();
        let archive = tmp.path().join("yedek.zip");
        backup_investigation(&store, &root, &inv_id, &archive).unwrap();
        assert!(archive.is_file());

        let new_id = restore_backup(&store, &root, &archive).unwrap();
        assert_ne!(new_id, inv_id);

        let restored = store.get_investigation(&new_id).unwrap();
        assert_eq!(restored.case_number, "2026/125");

        // Dosyalar, deşifreler ve notlar geri yüklenmiş olmalı.
        let files = store.list_files(&new_id).unwrap();
        assert_eq!(files.len(), 1);
        let transcripts = store.list_transcripts(&new_id).unwrap();
        assert_eq!(transcripts.len(), 1);
        let segments = store.list_segments(&transcripts[0].id).unwrap();
        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0].auto_text, "merhaba");
        let notes = store.list_notes(&new_id).unwrap();
        assert_eq!(notes.len(), 1);

        // Dosya içeriği gerçekten geri yüklenmeli.
        let key2 = restored.storage_key();
        let restored_file = root
            .sub_dir(&key2, SubDir::OriginalFiles)
            .join("a.wav");
        assert!(restored_file.is_file());
        assert_eq!(std::fs::read(&restored_file).unwrap(), b"ses verisi");
    }

    #[test]
    fn restore_does_not_overwrite_existing() {
        let (tmp, root, store, inv_id, key) = setup();
        let archive = tmp.path().join("yedek.zip");
        backup_investigation(&store, &root, &inv_id, &archive).unwrap();

        let new_id = restore_backup(&store, &root, &archive).unwrap();
        let restored = store.get_investigation(&new_id).unwrap();
        // Orijinal inceleme hâlâ yerinde.
        assert!(root.investigation_exists(&key));
        assert_ne!(restored.storage_key(), key);
    }

    #[test]
    fn invalid_archive_reports_error() {
        let (tmp, root, store, _inv_id, _key) = setup();
        let bad = tmp.path().join("bozuk.zip");
        std::fs::write(&bad, b"bu bir zip degil").unwrap();
        assert!(restore_backup(&store, &root, &bad).is_err());
    }

    #[test]
    fn export_contains_all_entities() {
        let (_tmp, _root, store, inv_id, _key) = setup();
        let export = export_investigation_data(&store, &inv_id).unwrap();
        assert_eq!(export.sources.len(), 1);
        assert_eq!(export.files.len(), 1);
        assert_eq!(export.transcripts.len(), 1);
        assert_eq!(export.segments.len(), 1);
        assert_eq!(export.notes.len(), 1);
    }

    #[test]
    fn size_is_computed() {
        let (_tmp, root, _store, _inv_id, key) = setup();
        let size = investigation_size(&root, &key);
        assert!(size >= 10, "boyut: {size}");
    }
}
