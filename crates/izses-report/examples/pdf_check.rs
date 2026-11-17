//! Geliştirme amaçlı örnek: PDF ve DOCX üretip diske yazar.
use izses_report::{
    build_report, docx::export_docx, html::export_html, pdf::export_pdf, ReportInput,
};

fn main() {
    let mut inv = izses_core::model::Investigation::new();
    inv.court = "Ankara 1. Asliye Ceza Mahkemesi".into();
    inv.case_number = "2026/125".into();
    inv.subject = "Ses kaydı çözümlemesi çğıöşü İI".into();
    inv.expert_name = "A. Bilirkişi".into();
    inv.expert_registry_no = "12345".into();
    let input = ReportInput {
        investigation: &inv,
        sources: &[],
        files: &[],
        transcripts: Vec::new(),
        notes: &[],
        expert_assessment: "Değerlendirme çğıöşü".into(),
        conclusion: "Sonuç İI".into(),
        model_name: "Whisper large-v3-turbo".into(),
        model_revision: "main".into(),
        method_notes: String::new(),
        history_lines: Vec::new(),
    };
    let report = build_report(&input).unwrap();
    export_pdf(&report, std::path::Path::new("/tmp/report.pdf")).unwrap();
    export_docx(&report, std::path::Path::new("/tmp/report.docx")).unwrap();
    export_html(&report, std::path::Path::new("/tmp/report.html")).unwrap();
    println!("yazıldı");
}
