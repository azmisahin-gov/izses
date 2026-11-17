//! Deşifre, metin bölümleri, düzeltme geçmişi ve notlar.
//!
//! Otomatik üretilen metin ile bilirkişinin düzelttiği metin her zaman ayrı
//! alanlarda saklanır. Böylece hangi metnin model tarafından üretildiği ve
//! hangisinin bilirkişi tarafından değiştirildiği her zaman anlaşılır.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::model::Id;

/// Deşifre işinin durumu.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TranscriptStatus {
    /// Kuyruğa alındı, henüz başlamadı.
    Queued,
    /// İşlem sürüyor.
    Running,
    /// Tamamlandı.
    Completed,
    /// Hata nedeniyle durdu; kaldığı yerden sürdürülebilir.
    Failed,
    /// Kullanıcı iptal etti.
    Cancelled,
}

impl TranscriptStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Running => "running",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }

    pub fn from_str_opt(s: &str) -> Option<Self> {
        match s {
            "queued" => Some(Self::Queued),
            "running" => Some(Self::Running),
            "completed" => Some(Self::Completed),
            "failed" => Some(Self::Failed),
            "cancelled" => Some(Self::Cancelled),
            _ => None,
        }
    }

    pub fn label_tr(self) -> &'static str {
        match self {
            Self::Queued => "Kuyrukta",
            Self::Running => "İşleniyor",
            Self::Completed => "Tamamlandı",
            Self::Failed => "Hata",
            Self::Cancelled => "İptal edildi",
        }
    }
}

/// Deşifre işleminin ayarları ve kaydı.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transcript {
    pub id: Id,
    pub investigation_id: Id,
    pub evidence_file_id: Id,
    /// Kullanılan model adı (ör. whisper-large-v3-turbo).
    pub model_name: String,
    /// Model sürümü veya kaynağı.
    pub model_revision: String,
    /// Model ağırlık dosyasının SHA-256 değeri.
    pub model_sha256: Option<String>,
    /// Dil kodu (ör. "tr").
    pub language: String,
    /// Görev: transcribe (çeviri yapılmaz).
    pub task: String,
    /// Kullanılan ayarlar (JSON): sıcaklık, kiriş vb.
    pub settings: serde_json::Value,
    pub status: TranscriptStatus,
    /// Tamamlanan parça sayısı.
    pub completed_segments: u32,
    /// Toplam parça sayısı (biliniyorsa).
    pub total_segments: Option<u32>,
    /// Hata mesajı (varsa).
    pub error: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Transcript {
    pub fn new(investigation_id: &Id, evidence_file_id: &Id) -> Self {
        let now = Utc::now();
        Self {
            id: Id::new(),
            investigation_id: investigation_id.clone(),
            evidence_file_id: evidence_file_id.clone(),
            model_name: String::new(),
            model_revision: String::new(),
            model_sha256: None,
            language: "tr".to_string(),
            task: "transcribe".to_string(),
            settings: serde_json::json!({}),
            status: TranscriptStatus::Queued,
            completed_segments: 0,
            total_segments: None,
            error: None,
            created_at: now,
            updated_at: now,
        }
    }

    /// Tamamlanma oranı (0.0–1.0), toplam biliniyorsa.
    pub fn progress(&self) -> Option<f32> {
        let total = self.total_segments?;
        if total == 0 {
            return Some(1.0);
        }
        Some((self.completed_segments as f32 / total as f32).clamp(0.0, 1.0))
    }
}

/// Bölüm üzerindeki bilirkişi işaretleri.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SegmentFlags {
    /// Anlaşılamayan bölüm.
    pub unclear: bool,
    /// Yeniden dinlenecek bölüm.
    pub relisten: bool,
    /// Gürültü gözlemi.
    pub noise: bool,
    /// Üst üste konuşma gözlemi.
    pub overlap: bool,
}

