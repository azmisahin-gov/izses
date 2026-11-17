//! Delil kaynaklarının ve dosyalarının kalıcılığı.

use std::path::PathBuf;

use chrono::{DateTime, Utc};
use rusqlite::{params, Row};

use crate::device::DeviceKind;
use crate::error::{CoreError, CoreResult};
use crate::inventory::{AudioTechnicalInfo, EvidenceFile, FileCategory, VerificationState};
use crate::model::Id;
use crate::store::Store;
use crate::timefmt;
use crate::transcript::EvidenceSource;

fn parse_dt(s: Option<String>) -> Option<DateTime<Utc>> {
    s.and_then(|s| timefmt::parse_iso(&s))
}

fn source_from_row(row: &Row<'_>) -> rusqlite::Result<EvidenceSource> {
    let kind_str: String = row.get("kind")?;
    Ok(EvidenceSource {
        id: Id(row.get("id")?),
        investigation_id: Id(row.get("investigation_id")?),
        label: row.get("label")?,
        root_path: PathBuf::from(row.get::<_, String>("root_path")?),
        kind: parse_kind(&kind_str),
        filesystem: row.get("filesystem")?,
        total_bytes: row.get("total_bytes")?,
        ordinal: row.get("ordinal")?,
        added_at: parse_dt(row.get("added_at")?).unwrap_or_else(Utc::now),
        last_scan_at: parse_dt(row.get("last_scan_at")?),
        last_scan_count: row.get("last_scan_count")?,
    })
}

fn parse_kind(s: &str) -> DeviceKind {
    match s {
        "removable" => DeviceKind::Removable,
        "fixed" => DeviceKind::Fixed,
        "network" => DeviceKind::Network,
        "optical" => DeviceKind::Optical,
        _ => DeviceKind::Unknown,
    }
}

fn kind_str(k: DeviceKind) -> &'static str {
    match k {
        DeviceKind::Removable => "removable",
        DeviceKind::Fixed => "fixed",
        DeviceKind::Network => "network",
        DeviceKind::Optical => "optical",
        DeviceKind::Unknown => "unknown",
    }
}

fn category_str(c: FileCategory) -> &'static str {
    match c {
        FileCategory::Audio => "audio",
        FileCategory::Document => "document",
        FileCategory::Other => "other",
    }
}

fn parse_category(s: &str) -> FileCategory {
    match s {
        "audio" => FileCategory::Audio,
        "document" => FileCategory::Document,
        _ => FileCategory::Other,
    }
}

fn verification_str(v: VerificationState) -> &'static str {
    match v {
        VerificationState::NotAcquired => "not_acquired",
        VerificationState::InProgress => "in_progress",
        VerificationState::Verified => "verified",
        VerificationState::Mismatch => "mismatch",
        VerificationState::Failed => "failed",
    }
}

fn parse_verification(s: &str) -> VerificationState {
    match s {
        "in_progress" => VerificationState::InProgress,
        "verified" => VerificationState::Verified,
        "mismatch" => VerificationState::Mismatch,
        "failed" => VerificationState::Failed,
        _ => VerificationState::NotAcquired,
    }
}

fn file_from_row(row: &Row<'_>) -> rusqlite::Result<EvidenceFile> {
    let category: String = row.get("category")?;
    let verification: String = row.get("verification")?;
    let audio_json: Option<String> = row.get("audio")?;
    Ok(EvidenceFile {
        id: Id(row.get("id")?),
        investigation_id: Id(row.get("investigation_id")?),
        source_id: Id(row.get("source_id")?),
        relative_path: PathBuf::from(row.get::<_, String>("relative_path")?),
        local_relative_path: PathBuf::from(row.get::<_, String>("local_relative_path")?),
        size_bytes: row.get("size_bytes")?,
        extension: row.get("extension")?,
        category: parse_category(&category),
        source_modified: parse_dt(row.get("source_modified")?),
        source_created: parse_dt(row.get("source_created")?),
        acquired_at: parse_dt(row.get("acquired_at")?),
        source_sha256: row.get("source_sha256")?,
        copy_sha256: row.get("copy_sha256")?,
        verification: parse_verification(&verification),
        audio: audio_json.and_then(|j| serde_json::from_str(&j).ok()),
        issue: row.get("issue")?,
    })
}

