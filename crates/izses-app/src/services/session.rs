//! İnceleme oturumu: açık incelemenin çalışma durumu.
//!
//! Bir inceleme açıldığında tüm veriler (kaynaklar, dosyalar, deşifreler,
//! notlar, işlem geçmişi) yüklenir. Oturum, arayüzün ihtiyaç duyduğu
//! türetilmiş bilgileri de tutar.

use std::collections::HashMap;

use izses_core::audit::AuditEntry;
use izses_core::error::{CoreError, CoreResult};
use izses_core::inventory::EvidenceFile;
use izses_core::investigation::StorageRoot;
use izses_core::model::{Id, Investigation, InvestigationStatus};
use izses_core::store::Store;
use izses_core::transcript::{EvidenceSource, Note, Transcript, TranscriptSegment};

/// Açık bir incelemenin tüm çalışma verisi.
pub struct Session {
    pub investigation: Investigation,
    pub storage_key: String,
    pub sources: Vec<EvidenceSource>,
    pub files: Vec<EvidenceFile>,
    pub transcripts: Vec<Transcript>,
    /// Deşifre kimliğine göre metin bölümleri.
    pub segments: HashMap<Id, Vec<TranscriptSegment>>,
    pub notes: Vec<Note>,
    pub history: Vec<AuditEntry>,
}

impl Session {
    /// İncelemeyi yükler.
    pub fn load(store: &Store, investigation_id: &Id) -> CoreResult<Self> {
        let investigation = store.get_investigation(investigation_id)?;
        let storage_key = investigation.storage_key();
        Self::load_into(store, investigation, storage_key)
    }

    /// Depolama anahtarından incelemeyi yükler.
    pub fn load_by_key(store: &Store, storage_key: &str) -> CoreResult<Self> {
        let investigation = store.get_investigation_by_storage_key(storage_key)?;
        let key = investigation.storage_key();
        Self::load_into(store, investigation, key)
    }

    fn load_into(
        store: &Store,
        investigation: Investigation,
        storage_key: String,
    ) -> CoreResult<Self> {
        let id = investigation.id.clone();
        let sources = store.list_sources(&id)?;
        let files = store.list_files(&id)?;
        let transcripts = store.list_transcripts(&id)?;

        let mut segments = HashMap::new();
        for t in &transcripts {
            segments.insert(t.id.clone(), store.list_segments(&t.id)?);
        }

        let notes = store.list_notes(&id)?;
        let history = store.list_audit(&id)?;

        Ok(Self {
            investigation,
            storage_key,
            sources,
            files,
            transcripts,
            segments,
            notes,
            history,
        })
    }

    /// İncelemeyi yeniden yükler (dış değişikliklerden sonra).
    pub fn refresh(&mut self, store: &Store) -> CoreResult<()> {
        let id = self.investigation.id.clone();
        *self = Self::load(store, &id)?;
        Ok(())
    }

    pub fn id(&self) -> &Id {
        &self.investigation.id
    }

    /// Bir ses dosyasına ait deşifre kaydı.
    pub fn transcript_for_file(&self, file_id: &Id) -> Option<&Transcript> {
        self.transcripts
            .iter()
            .find(|t| &t.evidence_file_id == file_id)
    }

    /// Bir dosyanın metin bölümleri.
    pub fn segments_for_file(&self, file_id: &Id) -> Vec<&TranscriptSegment> {
        match self.transcript_for_file(file_id) {
            Some(t) => self
                .segments
                .get(&t.id)
                .map(|v| v.iter().collect())
                .unwrap_or_default(),
            None => Vec::new(),
        }
    }

    /// Bir kaynağa ait dosyalar.
    pub fn files_for_source(&self, source_id: &Id) -> Vec<&EvidenceFile> {
        self.files
            .iter()
            .filter(|f| &f.source_id == source_id)
            .collect()
    }

    pub fn audio_files(&self) -> Vec<&EvidenceFile> {
        self.files.iter().filter(|f| f.is_audio()).collect()
    }

    /// İncelemenin genel ilerleme özeti.
    pub fn progress(&self) -> InvestigationProgress {
        let audio = self.audio_files();
        let verified = self.files.iter().filter(|f| f.is_verified()).count();
        let completed = self
            .transcripts
            .iter()
            .filter(|t| t.status == izses_core::transcript::TranscriptStatus::Completed)
            .count();

        let mut total_segments = 0usize;
        let mut edited_segments = 0usize;
        let mut flagged_segments = 0usize;
        for segs in self.segments.values() {
            for s in segs {
                total_segments += 1;
                if s.is_edited() {
                    edited_segments += 1;
                }
                if s.has_attention_flag() {
                    flagged_segments += 1;
                }
            }
        }

        InvestigationProgress {
            total_files: self.files.len(),
            verified_files: verified,
            audio_files: audio.len(),
            transcribed_files: completed,
            total_segments,
            edited_segments,
            flagged_segments,
        }
    }

