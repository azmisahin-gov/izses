//! İşlem geçmişinin (audit log) kalıcılığı.

use chrono::{DateTime, Utc};
use rusqlite::{params, Row};

use crate::audit::{ActionKind, AuditEntry};
use crate::error::CoreResult;
use crate::model::Id;
use crate::store::Store;
use crate::timefmt;

fn parse_dt(s: Option<String>) -> Option<DateTime<Utc>> {
    s.and_then(|s| timefmt::parse_iso(&s))
}

fn kind_from_str(s: &str) -> ActionKind {
    match s {
        "investigation_created" => ActionKind::InvestigationCreated,
        "investigation_updated" => ActionKind::InvestigationUpdated,
        "investigation_opened" => ActionKind::InvestigationOpened,
        "evidence_source_added" => ActionKind::EvidenceSourceAdded,
        "evidence_source_removed" => ActionKind::EvidenceSourceRemoved,
        "scan_started" => ActionKind::ScanStarted,
        "scan_completed" => ActionKind::ScanCompleted,
        "acquisition_started" => ActionKind::AcquisitionStarted,
        "acquisition_completed" => ActionKind::AcquisitionCompleted,
        "acquisition_failed" => ActionKind::AcquisitionFailed,
        "hash_verified" => ActionKind::HashVerified,
        "hash_mismatch" => ActionKind::HashMismatch,
        "transcript_queued" => ActionKind::TranscriptQueued,
        "transcript_started" => ActionKind::TranscriptStarted,
        "transcript_segment_completed" => ActionKind::TranscriptSegmentCompleted,
        "transcript_completed" => ActionKind::TranscriptCompleted,
        "transcript_failed" => ActionKind::TranscriptFailed,
        "transcript_resumed" => ActionKind::TranscriptResumed,
        "segment_edited" => ActionKind::SegmentEdited,
        "segment_split" => ActionKind::SegmentSplit,
        "segment_merged" => ActionKind::SegmentMerged,
        "segment_deleted" => ActionKind::SegmentDeleted,
        "segment_added" => ActionKind::SegmentAdded,
        "speaker_labeled" => ActionKind::SpeakerLabeled,
        "note_added" => ActionKind::NoteAdded,
        "note_updated" => ActionKind::NoteUpdated,
        "derived_audio_created" => ActionKind::DerivedAudioCreated,
        "report_drafted" => ActionKind::ReportDrafted,
        "report_exported" => ActionKind::ReportExported,
        "backup_created" => ActionKind::BackupCreated,
        "backup_restored" => ActionKind::BackupRestored,
        _ => ActionKind::Other,
    }
}

fn entry_from_row(row: &Row<'_>) -> rusqlite::Result<AuditEntry> {
    let kind: String = row.get("kind")?;
    let details: Option<String> = row.get("details")?;
    Ok(AuditEntry {
        id: Id(row.get("id")?),
        investigation_id: Id(row.get("investigation_id")?),
        at: parse_dt(row.get("at")?).unwrap_or_else(Utc::now),
        kind: kind_from_str(&kind),
        summary: row.get("summary")?,
        details: details.and_then(|d| serde_json::from_str(&d).ok()),
    })
}

impl Store {
    /// Bir işlem kaydını yazar.
    pub fn insert_audit(&self, entry: &AuditEntry) -> CoreResult<()> {
        let details = match &entry.details {
            Some(d) => Some(serde_json::to_string(d)?),
            None => None,
        };
        self.conn().execute(
            r#"
            INSERT INTO audit_log (id, investigation_id, at, kind, summary, details)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            "#,
            params![
                entry.id.0,
                entry.investigation_id.0,
                timefmt::to_iso(&entry.at),
                entry.kind.as_str(),
                entry.summary,
                details,
            ],
        )?;
        Ok(())
    }

    /// Bir incelemenin işlem geçmişini zaman sırasına göre döndürür.
    pub fn list_audit(&self, investigation_id: &Id) -> CoreResult<Vec<AuditEntry>> {
        let mut stmt = self
            .conn()
            .prepare("SELECT * FROM audit_log WHERE investigation_id = ?1 ORDER BY at, rowid")?;
        let rows = stmt.query_map(params![investigation_id.0], entry_from_row)?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    /// Bir incelemenin işlem geçmişini ters sırada (en yeni önce) döndürür.
    pub fn list_audit_recent(
        &self,
        investigation_id: &Id,
        limit: u32,
    ) -> CoreResult<Vec<AuditEntry>> {
        let mut stmt = self.conn().prepare(
            "SELECT * FROM audit_log WHERE investigation_id = ?1 ORDER BY at DESC, rowid DESC LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![investigation_id.0, limit], entry_from_row)?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    /// İşlem geçmişini metin biçiminde dışa aktarır.
    pub fn export_audit_text(&self, investigation_id: &Id) -> CoreResult<String> {
        let entries = self.list_audit(investigation_id)?;
        let mut out = String::new();
        for e in entries {
            out.push_str(&format!(
                "{}  {:<32}  {}\n",
                timefmt::format_datetime(&e.at),
                e.kind.label_tr(),
                e.summary
            ));
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Investigation;

    #[test]
    fn audit_entries_are_ordered_and_scoped() {
        let store = Store::open_in_memory().unwrap();
        let mut inv = Investigation::new();
        inv.case_number = "A".into();
        store.insert_investigation(&inv).unwrap();

        let mut other = Investigation::new();
        other.case_number = "B".into();
        store.insert_investigation(&other).unwrap();

        store
            .insert_audit(&AuditEntry::new(
                &inv.id,
                ActionKind::InvestigationCreated,
                "oluşturuldu",
            ))
            .unwrap();
        store
            .insert_audit(&AuditEntry::new(
                &inv.id,
                ActionKind::EvidenceSourceAdded,
                "kaynak eklendi",
            ))
            .unwrap();
        store
            .insert_audit(&AuditEntry::new(
                &other.id,
                ActionKind::InvestigationCreated,
                "diğer",
            ))
            .unwrap();

        let entries = store.list_audit(&inv.id).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].kind, ActionKind::InvestigationCreated);
        assert_eq!(entries[1].kind, ActionKind::EvidenceSourceAdded);

        let recent = store.list_audit_recent(&inv.id, 1).unwrap();
        assert_eq!(recent[0].kind, ActionKind::EvidenceSourceAdded);
    }

    #[test]
    fn audit_details_roundtrip() {
        let store = Store::open_in_memory().unwrap();
        let inv = Investigation::new();
        store.insert_investigation(&inv).unwrap();
        let entry = AuditEntry::new(&inv.id, ActionKind::HashVerified, "doğrulandı")
            .with_details(serde_json::json!({"dosya": "a.wav", "eslesme": true}));
        store.insert_audit(&entry).unwrap();
        let got = store.list_audit(&inv.id).unwrap();
        assert_eq!(got[0].details.as_ref().unwrap()["eslesme"], true);
    }

    #[test]
    fn export_produces_text() {
        let store = Store::open_in_memory().unwrap();
        let inv = Investigation::new();
        store.insert_investigation(&inv).unwrap();
        store
            .insert_audit(&AuditEntry::new(&inv.id, ActionKind::Other, "test"))
            .unwrap();
        let text = store.export_audit_text(&inv.id).unwrap();
        assert!(text.contains("Diğer işlem"));
    }
}