/// Zaman damgalı tek bir metin bölümü.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptSegment {
    pub id: Id,
    pub transcript_id: Id,
    /// Bölümün sırası.
    pub index: u32,
    pub start_ms: u64,
    pub end_ms: u64,
    /// Modelin ürettiği metin (değiştirilmez).
    pub auto_text: String,
    /// Bilirkişinin düzelttiği metin; `None` ise düzeltme yapılmamıştır.
    pub edited_text: Option<String>,
    /// Konuşmacı etiketi; yalnızca bilirkişi tarafından atanır.
    pub speaker: Option<String>,
    pub flags: SegmentFlags,
    /// Bilirkişi gözlem notu.
    pub observation: Option<String>,
    /// Modelin ürettiği ortalama log-olasılık; model sağlamıyorsa `None`.
    pub avg_logprob: Option<f64>,
    /// Konuşma yok olasılığı; model sağlamıyorsa `None`.
    pub no_speech_prob: Option<f64>,
    pub updated_at: DateTime<Utc>,
}

impl TranscriptSegment {
    pub fn new(
        transcript_id: &Id,
        index: u32,
        start_ms: u64,
        end_ms: u64,
        auto_text: String,
    ) -> Self {
        Self {
            id: Id::new(),
            transcript_id: transcript_id.clone(),
            index,
            start_ms,
            end_ms,
            auto_text,
            edited_text: None,
            speaker: None,
            flags: SegmentFlags::default(),
            observation: None,
            avg_logprob: None,
            no_speech_prob: None,
            updated_at: Utc::now(),
        }
    }

    /// Bilirkişinin göreceği nihai metin.
    pub fn effective_text(&self) -> &str {
        self.edited_text.as_deref().unwrap_or(&self.auto_text)
    }

    /// Bölüm düzeltilmiş mi?
    pub fn is_edited(&self) -> bool {
        self.edited_text.is_some()
    }

    pub fn duration_ms(&self) -> u64 {
        self.end_ms.saturating_sub(self.start_ms)
    }

    pub fn time_label(&self) -> String {
        format!(
            "{} – {}",
            crate::timefmt::format_ms(self.start_ms),
            crate::timefmt::format_ms(self.end_ms)
        )
    }

    /// Bilirkişinin dikkat etmesi gereken bir işaret var mı?
    pub fn has_attention_flag(&self) -> bool {
        self.flags.unclear || self.flags.relisten || self.flags.noise || self.flags.overlap
    }
}

/// Metin değişiklik geçmişi kaydı.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SegmentRevision {
    pub id: Id,
    pub segment_id: Id,
    pub transcript_id: Id,
    pub at: DateTime<Utc>,
    /// Önceki nihai metin.
    pub before_text: String,
    /// Yeni nihai metin.
    pub after_text: String,
    /// Değişikliği yapan (bilirkişi).
    pub author: String,
    /// Kısa açıklama (ör. "düzeltme", "bölme", "birleştirme").
    pub note: Option<String>,
}

impl SegmentRevision {
    pub fn new(
        segment_id: &Id,
        transcript_id: &Id,
        before_text: impl Into<String>,
        after_text: impl Into<String>,
        author: impl Into<String>,
    ) -> Self {
        Self {
            id: Id::new(),
            segment_id: segment_id.clone(),
            transcript_id: transcript_id.clone(),
            at: Utc::now(),
            before_text: before_text.into(),
            after_text: after_text.into(),
            author: author.into(),
            note: None,
        }
    }
}

/// İnceleme notu kategorisi.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NoteKind {
    General,
    Technical,
    Observation,
    Finding,
}

impl NoteKind {
    pub fn label_tr(self) -> &'static str {
        match self {
            Self::General => "Genel",
            Self::Technical => "Teknik",
            Self::Observation => "Gözlem",
            Self::Finding => "Bulgu",
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::General => "general",
            Self::Technical => "technical",
            Self::Observation => "observation",
            Self::Finding => "finding",
        }
    }

    pub fn from_str_opt(s: &str) -> Option<Self> {
        match s {
            "general" => Some(Self::General),
            "technical" => Some(Self::Technical),
            "observation" => Some(Self::Observation),
            "finding" => Some(Self::Finding),
            _ => None,
        }
    }
}