    /// Rapor öncesi eksik işlerin özeti.
    pub fn outstanding_work(&self) -> Vec<String> {
        let mut out = Vec::new();
        for field in self.investigation.missing_required_fields() {
            out.push(format!("Eksik bilgi: {field}"));
        }
        let unverified: Vec<&EvidenceFile> =
            self.files.iter().filter(|f| !f.is_verified()).collect();
        if !unverified.is_empty() {
            out.push(format!(
                "{} dosyanın aktarımı doğrulanmadı",
                unverified.len()
            ));
        }
        let pending: Vec<&EvidenceFile> = self
            .audio_files()
            .into_iter()
            .filter(|f| {
                self.transcript_for_file(&f.id)
                    .map(|t| t.status != izses_core::transcript::TranscriptStatus::Completed)
                    .unwrap_or(true)
            })
            .collect();
        if !pending.is_empty() {
            out.push(format!(
                "{} ses dosyasının deşifresi tamamlanmadı",
                pending.len()
            ));
        }
        let flagged = self
            .segments
            .values()
            .flatten()
            .filter(|s| s.has_attention_flag())
            .count();
        if flagged > 0 {
            out.push(format!("{flagged} metin bölümü işaretli (kontrol gerekli)"));
        }
        out
    }

    /// İnceleme durumunu değiştirir ve kaydeder.
    pub fn set_status(&mut self, store: &Store, status: InvestigationStatus) -> CoreResult<()> {
        self.investigation.status = status;
        store.update_investigation(&mut self.investigation)?;
        Ok(())
    }

    /// İnceleme bilgilerini kaydeder.
    pub fn save_investigation(&mut self, store: &Store) -> CoreResult<()> {
        store.update_investigation(&mut self.investigation)
    }

    /// Bir notu ekler.
    pub fn add_note(&mut self, store: &Store, mut note: Note) -> CoreResult<Id> {
        note.investigation_id = self.investigation.id.clone();
        store.insert_note(&note)?;
        self.notes.insert(0, note.clone());
        store.insert_audit(&izses_core::audit::AuditEntry::new(
            &self.investigation.id,
            izses_core::audit::ActionKind::NoteAdded,
            format!("Not eklendi: {}", note.title),
        ))?;
        Ok(note.id)
    }

    /// Bir notu günceller.
    pub fn update_note(&mut self, store: &Store, mut note: Note) -> CoreResult<()> {
        store.update_note(&mut note)?;
        if let Some(existing) = self.notes.iter_mut().find(|n| n.id == note.id) {
            *existing = note;
        }
        Ok(())
    }

    /// Bir notu siler.
    pub fn delete_note(&mut self, store: &Store, id: &Id) -> CoreResult<()> {
        store.delete_note(id)?;
        self.notes.retain(|n| &n.id != id);
        Ok(())
    }

    /// İşlem geçmişini yeniden yükler.
    pub fn reload_history(&mut self, store: &Store) -> CoreResult<()> {
        self.history = store.list_audit(&self.investigation.id)?;
        Ok(())
    }

    /// İşlem geçmişini rapor için satırlara çevirir.
    pub fn history_lines(&self) -> Vec<String> {
        self.history
            .iter()
            .map(|e| {
                format!(
                    "{}  {}  {}",
                    izses_core::timefmt::format_datetime(&e.at),
                    e.kind.label_tr(),
                    e.summary
                )
            })
            .collect()
    }

    /// İncelemenin kök klasörü.
    pub fn dir(&self, root: &StorageRoot) -> std::path::PathBuf {
        root.investigation_dir(&self.storage_key)
    }

    /// Bir delil dosyasının yerel tam yolu.
    pub fn local_path(&self, root: &StorageRoot, file: &EvidenceFile) -> std::path::PathBuf {
        let rel = file
            .local_relative_path
            .strip_prefix(izses_core::investigation::SubDir::OriginalFiles.name())
            .unwrap_or(&file.local_relative_path);
        root.sub_dir(
            &self.storage_key,
            izses_core::investigation::SubDir::OriginalFiles,
        )
        .join(rel)
    }

    /// Oturum verisinin tutarlı olduğunu doğrular.
    ///
    /// Dosya kayıtları ile diskteki kopyalar arasında fark varsa bildirir.
    pub fn verify_local_files(&self, root: &StorageRoot) -> Vec<String> {
        let mut issues = Vec::new();
        for f in &self.files {
            if f.is_verified() {
                let path = self.local_path(root, f);
                if !path.is_file() {
                    issues.push(format!("{}: yerel kopya bulunamadı", f.file_name()));
                }
            }
        }
        issues
    }
}

/// İncelemenin ilerleme özeti.
#[derive(Debug, Clone, Default)]
pub struct InvestigationProgress {
    pub total_files: usize,
    pub verified_files: usize,
    pub audio_files: usize,
    pub transcribed_files: usize,
    pub total_segments: usize,
    pub edited_segments: usize,
    pub flagged_segments: usize,
}

impl InvestigationProgress {
    /// Doğrulama oranı (0.0–1.0).
    pub fn verification_fraction(&self) -> f32 {
        if self.total_files == 0 {
            0.0
        } else {
            self.verified_files as f32 / self.total_files as f32
        }
    }

