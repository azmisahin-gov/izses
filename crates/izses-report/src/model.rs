//! Bilirkişi raporu veri modeli.
//!
//! Rapor, inceleme boyunca girilen bilgilerden otomatik olarak derlenir.
//! Otomatik oluşturulan kısımlar ile bilirkişinin kendi değerlendirmesi
//! ayrı alanlarda tutulur ve raporda ayırt edilebilir.

use chrono::{DateTime, Utc};
use izses_core::inventory::EvidenceFile;
use izses_core::model::Investigation;
use izses_core::transcript::{EvidenceSource, TranscriptSegment};
use serde::{Deserialize, Serialize};

/// Rapordaki bir başlık ve gövde bloğu.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Section {
    pub heading: String,
    /// Paragraflar.
    pub paragraphs: Vec<String>,
    /// Madde işaretli liste (varsa).
    pub bullets: Vec<String>,
    /// Tablo satırları (varsa).
    pub table: Option<Table>,
    /// Bu bölüm bilirkişi tarafından mı yazıldı?
    pub authored_by_expert: bool,
}

impl Section {
    pub fn new(heading: impl Into<String>) -> Self {
        Self {
            heading: heading.into(),
            paragraphs: Vec::new(),
            bullets: Vec::new(),
            table: None,
            authored_by_expert: false,
        }
    }

    pub fn with_paragraph(mut self, text: impl Into<String>) -> Self {
        self.paragraphs.push(text.into());
        self
    }

    pub fn with_bullet(mut self, text: impl Into<String>) -> Self {
        self.bullets.push(text.into());
        self
    }

    pub fn expert(mut self) -> Self {
        self.authored_by_expert = true;
        self
    }

    pub fn is_empty(&self) -> bool {
        self.paragraphs.is_empty()
            && self.bullets.is_empty()
            && self.table.as_ref().map(|t| t.rows.is_empty()).unwrap_or(true)
    }
}

/// Basit tablo.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Table {
    pub headers: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

/// Deşifre metni bölümü (rapora gömülür).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptBlock {
    pub file_name: String,
    pub source_label: String,
    pub sha256: Option<String>,
    pub duration_label: String,
    pub segments: Vec<TranscriptSegment>,
}

/// Rapor öncesi yapılan tutarlılık kontrolünün tek bir bulgusu.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckSeverity {
    /// Rapor üretimini engelleyen eksiklik.
    Blocker,
    /// Dikkat edilmesi gereken uyarı.
    Warning,
    /// Bilgilendirme.
    Info,
}

impl CheckSeverity {
    pub fn label_tr(self) -> &'static str {
        match self {
            Self::Blocker => "Engel",
            Self::Warning => "Uyarı",
            Self::Info => "Bilgi",
        }
    }
}

/// Tutarlılık kontrolü bulgusu.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportCheck {
    pub severity: CheckSeverity,
    pub message: String,
}

/// Derlenmiş rapor.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Report {
    pub title: String,
    pub generated_at: DateTime<Utc>,
    pub investigation: Investigation,
    pub sections: Vec<Section>,
    pub transcript_blocks: Vec<TranscriptBlock>,
    pub checks: Vec<ReportCheck>,
}

impl Report {
    /// Raporun engelleyici eksikleri var mı?
    pub fn has_blockers(&self) -> bool {
        self.checks
            .iter()
            .any(|c| c.severity == CheckSeverity::Blocker)
    }

    pub fn blockers(&self) -> impl Iterator<Item = &ReportCheck> {
        self.checks
            .iter()
            .filter(|c| c.severity == CheckSeverity::Blocker)
    }

    pub fn warnings(&self) -> impl Iterator<Item = &ReportCheck> {
        self.checks
            .iter()
            .filter(|c| c.severity == CheckSeverity::Warning)
    }

    /// Bütün bölümleri sırayla dolaşır.
    pub fn all_sections(&self) -> impl Iterator<Item = &Section> {
        self.sections.iter()
    }
}

/// Rapor derleme girdisi: inceleme, kaynaklar, dosyalar, deşifreler ve notlar.
pub struct ReportInput<'a> {
    pub investigation: &'a Investigation,
    pub sources: &'a [EvidenceSource],
    pub files: &'a [EvidenceFile],
    /// Dosya kimliğine göre deşifre bölümleri.
    pub transcripts: Vec<(&'a EvidenceFile, &'a [TranscriptSegment])>,
    pub notes: &'a [izses_core::transcript::Note],
    /// Bilirkişinin kendi değerlendirmesi (serbest metin).
    pub expert_assessment: String,
    /// Gerekçeli sonuç (serbest metin).
    pub conclusion: String,
    /// Kullanılan model adı ve sürümü.
    pub model_name: String,
    pub model_revision: String,
    /// İnceleme yöntemi açıklaması (bilirkişi düzenleyebilir).
    pub method_notes: String,
    /// İşlem geçmişi özeti (satırlar).
    pub history_lines: Vec<String>,
}