/// İnceleme sırasında alınan not.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Note {
    pub id: Id,
    pub investigation_id: Id,
    /// İlgili ses dosyası (isteğe bağlı).
    pub evidence_file_id: Option<Id>,
    /// İlgili metin bölümü (isteğe bağlı).
    pub segment_id: Option<Id>,
    pub kind: NoteKind,
    pub title: String,
    pub body: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Note {
    pub fn new(
        investigation_id: &Id,
        kind: NoteKind,
        title: impl Into<String>,
        body: impl Into<String>,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Id::new(),
            investigation_id: investigation_id.clone(),
            evidence_file_id: None,
            segment_id: None,
            kind,
            title: title.into(),
            body: body.into(),
            created_at: now,
            updated_at: now,
        }
    }
}

/// Kaynak delil tanımı.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceSource {
    pub id: Id,
    pub investigation_id: Id,
    /// Kullanıcıya gösterilecek etiket (ör. "KINGSTON USB").
    pub label: String,
    /// Kaynağın kök yolu.
    pub root_path: std::path::PathBuf,
    /// Aygıt türü.
    pub kind: crate::device::DeviceKind,
    pub filesystem: Option<String>,
    pub total_bytes: Option<u64>,
    /// Kaynak tanımının sırası (kaynak_001, kaynak_002 ...).
    pub ordinal: u32,
    pub added_at: DateTime<Utc>,
    /// Son tarama zamanı.
    pub last_scan_at: Option<DateTime<Utc>>,
    /// Son taramada bulunan dosya sayısı.
    pub last_scan_count: Option<u32>,
}

impl EvidenceSource {
    pub fn new(
        investigation_id: &Id,
        label: impl Into<String>,
        root_path: impl Into<std::path::PathBuf>,
        kind: crate::device::DeviceKind,
        ordinal: u32,
    ) -> Self {
        Self {
            id: Id::new(),
            investigation_id: investigation_id.clone(),
            label: label.into(),
            root_path: root_path.into(),
            kind,
            filesystem: None,
            total_bytes: None,
            ordinal,
            added_at: Utc::now(),
            last_scan_at: None,
            last_scan_count: None,
        }
    }

    /// Kaynak klasörünün adı (ör. "kaynak_001").
    pub fn folder_name(&self) -> String {
        format!("kaynak_{:03}", self.ordinal)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn effective_text_prefers_edit() {
        let mut seg = TranscriptSegment::new(&Id::new(), 0, 0, 1000, "otomatik metin".into());
        assert_eq!(seg.effective_text(), "otomatik metin");
        assert!(!seg.is_edited());
        seg.edited_text = Some("düzeltilmiş metin".into());
        assert_eq!(seg.effective_text(), "düzeltilmiş metin");
        assert!(seg.is_edited());
    }

    #[test]
    fn progress_is_clamped() {
        let mut t = Transcript::new(&Id::new(), &Id::new());
        assert!(t.progress().is_none());
        t.total_segments = Some(10);
        t.completed_segments = 5;
        assert_eq!(t.progress(), Some(0.5));
        t.completed_segments = 99;
        assert_eq!(t.progress(), Some(1.0));
    }

    #[test]
    fn status_roundtrip() {
        for s in [
            TranscriptStatus::Queued,
            TranscriptStatus::Running,
            TranscriptStatus::Completed,
            TranscriptStatus::Failed,
            TranscriptStatus::Cancelled,
        ] {
            assert_eq!(TranscriptStatus::from_str_opt(s.as_str()), Some(s));
        }
    }

    #[test]
    fn time_label_formats() {
        let seg = TranscriptSegment::new(&Id::new(), 0, 1500, 4200, String::new());
        assert_eq!(seg.time_label(), "00:00:01.500 – 00:00:04.200");
        assert_eq!(seg.duration_ms(), 2700);
    }
}
