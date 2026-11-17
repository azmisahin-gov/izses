//! İnceleme merkezli veri modeli.
//!
//! Temel ilişki:
//! `Bir inceleme → birden fazla delil kaynağı → birden fazla dosya →`
//! `her ses dosyası için deşifre → bilirkişi kontrolü → rapor`.

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Tüm varlıklarda kullanılan kimlik türü.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Id(pub String);

impl Id {
    pub fn new() -> Self {
        Self(Uuid::new_v4().to_string())
    }
}

impl Default for Id {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for Id {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// İncelemenin yaşam döngüsü.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InvestigationStatus {
    /// Yeni oluşturuldu, bilgiler eksik olabilir.
    Draft,
    /// Üzerinde çalışılıyor.
    InProgress,
    /// Bilirkişi tamamladı olarak işaretledi.
    Completed,
    /// Arşivlendi.
    Archived,
}

impl InvestigationStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::InProgress => "in_progress",
            Self::Completed => "completed",
            Self::Archived => "archived",
        }
    }

    pub fn from_str_opt(s: &str) -> Option<Self> {
        match s {
            "draft" => Some(Self::Draft),
            "in_progress" => Some(Self::InProgress),
            "completed" => Some(Self::Completed),
            "archived" => Some(Self::Archived),
            _ => None,
        }
    }

    pub fn label_tr(self) -> &'static str {
        match self {
            Self::Draft => "Taslak",
            Self::InProgress => "Devam ediyor",
            Self::Completed => "Tamamlandı",
            Self::Archived => "Arşivlendi",
        }
    }
}

/// Bir inceleme kaydı.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Investigation {
    pub id: Id,
    /// Görevlendiren mahkeme veya merci.
    pub court: String,
    /// Dosya / soruşturma numarası.
    pub case_number: String,
    /// Görevlendirme tarihi.
    pub assignment_date: Option<NaiveDate>,
    /// Son teslim tarihi.
    pub due_date: Option<NaiveDate>,
    /// Verilen süre (gün).
    pub allowed_days: Option<u32>,
    /// Taraf bilgileri.
    pub parties: Vec<Party>,
    /// İnceleme konusu.
    pub subject: String,
    /// Bilirkişinin adı ve soyadı.
    pub expert_name: String,
    /// Bilirkişi sicil numarası.
    pub expert_registry_no: String,
    /// Uzmanlık alanı.
    pub expertise_field: String,
    /// Görevlendirme yazısındaki sorular.
    pub questions: Vec<String>,
    /// Yerel depolama klasörü anahtarı.
    ///
    /// İnceleme oluşturulurken bir kez atanır ve sonradan değiştirilmez;
    /// böylece dosya numarası değişse bile veriler yerinde kalır.
    pub storage_key: String,
    /// İnceleme kapsamı notları.
    pub scope_notes: String,
    pub status: InvestigationStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Investigation {
    pub fn new() -> Self {
        let now = Utc::now();
        Self {
            id: Id::new(),
            court: String::new(),
            case_number: String::new(),
            assignment_date: None,
            due_date: None,
            allowed_days: None,
            parties: Vec::new(),
            subject: String::new(),
            expert_name: String::new(),
            expert_registry_no: String::new(),
            expertise_field: "56.12 – Ses Kayıtlarının Metin Hâline Dönüştürülmesi".to_string(),
            questions: Vec::new(),
            storage_key: String::new(),
            scope_notes: String::new(),
            status: InvestigationStatus::Draft,
            created_at: now,
            updated_at: now,
        }
    }

    /// Depolama klasörü adı olarak kullanılabilecek güvenli kimlik.
    ///
    /// Atanmış bir anahtar varsa o kullanılır; yoksa dosya numarasından
    /// türetilir. Dosya numarasındaki `/` ve benzeri karakterler dosya
    /// sisteminde klasör ayıracı olarak yorumlanmamalıdır.
    pub fn storage_key(&self) -> String {
        if !self.storage_key.is_empty() {
            return self.storage_key.clone();
        }
        sanitize_storage_key(&self.case_number, &self.id.0)
    }

    /// Depolama anahtarını bir kez atar.
    pub fn assign_storage_key(&mut self, key: impl Into<String>) {
        self.storage_key = key.into();
    }

    /// Rapor ve arayüzde gösterilecek kısa başlık.
    pub fn display_title(&self) -> String {
        if self.case_number.is_empty() {
            "İsimsiz inceleme".to_string()
        } else {
            format!("{} – {}", self.case_number, short(&self.subject, 60))
        }
    }

    /// Zorunlu alanlara göre eksik bilgi listesi.
    ///
    /// Bu liste rapor öncesi kontrol ve arayüz uyarıları için kullanılır.
    pub fn missing_required_fields(&self) -> Vec<&'static str> {
        let mut missing = Vec::new();
        if self.court.trim().is_empty() {
            missing.push("Görevlendiren merci");
        }
        if self.case_number.trim().is_empty() {
            missing.push("Dosya numarası");
        }
        if self.expert_name.trim().is_empty() {
            missing.push("Bilirkişi adı ve soyadı");
        }
        if self.expert_registry_no.trim().is_empty() {
            missing.push("Bilirkişi sicil numarası");
        }
        if self.subject.trim().is_empty() {
            missing.push("İnceleme konusu");
        }
        missing
    }

    pub fn is_complete(&self) -> bool {
        self.missing_required_fields().is_empty()
    }
}

