//! İşlem geçmişi (audit log).
//!
//! Her önemli işlem tarih ve saat bilgisiyle kaydedilir. Kayıtlar
//! inceleme bazındadır ve rapora özet olarak girer. Hassas tam yollar
//! loglanmaz; yalnızca dosya adı ve kısa bağlam saklanır.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::model::Id;

/// İşlem türleri.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionKind {
    InvestigationCreated,
    InvestigationUpdated,
    InvestigationOpened,
    EvidenceSourceAdded,
    EvidenceSourceRemoved,
    ScanStarted,
    ScanCompleted,
    AcquisitionStarted,
    AcquisitionCompleted,
    AcquisitionFailed,
    HashVerified,
    HashMismatch,
    TranscriptQueued,
    TranscriptStarted,
    TranscriptSegmentCompleted,
    TranscriptCompleted,
    TranscriptFailed,
    TranscriptResumed,
    SegmentEdited,
    SegmentSplit,
    SegmentMerged,
    SegmentDeleted,
    SegmentAdded,
    SpeakerLabeled,
    NoteAdded,
    NoteUpdated,
    DerivedAudioCreated,
    ReportDrafted,
    ReportExported,
    BackupCreated,
    BackupRestored,
    Other,
}

impl ActionKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::InvestigationCreated => "investigation_created",
            Self::InvestigationUpdated => "investigation_updated",
            Self::InvestigationOpened => "investigation_opened",
            Self::EvidenceSourceAdded => "evidence_source_added",
            Self::EvidenceSourceRemoved => "evidence_source_removed",
            Self::ScanStarted => "scan_started",
            Self::ScanCompleted => "scan_completed",
            Self::AcquisitionStarted => "acquisition_started",
            Self::AcquisitionCompleted => "acquisition_completed",
            Self::AcquisitionFailed => "acquisition_failed",
            Self::HashVerified => "hash_verified",
            Self::HashMismatch => "hash_mismatch",
            Self::TranscriptQueued => "transcript_queued",
            Self::TranscriptStarted => "transcript_started",
            Self::TranscriptSegmentCompleted => "transcript_segment_completed",
            Self::TranscriptCompleted => "transcript_completed",
            Self::TranscriptFailed => "transcript_failed",
            Self::TranscriptResumed => "transcript_resumed",
            Self::SegmentEdited => "segment_edited",
            Self::SegmentSplit => "segment_split",
            Self::SegmentMerged => "segment_merged",
            Self::SegmentDeleted => "segment_deleted",
            Self::SegmentAdded => "segment_added",
            Self::SpeakerLabeled => "speaker_labeled",
            Self::NoteAdded => "note_added",
            Self::NoteUpdated => "note_updated",
            Self::DerivedAudioCreated => "derived_audio_created",
            Self::ReportDrafted => "report_drafted",
            Self::ReportExported => "report_exported",
            Self::BackupCreated => "backup_created",
            Self::BackupRestored => "backup_restored",
            Self::Other => "other",
        }
    }

    /// Raporda gösterilecek Türkçe etiket.
    pub fn label_tr(self) -> &'static str {
        match self {
            Self::InvestigationCreated => "İnceleme oluşturuldu",
            Self::InvestigationUpdated => "İnceleme bilgileri güncellendi",
            Self::InvestigationOpened => "İnceleme açıldı",
            Self::EvidenceSourceAdded => "Delil kaynağı eklendi",
            Self::EvidenceSourceRemoved => "Delil kaynağı kaldırıldı",
            Self::ScanStarted => "Kaynak taraması başladı",
            Self::ScanCompleted => "Kaynak taraması tamamlandı",
            Self::AcquisitionStarted => "Dosya aktarımı başladı",
            Self::AcquisitionCompleted => "Dosya aktarımı tamamlandı",
            Self::AcquisitionFailed => "Dosya aktarımı başarısız",
            Self::HashVerified => "SHA-256 doğrulandı",
            Self::HashMismatch => "SHA-256 uyuşmazlığı",
            Self::TranscriptQueued => "Deşifre kuyruğa alındı",
            Self::TranscriptStarted => "Deşifre başladı",
            Self::TranscriptSegmentCompleted => "Deşifre parçası tamamlandı",
            Self::TranscriptCompleted => "Deşifre tamamlandı",
            Self::TranscriptFailed => "Deşifre başarısız",
            Self::TranscriptResumed => "Deşifre kaldığı yerden sürdürüldü",
            Self::SegmentEdited => "Metin bölümü düzeltildi",
            Self::SegmentSplit => "Metin bölümü ayrıldı",
            Self::SegmentMerged => "Metin bölümleri birleştirildi",
            Self::SegmentDeleted => "Metin bölümü silindi",
            Self::SegmentAdded => "Metin bölümü eklendi",
            Self::SpeakerLabeled => "Konuşmacı etiketlendi",
            Self::NoteAdded => "Not eklendi",
            Self::NoteUpdated => "Not güncellendi",
            Self::DerivedAudioCreated => "Türetilmiş ses dosyası oluşturuldu",
            Self::ReportDrafted => "Rapor taslağı oluşturuldu",
            Self::ReportExported => "Rapor dışa aktarıldı",
            Self::BackupCreated => "Yedek oluşturuldu",
            Self::BackupRestored => "Yedekten geri yüklendi",
            Self::Other => "Diğer işlem",
        }
    }
}

/// Tek bir işlem kaydı.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    pub id: Id,
    pub investigation_id: Id,
    pub at: DateTime<Utc>,
    pub kind: ActionKind,
    /// Kısa, hassas veri içermeyen açıklama.
    pub summary: String,
    /// İsteğe bağlı ek ayrıntı (JSON). Hassas tam yollar buraya yazılmaz.
    pub details: Option<serde_json::Value>,
}

impl AuditEntry {
    pub fn new(investigation_id: &Id, kind: ActionKind, summary: impl Into<String>) -> Self {
        Self {
            id: Id::new(),
            investigation_id: investigation_id.clone(),
            at: Utc::now(),
            kind,
            summary: summary.into(),
            details: None,
        }
    }

    pub fn with_details(mut self, details: serde_json::Value) -> Self {
        self.details = Some(details);
        self
    }
}
