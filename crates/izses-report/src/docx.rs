//! DOCX (Office Open XML) dışa aktarma.
//!
//! DOCX bir ZIP kapsayıcısıdır. Belge, Word tarafından açılabilmesi için
//! gerekli asgari parçaları içerir: içerik türleri, ilişkiler, ana belge,
//! stiller ve üst veri.
//!
//! Otomatik oluşturulan bölümler ile bilirkişi tarafından yazılan bölümler
//! görsel olarak ayırt edilir (bilirkişi bölümleri italik olarak işaretlenir).

use std::io::Write;
use std::path::Path;

use crate::document::ReportError;
use crate::model::{Report, Section};

/// Raporu DOCX olarak yazar.
pub fn export_docx(report: &Report, path: &Path) -> Result<(), ReportError> {
    let file = std::fs::File::create(path)?;
    let mut zip = zip::ZipWriter::new(file);
    let options: zip::write::FileOptions<'_, ()> =
        zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    write_entry(&mut zip, "[Content_Types].xml", CONTENT_TYPES, options)?;
    write_entry(&mut zip, "_rels/.rels", ROOT_RELS, options)?;
    write_entry(&mut zip, "word/_rels/document.xml.rels", DOC_RELS, options)?;
    write_entry(&mut zip, "word/styles.xml", STYLES_XML, options)?;
    write_entry(&mut zip, "docProps/core.xml", &core_props(report), options)?;

    let document = build_document_xml(report);
    write_entry(&mut zip, "word/document.xml", &document, options)?;

    zip.finish()
        .map_err(|e| ReportError::Export(format!("DOCX paketlenemedi: {e}")))?;
    Ok(())
}

fn write_entry<W: Write + std::io::Seek>(
    zip: &mut zip::ZipWriter<W>,
    name: &str,
    content: &str,
    options: zip::write::FileOptions<'_, ()>,
) -> Result<(), ReportError> {
    zip.start_file(name, options)
        .map_err(|e| ReportError::Export(format!("DOCX girdisi yazılamadı: {e}")))?;
    zip.write_all(content.as_bytes())?;
    Ok(())
}

const CONTENT_TYPES: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
  <Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
  <Default Extension="xml" ContentType="application/xml"/>
  <Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>
  <Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/>
  <Override PartName="/docProps/core.xml" ContentType="application/vnd.openxmlformats-package.core-properties+xml"/>
</Types>"#;

const ROOT_RELS: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>
  <Relationship Id="rId2" Type="http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties" Target="docProps/core.xml"/>
</Relationships>"#;

const DOC_RELS: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/>
</Relationships>"#;

const STYLES_XML: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:docDefaults>
    <w:rPrDefault>
      <w:rPr>
        <w:rFonts w:ascii="Calibri" w:hAnsi="Calibri" w:cs="Calibri"/>
        <w:sz w:val="22"/>
      </w:rPr>
    </w:rPrDefault>
  </w:docDefaults>
  <w:style w:type="paragraph" w:styleId="Normal" w:default="1">
    <w:name w:val="Normal"/>
  </w:style>
  <w:style w:type="paragraph" w:styleId="Heading1">
    <w:name w:val="heading 1"/>
    <w:pPr><w:spacing w:before="240" w:after="120"/></w:pPr>
    <w:rPr><w:b/><w:sz w:val="28"/></w:rPr>
  </w:style>
  <w:style w:type="paragraph" w:styleId="Heading2">
    <w:name w:val="heading 2"/>
    <w:pPr><w:spacing w:before="200" w:after="100"/></w:pPr>
    <w:rPr><w:b/><w:sz w:val="24"/></w:rPr>
  </w:style>
  <w:style w:type="paragraph" w:styleId="ExpertBlock">
    <w:name w:val="Expert Block"/>
    <w:rPr><w:i/></w:rPr>
  </w:style>
</w:styles>"#;