impl Store {
    /// Yeni delil kaynağı ekler.
    pub fn insert_source(&self, src: &EvidenceSource) -> CoreResult<()> {
        self.conn().execute(
            r#"
            INSERT INTO evidence_sources (
                id, investigation_id, label, root_path, kind, filesystem,
                total_bytes, ordinal, added_at, last_scan_at, last_scan_count
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
            "#,
            params![
                src.id.0,
                src.investigation_id.0,
                src.label,
                src.root_path.to_string_lossy(),
                kind_str(src.kind),
                src.filesystem,
                src.total_bytes,
                src.ordinal,
                timefmt::to_iso(&src.added_at),
                src.last_scan_at.as_ref().map(timefmt::to_iso),
                src.last_scan_count,
            ],
        )?;
        Ok(())
    }

    pub fn list_sources(&self, investigation_id: &Id) -> CoreResult<Vec<EvidenceSource>> {
        let mut stmt = self.conn().prepare(
            "SELECT * FROM evidence_sources WHERE investigation_id = ?1 ORDER BY ordinal",
        )?;
        let rows = stmt.query_map(params![investigation_id.0], source_from_row)?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    pub fn get_source(&self, id: &Id) -> CoreResult<EvidenceSource> {
        self.conn()
            .query_row(
                "SELECT * FROM evidence_sources WHERE id = ?1",
                params![id.0],
                source_from_row,
            )
            .map_err(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => CoreError::not_found("delil kaynağı"),
                other => CoreError::Database(other),
            })
    }

    pub fn delete_source(&self, id: &Id) -> CoreResult<()> {
        let n = self
            .conn()
            .execute("DELETE FROM evidence_sources WHERE id = ?1", params![id.0])?;
        if n == 0 {
            return Err(CoreError::not_found("delil kaynağı"));
        }
        Ok(())
    }

    pub fn update_source_scan(
        &self,
        id: &Id,
        scan_at: DateTime<Utc>,
        count: u32,
    ) -> CoreResult<()> {
        self.conn().execute(
            "UPDATE evidence_sources SET last_scan_at = ?2, last_scan_count = ?3 WHERE id = ?1",
            params![id.0, timefmt::to_iso(&scan_at), count],
        )?;
        Ok(())
    }

    /// Bir kaynakta aynı göreli yolun zaten kayıtlı olup olmadığı.
    pub fn source_has_relative(&self, source_id: &Id, relative: &str) -> CoreResult<bool> {
        let count: i64 = self.conn().query_row(
            "SELECT COUNT(*) FROM evidence_files WHERE source_id = ?1 AND relative_path = ?2",
            params![source_id.0, relative],
            |r| r.get(0),
        )?;
        Ok(count > 0)
    }

    /// Yeni delil dosyası ekler.
    pub fn insert_file(&self, file: &EvidenceFile) -> CoreResult<()> {
        let audio = match &file.audio {
            Some(a) => Some(serde_json::to_string(a)?),
            None => None,
        };
        self.conn().execute(
            r#"
            INSERT INTO evidence_files (
                id, investigation_id, source_id, relative_path, local_relative_path,
                size_bytes, extension, category, source_modified, source_created,
                acquired_at, source_sha256, copy_sha256, verification, audio, issue
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)
            "#,
            params![
                file.id.0,
                file.investigation_id.0,
                file.source_id.0,
                file.relative_path.to_string_lossy(),
                file.local_relative_path.to_string_lossy(),
                file.size_bytes,
                file.extension,
                category_str(file.category),
                file.source_modified.as_ref().map(timefmt::to_iso),
                file.source_created.as_ref().map(timefmt::to_iso),
                file.acquired_at.as_ref().map(timefmt::to_iso),
                file.source_sha256,
                file.copy_sha256,
                verification_str(file.verification),
                audio,
                file.issue,
            ],
        )?;
        Ok(())
    }

    /// Bir dosyanın teknik bilgilerini ve doğrulama durumunu günceller.
    pub fn update_file_audio(&self, id: &Id, audio: &AudioTechnicalInfo) -> CoreResult<()> {
        let json = serde_json::to_string(audio)?;
        self.conn().execute(
            "UPDATE evidence_files SET audio = ?2 WHERE id = ?1",
            params![id.0, json],
        )?;
        Ok(())
    }

    /// Aktarım ve doğrulama bilgilerini günceller.
    pub fn update_file_verification(&self, file: &EvidenceFile) -> CoreResult<()> {
        let audio = match &file.audio {
            Some(a) => Some(serde_json::to_string(a)?),
            None => None,
        };
        let n = self.conn().execute(
            r#"
            UPDATE evidence_files SET
                local_relative_path = ?2, acquired_at = ?3, source_sha256 = ?4,
                copy_sha256 = ?5, verification = ?6, audio = ?7, issue = ?8
            WHERE id = ?1
            "#,
            params![
                file.id.0,
                file.local_relative_path.to_string_lossy(),
                file.acquired_at.as_ref().map(timefmt::to_iso),
                file.source_sha256,
                file.copy_sha256,
                verification_str(file.verification),
                audio,
                file.issue,
            ],
        )?;
        if n == 0 {
            return Err(CoreError::not_found("delil dosyası"));
        }
        Ok(())
    }

    pub fn get_file(&self, id: &Id) -> CoreResult<EvidenceFile> {
        self.conn()
            .query_row(
                "SELECT * FROM evidence_files WHERE id = ?1",
                params![id.0],
                file_from_row,
            )
            .map_err(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => CoreError::not_found("delil dosyası"),
                other => CoreError::Database(other),
            })
    }

