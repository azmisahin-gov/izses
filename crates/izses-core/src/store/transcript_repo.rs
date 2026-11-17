//! Deşifre, metin bölümleri ve revizyon geçmişinin kalıcılığı.

use chrono::{DateTime, Utc};
use rusqlite::{params, Row};

use crate::error::{CoreError, CoreResult};
use crate::model::Id;
use crate::store::Store;
use crate::timefmt;
use crate::transcript::{SegmentRevision, Transcript, TranscriptSegment, TranscriptStatus};

fn parse_dt(s: Option<String>) -> Option<DateTime<Utc>> {
    s.and_then(|s| timefmt::parse_iso(&s))
}

fn transcript_from_row(row: &Row<'_>) -> rusqlite::Result<Transcript> {
    let status: String = row.get("status")?;
    let settings: String = row.get("settings")?;
    Ok(Transcript {
        id: Id(row.get("id")?),
        investigation_id: Id(row.get("investigation_id")?),
        evidence_file_id: Id(row.get("evidence_file_id")?),
        model_name: row.get("model_name")?,
        model_revision: row.get("model_revision")?,
        model_sha256: row.get("model_sha256")?,
        language: row.get("language")?,
        task: row.get("task")?,
        settings: serde_json::from_str(&settings).unwrap_or(serde_json::json!({})),
        status: TranscriptStatus::from_str_opt(&status).unwrap_or(TranscriptStatus::Queued),
        completed_segments: row.get("completed_segments")?,
        total_segments: row.get("total_segments")?,
        error: row.get("error")?,
        created_at: parse_dt(row.get("created_at")?).unwrap_or_else(Utc::now),
        updated_at: parse_dt(row.get("updated_at")?).unwrap_or_else(Utc::now),
    })
}

fn segment_from_row(row: &Row<'_>) -> rusqlite::Result<TranscriptSegment> {
    let flags: String = row.get("flags")?;
    Ok(TranscriptSegment {
        id: Id(row.get("id")?),
        transcript_id: Id(row.get("transcript_id")?),
        index: row.get("idx")?,
        start_ms: row.get("start_ms")?,
        end_ms: row.get("end_ms")?,
        auto_text: row.get("auto_text")?,
        edited_text: row.get("edited_text")?,
        speaker: row.get("speaker")?,
        flags: serde_json::from_str(&flags).unwrap_or_default(),
        observation: row.get("observation")?,
        avg_logprob: row.get("avg_logprob")?,
        no_speech_prob: row.get("no_speech_prob")?,
        updated_at: parse_dt(row.get("updated_at")?).unwrap_or_else(Utc::now),
    })
}

impl Store {
    /// Bir delil dosyası için deşifre kaydı oluşturur.
    ///
    /// Aynı dosya için ikinci kez çağrılırsa mevcut kayıt döndürülür.
    pub fn upsert_transcript(&self, t: &Transcript) -> CoreResult<Transcript> {
        if let Ok(existing) = self.get_transcript_for_file(&t.evidence_file_id) {
            return Ok(existing);
        }
        let settings = serde_json::to_string(&t.settings)?;
        self.conn().execute(
            r#"
            INSERT INTO transcripts (
                id, investigation_id, evidence_file_id, model_name, model_revision,
                model_sha256, language, task, settings, status, completed_segments,
                total_segments, error, created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)
            "#,
            params![
                t.id.0,
                t.investigation_id.0,
                t.evidence_file_id.0,
                t.model_name,
                t.model_revision,
                t.model_sha256,
                t.language,
                t.task,
                settings,
                t.status.as_str(),
                t.completed_segments,
                t.total_segments,
                t.error,
                timefmt::to_iso(&t.created_at),
                timefmt::to_iso(&t.updated_at),
            ],
        )?;
        Ok(t.clone())
    }

    pub fn update_transcript(&self, t: &mut Transcript) -> CoreResult<()> {
        t.updated_at = Utc::now();
        let settings = serde_json::to_string(&t.settings)?;
        let n = self.conn().execute(
            r#"
            UPDATE transcripts SET
                model_name = ?2, model_revision = ?3, model_sha256 = ?4,
                language = ?5, task = ?6, settings = ?7, status = ?8,
                completed_segments = ?9, total_segments = ?10, error = ?11,
                updated_at = ?12
            WHERE id = ?1
            "#,
            params![
                t.id.0,
                t.model_name,
                t.model_revision,
                t.model_sha256,
                t.language,
                t.task,
                settings,
                t.status.as_str(),
                t.completed_segments,
                t.total_segments,
                t.error,
                timefmt::to_iso(&t.updated_at),
            ],
        )?;
        if n == 0 {
            return Err(CoreError::not_found("deşifre"));
        }
        Ok(())
    }