fn core_props(report: &Report) -> String {
    let created = report.generated_at.to_rfc3339();
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<cp:coreProperties xmlns:cp="http://schemas.openxmlformats.org/package/2006/metadata/core-properties" xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:dcterms="http://purl.org/dc/terms/" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance">
  <dc:title>{}</dc:title>
  <dc:creator>{}</dc:creator>
  <cp:lastModifiedBy>İZSES</cp:lastModifiedBy>
  <dcterms:created xsi:type="dcterms:W3CDTF">{}</dcterms:created>
</cp:coreProperties>"#,
        xml_escape(&report.title),
        xml_escape(&report.investigation.expert_name),
        created
    )
}

fn build_document_xml(report: &Report) -> String {
    let mut body = String::new();

    // Başlık.
    body.push_str(&heading_paragraph(&report.title, 1));

    // Rapor üst verisi.
    body.push_str(&paragraph(&format!(
        "Rapor oluşturma zamanı: {}",
        izses_core::timefmt::format_datetime(&report.generated_at)
    )));

    // Tutarlılık kontrolü özeti.
    if !report.checks.is_empty() {
        body.push_str(&heading_paragraph("Rapor Öncesi Tutarlılık Kontrolü", 2));
        for check in &report.checks {
            body.push_str(&paragraph(&format!(
                "[{}] {}",
                check.severity.label_tr(),
                check.message
            )));
        }
    }

    // Bölümler.
    for section in report.all_sections() {
        body.push_str(&section_xml(section));
    }

    // Deşifre metinleri.
    if !report.transcript_blocks.is_empty() {
        body.push_str(&heading_paragraph("16. Zaman Damgalı Deşifre Metinleri", 2));
        for block in &report.transcript_blocks {
            body.push_str(&heading_paragraph(
                &format!("{} ({})", block.file_name, block.source_label),
                2,
            ));
            body.push_str(&paragraph(&format!(
                "Süre: {} · SHA-256: {}",
                block.duration_label,
                block.sha256.clone().unwrap_or_else(|| "—".into())
            )));
            for seg in &block.segments {
                let marker = if seg.is_edited() {
                    " (düzeltildi)"
                } else {
                    ""
                };
                let speaker = seg
                    .speaker
                    .as_ref()
                    .map(|s| format!("[{s}] "))
                    .unwrap_or_default();
                body.push_str(&paragraph(&format!(
                    "{}–{}  {}{}{}",
                    izses_core::timefmt::format_ms(seg.start_ms),
                    izses_core::timefmt::format_ms(seg.end_ms),
                    speaker,
                    seg.effective_text(),
                    marker
                )));
                if let Some(obs) = &seg.observation {
                    body.push_str(&expert_paragraph(&format!("Gözlem: {obs}")));
                }
            }
        }
    }

    // Ek: işlem geçmişi ayrıntısı.
    body.push_str(&heading_paragraph("Ekler", 2));
    body.push_str(&paragraph(
        "Bu rapora ilişkin delil kaynakları, dosya özetleri ve işlem geçmişi \
         inceleme klasöründe saklanmaktadır.",
    ));

    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
{}
    <w:sectPr>
      <w:pgSz w:w="11906" w:h="16838"/>
      <w:pgMar w:top="1134" w:right="1134" w:bottom="1134" w:left="1134"/>
    </w:sectPr>
  </w:body>
</w:document>"#,
        body
    )
}

fn section_xml(section: &Section) -> String {
    let mut out = heading_paragraph(&section.heading, 2);
    for p in &section.paragraphs {
        if section.authored_by_expert {
            out.push_str(&expert_paragraph(p));
        } else {
            out.push_str(&paragraph(p));
        }
    }
    for b in &section.bullets {
        if section.authored_by_expert {
            out.push_str(&expert_paragraph(&format!("• {b}")));
        } else {
            out.push_str(&paragraph(&format!("• {b}")));
        }
    }
    if let Some(table) = &section.table {
        out.push_str(&table_xml(table));
    }
    out
}