    pub fn list_files(&self, investigation_id: &Id) -> CoreResult<Vec<EvidenceFile>> {
        let mut stmt = self.conn().prepare(
            "SELECT * FROM evidence_files WHERE investigation_id = ?1 ORDER BY relative_path",
        )?;
        let rows = stmt.query_map(params![investigation_id.0], file_from_row)?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    pub fn list_audio_files(&self, investigation_id: &Id) -> CoreResult<Vec<EvidenceFile>> {
        let mut stmt = self.conn().prepare(
            "SELECT * FROM evidence_files WHERE investigation_id = ?1 AND category = 'audio' ORDER BY relative_path",
        )?;
        let rows = stmt.query_map(params![investigation_id.0], file_from_row)?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    pub fn list_files_by_source(&self, source_id: &Id) -> CoreResult<Vec<EvidenceFile>> {
        let mut stmt = self.conn().prepare(
            "SELECT * FROM evidence_files WHERE source_id = ?1 ORDER BY relative_path",
        )?;
        let rows = stmt.query_map(params![source_id.0], file_from_row)?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    /// Yarım kalmış aktarımları (durumu `in_progress` olan dosyalar) bulur.
    ///
    /// Uygulama ani kapandığında bu kayıtlar kurtarma için kullanılır.
    pub fn find_incomplete_acquisitions(&self, investigation_id: &Id) -> CoreResult<Vec<EvidenceFile>> {
        let mut stmt = self.conn().prepare(
            "SELECT * FROM evidence_files WHERE investigation_id = ?1 AND verification = 'in_progress' ORDER BY relative_path",
        )?;
        let rows = stmt.query_map(params![investigation_id.0], file_from_row)?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    /// Doğrulanmamış dosyaları (aktarılmamış, hatalı veya uyuşmayan) bulur.
    pub fn find_unverified_files(&self, investigation_id: &Id) -> CoreResult<Vec<EvidenceFile>> {
        let mut stmt = self.conn().prepare(
            "SELECT * FROM evidence_files WHERE investigation_id = ?1 AND verification != 'verified' ORDER BY relative_path",
        )?;
        let rows = stmt.query_map(params![investigation_id.0], file_from_row)?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    pub fn delete_file(&self, id: &Id) -> CoreResult<()> {
        let n = self
            .conn()
            .execute("DELETE FROM evidence_files WHERE id = ?1", params![id.0])?;
        if n == 0 {
            return Err(CoreError::not_found("delil dosyası"));
        }
        Ok(())
    }

    /// Bir incelemedeki bir sonraki kaynak sıra numarası.
    pub fn next_source_ordinal(&self, investigation_id: &Id) -> CoreResult<u32> {
        let max: Option<u32> = self.conn().query_row(
            "SELECT MAX(ordinal) FROM evidence_sources WHERE investigation_id = ?1",
            params![investigation_id.0],
            |r| r.get(0),
        )?;
        Ok(max.unwrap_or(0) + 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Investigation;

    fn setup() -> (Store, Investigation) {
        let store = Store::open_in_memory().unwrap();
        let mut inv = Investigation::new();
        inv.case_number = "2026/125".into();
        store.insert_investigation(&inv).unwrap();
        (store, inv)
    }

    fn sample_file(inv: &Investigation, source: &EvidenceSource, rel: &str) -> EvidenceFile {
        EvidenceFile {
            id: Id::new(),
            investigation_id: inv.id.clone(),
            source_id: source.id.clone(),
            relative_path: PathBuf::from(rel),
            local_relative_path: PathBuf::from(format!("orijinal_dosyalar/{rel}")),
            size_bytes: 1024,
            extension: "wav".into(),
            category: FileCategory::Audio,
            source_modified: None,
            source_created: None,
            acquired_at: None,
            source_sha256: None,
            copy_sha256: None,
            verification: VerificationState::NotAcquired,
            audio: None,
            issue: None,
        }
    }

    #[test]
    fn insert_and_list_sources() {
        let (store, inv) = setup();
        let s1 = EvidenceSource::new(&inv.id, "USB", "/tmp/usb", DeviceKind::Removable, 1);
        let s2 = EvidenceSource::new(&inv.id, "SD", "/tmp/sd", DeviceKind::Removable, 2);
        store.insert_source(&s1).unwrap();
        store.insert_source(&s2).unwrap();
        let list = store.list_sources(&inv.id).unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].label, "USB");
        assert_eq!(store.next_source_ordinal(&inv.id).unwrap(), 3);
    }

    #[test]
    fn files_are_scoped_to_investigation() {
        let (store, inv) = setup();
        let src = EvidenceSource::new(&inv.id, "USB", "/tmp/usb", DeviceKind::Removable, 1);
        store.insert_source(&src).unwrap();
        store.insert_file(&sample_file(&inv, &src, "a.wav")).unwrap();

        let mut other = Investigation::new();
        other.case_number = "2026/999".into();
        store.insert_investigation(&other).unwrap();

        assert_eq!(store.list_files(&inv.id).unwrap().len(), 1);
        assert_eq!(store.list_files(&other.id).unwrap().len(), 0);
    }

    #[test]
    fn verification_state_persists() {
        let (store, inv) = setup();
        let src = EvidenceSource::new(&inv.id, "USB", "/tmp/usb", DeviceKind::Removable, 1);
        store.insert_source(&src).unwrap();
        let mut f = sample_file(&inv, &src, "a.wav");
        store.insert_file(&f).unwrap();

        f.verification = VerificationState::Verified;
        f.source_sha256 = Some("aa".into());
        f.copy_sha256 = Some("aa".into());
        f.acquired_at = Some(Utc::now());
        store.update_file_verification(&f).unwrap();

        let got = store.get_file(&f.id).unwrap();
        assert!(got.is_verified());
        assert_eq!(got.source_sha256.as_deref(), Some("aa"));
        assert!(store.find_unverified_files(&inv.id).unwrap().is_empty());
    }

    #[test]
    fn finds_incomplete_acquisitions() {
        let (store, inv) = setup();
        let src = EvidenceSource::new(&inv.id, "USB", "/tmp/usb", DeviceKind::Removable, 1);
        store.insert_source(&src).unwrap();
        let mut f = sample_file(&inv, &src, "a.wav");
        f.verification = VerificationState::InProgress;
        store.insert_file(&f).unwrap();
        let pending = store.find_incomplete_acquisitions(&inv.id).unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].id, f.id);
    }

    #[test]
    fn audio_info_roundtrips() {
        let (store, inv) = setup();
        let src = EvidenceSource::new(&inv.id, "USB", "/tmp/usb", DeviceKind::Removable, 1);
        store.insert_source(&src).unwrap();
        let f = sample_file(&inv, &src, "a.wav");
        store.insert_file(&f).unwrap();

        let info = AudioTechnicalInfo {
            container: Some("WAV".into()),
            codec: Some("pcm_s16le".into()),
            duration_ms: Some(12_345),
            channels: Some(2),
            sample_rate: Some(44_100),
            bit_depth: Some(16),
            bit_rate: Some(1_411_200),
            probe_error: None,
        };
        store.update_file_audio(&f.id, &info).unwrap();
        let got = store.get_file(&f.id).unwrap();
        let a = got.audio.unwrap();
        assert_eq!(a.duration_ms, Some(12_345));
        assert_eq!(a.channels, Some(2));
    }

    #[test]
    fn deleting_source_cascades_files() {
        let (store, inv) = setup();
        let src = EvidenceSource::new(&inv.id, "USB", "/tmp/usb", DeviceKind::Removable, 1);
        store.insert_source(&src).unwrap();
        store.insert_file(&sample_file(&inv, &src, "a.wav")).unwrap();
        store.delete_source(&src.id).unwrap();
        assert!(store.list_files(&inv.id).unwrap().is_empty());
    }

    #[test]
    fn duplicate_relative_detection() {
        let (store, inv) = setup();
        let src = EvidenceSource::new(&inv.id, "USB", "/tmp/usb", DeviceKind::Removable, 1);
        store.insert_source(&src).unwrap();
        assert!(!store.source_has_relative(&src.id, "a.wav").unwrap());
        store.insert_file(&sample_file(&inv, &src, "a.wav")).unwrap();
        assert!(store.source_has_relative(&src.id, "a.wav").unwrap());
    }
}