    pub fn get_transcript(&self, id: &Id) -> CoreResult<Transcript> {
        self.conn()
            .query_row(
                "SELECT * FROM transcripts WHERE id = ?1",
                params![id.0],
                transcript_from_row,
            )
            .map_err(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => CoreError::not_found("deşifre"),
                other => CoreError::Database(other),
            })
    }

    pub fn get_transcript_for_file(&self, evidence_file_id: &Id) -> CoreResult<Transcript> {
        self.conn()
            .query_row(
                "SELECT * FROM transcripts WHERE evidence_file_id = ?1",
                params![evidence_file_id.0],
                transcript_from_row,
            )
            .map_err(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => CoreError::not_found("deşifre"),
                other => CoreError::Database(other),
            })
    }

    pub fn list_transcripts(&self, investigation_id: &Id) -> CoreResult<Vec<Transcript>> {
        let mut stmt = self.conn().prepare(
            "SELECT * FROM transcripts WHERE investigation_id = ?1 ORDER BY created_at",
        )?;
        let rows = stmt.query_map(params![investigation_id.0], transcript_from_row)?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    /// Kuyruğa alınmış veya yarım kalmış deşifreleri bulur.
    ///
    /// Uygulama yeniden açıldığında bu işler kaldığı yerden sürdürülür.
    pub fn find_resumable_transcripts(&self, investigation_id: &Id) -> CoreResult<Vec<Transcript>> {
        let mut stmt = self.conn().prepare(
            "SELECT * FROM transcripts WHERE investigation_id = ?1 AND status IN ('queued','running','failed') ORDER BY created_at",
        )?;
        let rows = stmt.query_map(params![investigation_id.0], transcript_from_row)?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    pub fn delete_transcript(&self, id: &Id) -> CoreResult<()> {
        let n = self
            .conn()
            .execute("DELETE FROM transcripts WHERE id = ?1", params![id.0])?;
        if n == 0 {
            return Err(CoreError::not_found("deşifre"));
        }
        Ok(())
    }

    /// Bir metin bölümü ekler.
    pub fn insert_segment(&self, seg: &TranscriptSegment) -> CoreResult<()> {
        let flags = serde_json::to_string(&seg.flags)?;
        self.conn().execute(
            r#"
            INSERT INTO transcript_segments (
                id, transcript_id, idx, start_ms, end_ms, auto_text, edited_text,
                speaker, flags, observation, avg_logprob, no_speech_prob, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
            "#,
            params![
                seg.id.0,
                seg.transcript_id.0,
                seg.index,
                seg.start_ms,
                seg.end_ms,
                seg.auto_text,
                seg.edited_text,
                seg.speaker,
                flags,
                seg.observation,
                seg.avg_logprob,
                seg.no_speech_prob,
                timefmt::to_iso(&seg.updated_at),
            ],
        )?;
        Ok(())
    }

    /// Birden çok metin bölümünü tek işlemde ekler.
    pub fn insert_segments(&self, segs: &[TranscriptSegment]) -> CoreResult<()> {
        let tx = self.conn().unchecked_transaction()?;
        for seg in segs {
            let flags = serde_json::to_string(&seg.flags)?;
            tx.execute(
                r#"
                INSERT OR REPLACE INTO transcript_segments (
                    id, transcript_id, idx, start_ms, end_ms, auto_text, edited_text,
                    speaker, flags, observation, avg_logprob, no_speech_prob, updated_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
                "#,
                params![
                    seg.id.0,
                    seg.transcript_id.0,
                    seg.index,
                    seg.start_ms,
                    seg.end_ms,
                    seg.auto_text,
                    seg.edited_text,
                    seg.speaker,
                    flags,
                    seg.observation,
                    seg.avg_logprob,
                    seg.no_speech_prob,
                    timefmt::to_iso(&seg.updated_at),
                ],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Bir bölümün bilirkişi düzenlemelerini günceller.
    pub fn update_segment(&self, seg: &mut TranscriptSegment) -> CoreResult<()> {
        seg.updated_at = Utc::now();
        let flags = serde_json::to_string(&seg.flags)?;
        let n = self.conn().execute(
            r#"
            UPDATE transcript_segments SET
                idx = ?2, start_ms = ?3, end_ms = ?4, edited_text = ?5,
                speaker = ?6, flags = ?7, observation = ?8, updated_at = ?9
            WHERE id = ?1
            "#,
            params![
                seg.id.0,
                seg.index,
                seg.start_ms,
                seg.end_ms,
                seg.edited_text,
                seg.speaker,
                flags,
                seg.observation,
                timefmt::to_iso(&seg.updated_at),
            ],
        )?;
        if n == 0 {
            return Err(CoreError::not_found("metin bölümü"));
        }
        Ok(())
    }

    /// Bölümlerin sıra numaralarını yeniden atar (ekleme/silme sonrası).
    pub fn reindex_segments(&self, transcript_id: &Id) -> CoreResult<()> {
        let mut segs = self.list_segments(transcript_id)?;
        segs.sort_by_key(|s| (s.start_ms, s.index));
        let tx = self.conn().unchecked_transaction()?;
        for (i, seg) in segs.iter().enumerate() {
            tx.execute(
                "UPDATE transcript_segments SET idx = ?2 WHERE id = ?1",
                params![seg.id.0, i as u32],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn get_segment(&self, id: &Id) -> CoreResult<TranscriptSegment> {
        self.conn()
            .query_row(
                "SELECT * FROM transcript_segments WHERE id = ?1",
                params![id.0],
                segment_from_row,
            )
            .map_err(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => CoreError::not_found("metin bölümü"),
                other => CoreError::Database(other),
            })
    }

    pub fn list_segments(&self, transcript_id: &Id) -> CoreResult<Vec<TranscriptSegment>> {
        let mut stmt = self.conn().prepare(
            "SELECT * FROM transcript_segments WHERE transcript_id = ?1 ORDER BY idx",
        )?;
        let rows = stmt.query_map(params![transcript_id.0], segment_from_row)?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    pub fn delete_segment(&self, id: &Id) -> CoreResult<()> {
        let n = self
            .conn()
            .execute("DELETE FROM transcript_segments WHERE id = ?1", params![id.0])?;
        if n == 0 {
            return Err(CoreError::not_found("metin bölümü"));
        }
        Ok(())
    }

    /// Bir deşifredeki bölüm sayısı.
    pub fn count_segments(&self, transcript_id: &Id) -> CoreResult<u32> {
        let count: u32 = self.conn().query_row(
            "SELECT COUNT(*) FROM transcript_segments WHERE transcript_id = ?1",
            params![transcript_id.0],
            |r| r.get(0),
        )?;
        Ok(count)
    }

    /// Bir bölümün metin değişiklik kaydını ekler.
    pub fn insert_revision(&self, rev: &SegmentRevision) -> CoreResult<()> {
        self.conn().execute(
            r#"
            INSERT INTO segment_revisions (
                id, segment_id, transcript_id, at, before_text, after_text, author, note
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
            "#,
            params![
                rev.id.0,
                rev.segment_id.0,
                rev.transcript_id.0,
                timefmt::to_iso(&rev.at),
                rev.before_text,
                rev.after_text,
                rev.author,
                rev.note,
            ],
        )?;
        Ok(())
    }

    pub fn list_revisions(&self, transcript_id: &Id) -> CoreResult<Vec<SegmentRevision>> {
        let mut stmt = self.conn().prepare(
            "SELECT * FROM segment_revisions WHERE transcript_id = ?1 ORDER BY at",
        )?;
        let rows = stmt.query_map(params![transcript_id.0], |row| {
            Ok(SegmentRevision {
                id: Id(row.get("id")?),
                segment_id: Id(row.get("segment_id")?),
                transcript_id: Id(row.get("transcript_id")?),
                at: parse_dt(row.get("at")?).unwrap_or_else(Utc::now),
                before_text: row.get("before_text")?,
                after_text: row.get("after_text")?,
                author: row.get("author")?,
                note: row.get("note")?,
            })
        })?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    /// Bir bölümün tüm düzeltmelerini geri alır (otomatik metne döner).
    pub fn revert_segment(&self, id: &Id) -> CoreResult<()> {
        let n = self.conn().execute(
            "UPDATE transcript_segments SET edited_text = NULL, updated_at = ?2 WHERE id = ?1",
            params![id.0, timefmt::to_iso(&Utc::now())],
        )?;
        if n == 0 {
            return Err(CoreError::not_found("metin bölümü"));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inventory::{EvidenceFile, FileCategory, VerificationState};
    use crate::model::Investigation;
    use crate::transcript::EvidenceSource;
    use std::path::PathBuf;

    fn setup() -> (Store, Id, Id) {
        let store = Store::open_in_memory().unwrap();
        let mut inv = Investigation::new();
        inv.case_number = "2026/125".into();
        store.insert_investigation(&inv).unwrap();
        let src = EvidenceSource::new(
            &inv.id,
            "USB",
            "/tmp/usb",
            crate::device::DeviceKind::Removable,
            1,
        );
        store.insert_source(&src).unwrap();
        let file = EvidenceFile {
            id: Id::new(),
            investigation_id: inv.id.clone(),
            source_id: src.id.clone(),
            relative_path: PathBuf::from("a.wav"),
            local_relative_path: PathBuf::from("orijinal_dosyalar/a.wav"),
            size_bytes: 100,
            extension: "wav".into(),
            category: FileCategory::Audio,
            source_modified: None,
            source_created: None,
            acquired_at: None,
            source_sha256: None,
            copy_sha256: None,
            verification: VerificationState::Verified,
            audio: None,
            issue: None,
        };
        store.insert_file(&file).unwrap();
        (store, inv.id, file.id)
    }

    #[test]
    fn upsert_is_idempotent_per_file() {
        let (store, inv_id, file_id) = setup();
        let t = Transcript::new(&inv_id, &file_id);
        let a = store.upsert_transcript(&t).unwrap();
        let mut second = Transcript::new(&inv_id, &file_id);
        second.id = Id::new();
        let b = store.upsert_transcript(&second).unwrap();
        assert_eq!(a.id, b.id);
        assert_eq!(store.list_transcripts(&inv_id).unwrap().len(), 1);
    }

    #[test]
    fn segments_roundtrip_and_reindex() {
        let (store, inv_id, file_id) = setup();
        let t = store
            .upsert_transcript(&Transcript::new(&inv_id, &file_id))
            .unwrap();
        let mut s1 = TranscriptSegment::new(&t.id, 0, 0, 1000, "bir".into());
        s1.avg_logprob = Some(-0.2);
        let s2 = TranscriptSegment::new(&t.id, 1, 1000, 2500, "iki".into());
        store.insert_segments(&[s1.clone(), s2]).unwrap();

        let segs = store.list_segments(&t.id).unwrap();
        assert_eq!(segs.len(), 2);
        assert_eq!(segs[0].auto_text, "bir");
        assert_eq!(segs[0].avg_logprob, Some(-0.2));

        store.delete_segment(&segs[0].id).unwrap();
        store.reindex_segments(&t.id).unwrap();
        let segs = store.list_segments(&t.id).unwrap();
        assert_eq!(segs[0].index, 0);
        assert_eq!(segs[0].auto_text, "iki");
    }

    #[test]
    fn edited_text_kept_separate_from_auto() {
        let (store, inv_id, file_id) = setup();
        let t = store
            .upsert_transcript(&Transcript::new(&inv_id, &file_id))
            .unwrap();
        let mut seg = TranscriptSegment::new(&t.id, 0, 0, 1000, "otomatik".into());
        store.insert_segment(&seg).unwrap();

        seg.edited_text = Some("bilirkişi düzeltmesi".into());
        store.update_segment(&mut seg).unwrap();
        let got = store.get_segment(&seg.id).unwrap();
        assert_eq!(got.auto_text, "otomatik");
        assert_eq!(got.edited_text.as_deref(), Some("bilirkişi düzeltmesi"));
        assert!(got.is_edited());

        store.revert_segment(&seg.id).unwrap();
        let got = store.get_segment(&seg.id).unwrap();
        assert!(!got.is_edited());
        assert_eq!(got.effective_text(), "otomatik");
    }

    #[test]
    fn revisions_record_history() {
        let (store, inv_id, file_id) = setup();
        let t = store
            .upsert_transcript(&Transcript::new(&inv_id, &file_id))
            .unwrap();
        let seg = TranscriptSegment::new(&t.id, 0, 0, 1000, "ilk".into());
        store.insert_segment(&seg).unwrap();

        let rev = SegmentRevision::new(&seg.id, &t.id, "ilk", "sonra", "Bilirkişi");
        store.insert_revision(&rev).unwrap();
        let revs = store.list_revisions(&t.id).unwrap();
        assert_eq!(revs.len(), 1);
        assert_eq!(revs[0].before_text, "ilk");
        assert_eq!(revs[0].after_text, "sonra");
    }

    #[test]
    fn resumable_transcripts_detected() {
        let (store, inv_id, file_id) = setup();
        let mut t = store
            .upsert_transcript(&Transcript::new(&inv_id, &file_id))
            .unwrap();
        assert_eq!(store.find_resumable_transcripts(&inv_id).unwrap().len(), 1);

        t.status = TranscriptStatus::Completed;
        store.update_transcript(&mut t).unwrap();
        assert!(store.find_resumable_transcripts(&inv_id).unwrap().is_empty());
    }

    #[test]
    fn deleting_transcript_cascades_segments() {
        let (store, inv_id, file_id) = setup();
        let t = store
            .upsert_transcript(&Transcript::new(&inv_id, &file_id))
            .unwrap();
        store
            .insert_segment(&TranscriptSegment::new(&t.id, 0, 0, 100, "x".into()))
            .unwrap();
        store.delete_transcript(&t.id).unwrap();
        assert!(store.list_segments(&t.id).unwrap().is_empty());
    }
}