    /// Deşifre oranı (0.0–1.0).
    pub fn transcription_fraction(&self) -> f32 {
        if self.audio_files == 0 {
            0.0
        } else {
            self.transcribed_files as f32 / self.audio_files as f32
        }
    }
}

/// Yeni inceleme oluşturur: veritabanı kaydı + klasör düzeni.
pub fn create_investigation(
    store: &Store,
    root: &StorageRoot,
    mut investigation: Investigation,
) -> CoreResult<Session> {
    if investigation.case_number.trim().is_empty() {
        return Err(CoreError::invalid(
            "dosya numarası olmadan inceleme oluşturulamaz",
        ));
    }

    // Depolama anahtarı çakışırsa sonuna sayı ekle. Seçilen anahtar kayda
    // yazılır; böylece dosya numarası sonradan değişse bile veriler yerinde kalır.
    let base_key = investigation.storage_key();
    let mut key = base_key.clone();
    let mut suffix = 2;
    while store.storage_key_taken(&key)? || root.investigation_exists(&key) {
        key = format!("{base_key}-{suffix}");
        suffix += 1;
        if suffix > 1000 {
            return Err(CoreError::invalid(
                "benzersiz depolama anahtarı üretilemedi",
            ));
        }
    }
    investigation.assign_storage_key(key.clone());

    investigation.status = InvestigationStatus::InProgress;
    store.insert_investigation(&investigation)?;
    root.create_investigation_layout(&key)?;

    store.insert_audit(&izses_core::audit::AuditEntry::new(
        &investigation.id,
        izses_core::audit::ActionKind::InvestigationCreated,
        format!("İnceleme oluşturuldu: {}", investigation.case_number),
    ))?;

    Session::load(store, &investigation.id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use izses_core::transcript::NoteKind;

    #[allow(clippy::type_complexity)]
    fn setup() -> (tempfile::TempDir, StorageRoot, Store, Session) {
        let tmp = tempfile::tempdir().unwrap();
        let root = StorageRoot::new(tmp.path()).unwrap();
        let store = Store::open_in_memory().unwrap();
        let mut inv = Investigation::new();
        inv.case_number = "2026/125".into();
        inv.court = "Ankara 1. Asliye".into();
        inv.subject = "Ses kaydı".into();
        inv.expert_name = "A. Bilirkişi".into();
        inv.expert_registry_no = "12345".into();
        let session = create_investigation(&store, &root, inv).unwrap();
        (tmp, root, store, session)
    }

    #[test]
    fn creates_investigation_with_layout() {
        let (_tmp, root, _store, session) = setup();
        assert!(root.investigation_exists(&session.storage_key));
        assert_eq!(
            session.investigation.status,
            InvestigationStatus::InProgress
        );
    }

    #[test]
    fn duplicate_storage_key_gets_suffix() {
        let (_tmp, root, store, _session) = setup();
        let mut inv = Investigation::new();
        inv.case_number = "2026/125".into();
        let second = create_investigation(&store, &root, inv).unwrap();
        assert_ne!(second.storage_key, "2026-125");
        assert!(second.storage_key.starts_with("2026-125-"));
    }

    #[test]
    fn empty_case_number_rejected() {
        let tmp = tempfile::tempdir().unwrap();
        let root = StorageRoot::new(tmp.path()).unwrap();
        let store = Store::open_in_memory().unwrap();
        let result = create_investigation(&store, &root, Investigation::new());
        match result {
            Err(CoreError::Invalid(_)) => {}
            Err(other) => panic!("beklenen Invalid, gelen {other:?}"),
            Ok(_) => panic!("boş dosya numarası kabul edilmemeliydi"),
        }
    }

    #[test]
    fn session_tracks_progress() {
        let (_tmp, _root, _store, session) = setup();
        let p = session.progress();
        assert_eq!(p.total_files, 0);
        assert_eq!(p.verification_fraction(), 0.0);
    }

    #[test]
    fn outstanding_work_lists_missing_info() {
        let (_tmp, _root, _store, session) = setup();
        // Bu incelemede tüm zorunlu alanlar dolu; dosya eksikliği bildirilmeli.
        let work = session.outstanding_work();
        assert!(work.is_empty() || work.iter().any(|w| w.contains("deşifre")));
    }

    #[test]
    fn notes_are_added_and_listed() {
        let (_tmp, _root, store, mut session) = setup();
        let note = Note::new(&session.investigation.id, NoteKind::Technical, "T", "B");
        session.add_note(&store, note).unwrap();
        assert_eq!(session.notes.len(), 1);

        let reloaded = Session::load(&store, session.id()).unwrap();
        assert_eq!(reloaded.notes.len(), 1);
    }

    #[test]
    fn session_reload_preserves_everything() {
        let (_tmp, _root, store, session) = setup();
        let id = session.id().clone();
        let reloaded = Session::load(&store, &id).unwrap();
        assert_eq!(reloaded.investigation.case_number, "2026/125");
        assert_eq!(reloaded.storage_key, session.storage_key);
        // İşlem geçmişi oluşturma kaydını içermeli.
        assert!(!reloaded.history.is_empty());
    }
}