fn table_xml(table: &crate::model::Table) -> String {
    let mut out = String::new();
    out.push_str("<w:tbl><w:tblPr><w:tblBorders>");
    for edge in ["top", "left", "bottom", "right", "insideH", "insideV"] {
        out.push_str(&format!(
            r#"<w:{edge} w:val="single" w:sz="4" w:space="0" w:color="808080"/>"#
        ));
    }
    out.push_str("</w:tblBorders></w:tblPr>");
    out.push_str("<w:tr>");
    for h in &table.headers {
        out.push_str(&cell_xml(h, true));
    }
    out.push_str("</w:tr>");
    for row in &table.rows {
        out.push_str("<w:tr>");
        for c in row {
            out.push_str(&cell_xml(c, false));
        }
        out.push_str("</w:tr>");
    }
    out.push_str("</w:tbl>");
    out.push_str(&paragraph(""));
    out
}

fn cell_xml(text: &str, bold: bool) -> String {
    let rpr = if bold { "<w:rPr><w:b/></w:rPr>" } else { "" };
    format!(
        r#"<w:tc><w:tcPr><w:tcW w:w="0" w:type="auto"/></w:tcPr><w:p><w:r>{rpr}<w:t xml:space="preserve">{}</w:t></w:r></w:p></w:tc>"#,
        xml_escape(text)
    )
}

fn heading_paragraph(text: &str, level: u8) -> String {
    let style = match level {
        1 => "Heading1",
        _ => "Heading2",
    };
    format!(
        r#"<w:p><w:pPr><w:pStyle w:val="{style}"/></w:pPr><w:r><w:t xml:space="preserve">{}</w:t></w:r></w:p>"#,
        xml_escape(text)
    )
}

fn paragraph(text: &str) -> String {
    format!(
        r#"<w:p><w:r><w:t xml:space="preserve">{}</w:t></w:r></w:p>"#,
        xml_escape(text)
    )
}

/// Bilirkişi tarafından yazılmış bölüm (italik).
fn expert_paragraph(text: &str) -> String {
    format!(
        r#"<w:p><w:pPr><w:pStyle w:val="ExpertBlock"/></w:pPr><w:r><w:rPr><w:i/></w:rPr><w:t xml:space="preserve">{}</w:t></w:r></w:p>"#,
        xml_escape(text)
    )
}

/// XML için özel karakterleri kaçırır.
pub fn xml_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            // XML 1.0'da izin verilmeyen denetim karakterlerini at.
            c if (c as u32) < 0x20 && c != '\t' && c != '\n' && c != '\r' => out.push(' '),
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_xml_specials() {
        assert_eq!(xml_escape("a<b>&\"'"), "a&lt;b&gt;&amp;&quot;&apos;");
        assert_eq!(xml_escape("türkçe çğıöşü"), "türkçe çğıöşü");
    }

    #[test]
    fn docx_package_is_valid_zip_with_required_parts() {
        use crate::document::build_report;
        use crate::model::ReportInput;
        use izses_core::model::Investigation;

        let inv = Investigation::new();
        let input = ReportInput {
            investigation: &inv,
            sources: &[],
            files: &[],
            transcripts: Vec::new(),
            notes: &[],
            expert_assessment: String::new(),
            conclusion: String::new(),
            model_name: "Whisper".into(),
            model_revision: "main".into(),
            method_notes: String::new(),
            history_lines: Vec::new(),
        };
        let report = build_report(&input).unwrap();

        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("rapor.docx");
        export_docx(&report, &path).unwrap();

        // ZIP açılıp gerekli parçalar okunabilmeli.
        let file = std::fs::File::open(&path).unwrap();
        let mut archive = zip::ZipArchive::new(file).unwrap();
        for required in [
            "[Content_Types].xml",
            "_rels/.rels",
            "word/document.xml",
            "word/styles.xml",
            "word/_rels/document.xml.rels",
        ] {
            assert!(archive.by_name(required).is_ok(), "eksik parça: {required}");
        }

        // document.xml geçerli XML olmalı ve başlığı içermeli.
        let mut doc = archive.by_name("word/document.xml").unwrap();
        let mut content = String::new();
        std::io::Read::read_to_string(&mut doc, &mut content).unwrap();
        assert!(content.contains("<w:document"));
        assert!(content.contains("BİLİRKİŞİ RAPORU"));
    }
}