impl Default for Investigation {
    fn default() -> Self {
        Self::new()
    }
}

/// İncelemede yer alan taraf.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Party {
    pub role: String,
    pub name: String,
}

/// Dosya numarasından güvenli bir depolama anahtarı üretir.
///
/// Yalnızca harf, rakam, `-` ve `_` karakterleri korunur. Boşluk ve
/// noktalama işaretleri `-` ile değiştirilir. Sonuç boşsa kimlik kullanılır.
pub fn sanitize_storage_key(case_number: &str, fallback_id: &str) -> String {
    let mut out = String::with_capacity(case_number.len());
    let mut last_dash = false;
    for ch in case_number.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' {
            out.push(ch);
            last_dash = ch == '-';
        } else if ch.is_alphanumeric() {
            out.push(ch);
            last_dash = false;
        } else if !last_dash {
            out.push('-');
            last_dash = true;
        }
    }
    let trimmed = out.trim_matches('-').to_string();
    if trimmed.is_empty() {
        // Kimliğin ilk 8 karakteri yeterli bir yedektir.
        fallback_id.chars().take(8).collect()
    } else {
        trimmed
    }
}

fn short(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
        out.push('…');
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn storage_key_strips_path_separators() {
        let key = sanitize_storage_key("2026/125", "fallback");
        assert_eq!(key, "2026-125");
        assert!(!key.contains('/'));
    }

    #[test]
    fn storage_key_falls_back_when_empty() {
        let key = sanitize_storage_key("   ", "abcdef123456");
        assert_eq!(key, "abcdef12");
    }

    #[test]
    fn storage_key_keeps_alphanumerics() {
        assert_eq!(sanitize_storage_key("2026-125", "x"), "2026-125");
        assert_eq!(sanitize_storage_key("A.B:C", "x"), "A-B-C");
    }

    #[test]
    fn assigned_storage_key_takes_precedence() {
        let mut inv = Investigation::new();
        inv.case_number = "2026/125".into();
        assert_eq!(inv.storage_key(), "2026-125");
        inv.assign_storage_key("2026-125-2");
        assert_eq!(inv.storage_key(), "2026-125-2");
    }

    #[test]
    fn missing_required_fields_detects_blanks() {
        let mut inv = Investigation::new();
        assert!(!inv.is_complete());
        inv.court = "Ankara 1. Asliye".into();
        inv.case_number = "2026/125".into();
        inv.expert_name = "A. Bilirkişi".into();
        inv.expert_registry_no = "12345".into();
        inv.subject = "Ses kaydı deşifresi".into();
        assert!(inv.is_complete());
        assert!(inv.missing_required_fields().is_empty());
    }
}
