//! HTML dışa aktarma (rapor önizlemesi ve arşivleme için).
//!
//! Çıktı tek dosyadır ve tarayıcı dışında hiçbir kaynağa ihtiyaç duymaz.

use std::path::Path;

use crate::document::ReportError;
use crate::docx::xml_escape;
use crate::model::{Report, Section};

/// Raporu HTML olarak yazar.
pub fn export_html(report: &Report, path: &Path) -> Result<(), ReportError> {
    let html = render_html(report);
    std::fs::write(path, html)?;
    Ok(())
}

/// Raporu HTML metnine çevirir (arayüzde önizleme için de kullanılır).
pub fn render_html(report: &Report) -> String {
    let mut body = String::new();
    body.push_str(&format!("<h1>{}</h1>", xml_escape(&report.title)));
    body.push_str(&format!(
        "<p class=\"meta\">Rapor oluşturma zamanı: {}</p>",
        izses_core::timefmt::format_datetime(&report.generated_at)
    ));

    if !report.checks.is_empty() {
        body.push_str("<h2>Rapor Öncesi Tutarlılık Kontrolü</h2><ul>");
        for check in &report.checks {
            body.push_str(&format!(
                "<li class=\"check {}\"><strong>[{}]</strong> {}</li>",
                severity_class(check),
                check.severity.label_tr(),
                xml_escape(&check.message)
            ));
        }
        body.push_str("</ul>");
    }

    for section in report.all_sections() {
        body.push_str(&section_html(section));
    }

    if !report.transcript_blocks.is_empty() {
        body.push_str("<h2>16. Zaman Damgalı Deşifre Metinleri</h2>");
        for block in &report.transcript_blocks {
            body.push_str(&format!(
                "<h3>{} <span class=\"src\">({})</span></h3>",
                xml_escape(&block.file_name),
                xml_escape(&block.source_label)
            ));
            body.push_str(&format!(
                "<p class=\"meta\">Süre: {} · SHA-256: <code>{}</code></p>",
                xml_escape(&block.duration_label),
                xml_escape(block.sha256.as_deref().unwrap_or("—"))
            ));
            body.push_str("<table class=\"segments\"><thead><tr><th>Başlangıç</th><th>Bitiş</th><th>Konuşmacı</th><th>Metin</th><th>Durum</th></tr></thead><tbody>");
            for seg in &block.segments {
                let status = if seg.is_edited() {
                    "Düzeltildi"
                } else {
                    "Otomatik"
                };
                let flags = flag_summary(seg);
                body.push_str(&format!(
                    "<tr class=\"{}\"><td>{}</td><td>{}</td><td>{}</td><td>{}{}</td><td>{}</td></tr>",
                    if seg.has_attention_flag() { "flagged" } else { "" },
                    izses_core::timefmt::format_ms(seg.start_ms),
                    izses_core::timefmt::format_ms(seg.end_ms),
                    xml_escape(seg.speaker.as_deref().unwrap_or("—")),
                    xml_escape(seg.effective_text()),
                    flags,
                    status
                ));
            }
            body.push_str("</tbody></table>");
        }
    }

    body.push_str("<h2>Ekler</h2><p>Bu rapora ilişkin delil kaynakları, dosya özetleri ve işlem geçmişi inceleme klasöründe saklanmaktadır.</p>");

    format!(
        r#"<!DOCTYPE html>
<html lang="tr">
<head>
<meta charset="utf-8">
<title>{title}</title>
<style>
:root {{ color-scheme: light; }}
body {{ font-family: "Segoe UI", Calibri, Arial, sans-serif; max-width: 900px; margin: 2rem auto; padding: 0 1rem; color: #1a1a1a; line-height: 1.6; }}
h1 {{ font-size: 1.6rem; border-bottom: 2px solid #333; padding-bottom: .4rem; }}
h2 {{ font-size: 1.2rem; margin-top: 1.6rem; }}
h3 {{ font-size: 1.05rem; }}
.meta {{ color: #555; font-size: .9rem; }}
.src {{ font-weight: normal; color: #666; font-size: .9rem; }}
.expert {{ font-style: italic; background: #f7f7f9; border-left: 3px solid #8a8a99; padding: .5rem .8rem; }}
table {{ border-collapse: collapse; width: 100%; margin: .8rem 0; font-size: .9rem; }}
th, td {{ border: 1px solid #c8c8c8; padding: .35rem .5rem; text-align: left; vertical-align: top; }}
th {{ background: #eef0f2; }}
tr.flagged {{ background: #fff6e5; }}
.check.blocker {{ color: #a11212; }}
.check.warning {{ color: #8a5a00; }}
code {{ background: #f2f2f2; padding: 0 .25rem; }}
</style>
</head>
<body>
{body}
</body>
</html>"#,
        title = xml_escape(&report.title),
        body = body
    )
}

fn section_html(section: &Section) -> String {
    let class = if section.authored_by_expert {
        "expert"
    } else {
        ""
    };
    let mut out = format!("<h2>{}</h2>", xml_escape(&section.heading));
    for p in &section.paragraphs {
        out.push_str(&format!("<p class=\"{class}\">{}</p>", xml_escape(p)));
    }
    if !section.bullets.is_empty() {
        out.push_str(&format!("<ul class=\"{class}\">"));
        for b in &section.bullets {
            out.push_str(&format!("<li>{}</li>", xml_escape(b)));
        }
        out.push_str("</ul>");
    }
    if let Some(table) = &section.table {
        out.push_str("<table><thead><tr>");
        for h in &table.headers {
            out.push_str(&format!("<th>{}</th>", xml_escape(h)));
        }
        out.push_str("</tr></thead><tbody>");
        for row in &table.rows {
            out.push_str("<tr>");
            for c in row {
                out.push_str(&format!("<td>{}</td>", xml_escape(c)));
            }
            out.push_str("</tr>");
        }
        out.push_str("</tbody></table>");
    }
    out
}

fn severity_class(check: &crate::model::ReportCheck) -> &'static str {
    use crate::model::CheckSeverity::*;
    match check.severity {
        Blocker => "blocker",
        Warning => "warning",
        Info => "info",
    }
}

fn flag_summary(seg: &izses_core::transcript::TranscriptSegment) -> String {
    let mut tags = Vec::new();
    if seg.flags.unclear {
        tags.push("anlaşılamadı");
    }
    if seg.flags.relisten {
        tags.push("yeniden dinlenmeli");
    }
    if seg.flags.noise {
        tags.push("gürültü");
    }
    if seg.flags.overlap {
        tags.push("üst üste konuşma");
    }
    if tags.is_empty() {
        String::new()
    } else {
        format!(" <span class=\"flags\">[{}]</span>", tags.join(", "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::build_report;
    use crate::model::ReportInput;
    use izses_core::model::Investigation;

    #[test]
    fn renders_html_with_title_and_sections() {
        let inv = Investigation::new();
        let input = ReportInput {
            investigation: &inv,
            sources: &[],
            files: &[],
            transcripts: Vec::new(),
            notes: &[],
            expert_assessment: "Değerlendirme metni".into(),
            conclusion: "Sonuç metni".into(),
            model_name: "Whisper".into(),
            model_revision: "main".into(),
            method_notes: String::new(),
            history_lines: Vec::new(),
        };
        let report = build_report(&input).unwrap();
        let html = render_html(&report);
        assert!(html.contains("<!DOCTYPE html>"));
        assert!(html.contains("BİLİRKİŞİ RAPORU"));
        assert!(html.contains("Değerlendirme metni"));
        assert!(html.contains("Gerekçeli Sonuç"));
        assert!(html.contains("lang=\"tr\""));
    }
}
