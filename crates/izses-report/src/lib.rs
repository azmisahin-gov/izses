//! Bilirkişi raporu üretimi ve dışa aktarma.
//!
//! Rapor, inceleme verilerinden otomatik derlenir. Çıktı biçimleri:
//! DOCX (Word), PDF ve HTML. Otomatik oluşturulan bölümler ile bilirkişinin
//! kendi değerlendirmesi raporda ayırt edilebilir.

pub mod docx;
pub mod document;
pub mod html;
pub mod model;
pub mod pdf;

pub use document::{build_report, run_checks, ExportFormat, ReportError, ReportResult};
pub use model::{CheckSeverity, Report, ReportCheck, ReportInput, Section, Table, TranscriptBlock};
