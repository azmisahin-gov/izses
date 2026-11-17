//! PDF dışa aktarma.
//!
//! Bağımsız, kendi kendine yeten bir PDF yazıcısı. Türkçe karakterlerin doğru
//! görünmesi için gömülü bir TrueType yazı tipi kullanılır; PDF okuyucunun
//! sistem yazı tiplerine bağımlılığı kalmaz.
//!
//! Kodlama: Identity-H. İçerik akışında glif kimlikleri (CID = GID) yazılır.
//! `CIDToGIDMap` kimlik eşlemesi, `ToUnicode` tablosu glifleri Unicode
//! karakterlere geri eşler (kopyalama ve arama için).

use std::collections::BTreeMap;
use std::io::Write;
use std::path::Path;

use crate::document::ReportError;
use crate::model::Report;

/// A4 sayfa ölçüsü (punto).
const PAGE_WIDTH: f64 = 595.276;
const PAGE_HEIGHT: f64 = 841.890;
/// Kenar boşluğu (2 cm).
const MARGIN: f64 = 56.7;
const BODY_SIZE: f64 = 10.5;
const HEADING1_SIZE: f64 = 16.0;
const HEADING2_SIZE: f64 = 12.5;
const LINE_HEIGHT: f64 = 1.42;

/// Raporu PDF olarak yazar.
pub fn export_pdf(report: &Report, path: &Path) -> Result<(), ReportError> {
    let font_data = load_font_data()?;
    let mut builder = PdfBuilder::new(font_data)?;
    let bytes = builder.render(report)?;
    std::fs::write(path, bytes)?;
    Ok(())
}

/// Gömülecek yazı tipini bulur.
///
/// Kurulum paketiyle gelen yazı tipi önceliklidir; yoksa sistemdeki yaygın
/// bir Unicode yazı tipi kullanılır.
fn load_font_data() -> Result<Vec<u8>, ReportError> {
    let mut candidates: Vec<std::path::PathBuf> = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.join("fonts/DejaVuSans.ttf"));
            candidates.push(dir.join("resources/fonts/DejaVuSans.ttf"));
        }
    }
    candidates.extend(
        [
            "C:/Windows/Fonts/segoeui.ttf",
            "C:/Windows/Fonts/arial.ttf",
            "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
            "/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf",
            "/usr/share/fonts/truetype/freefont/FreeSans.ttf",
        ]
        .into_iter()
        .map(std::path::PathBuf::from),
    );

    for c in candidates {
        if c.is_file() {
            return std::fs::read(&c).map_err(ReportError::Io);
        }
    }
    Err(ReportError::Export(
        "PDF için gömülecek yazı tipi bulunamadı".to_string(),
    ))
}

struct PdfBuilder {
    font_data: Vec<u8>,
    cmap: BTreeMap<u16, u16>,
    /// Glif kimliği -> genişlik (1000 birim/em).
    widths: BTreeMap<u16, u16>,
    units_per_em: f64,
    ascender: f64,
    descender: f64,
    /// Kullanılan glif kimlikleri.
    used_gids: Vec<u16>,
    space_gid: u16,
}

impl PdfBuilder {
    fn new(font_data: Vec<u8>) -> Result<Self, ReportError> {
        let face = ttf_parser::Face::parse(&font_data, 0)
            .map_err(|e| ReportError::Export(format!("yazı tipi ayrıştırılamadı: {e}")))?;

        let units_per_em = face.units_per_em() as f64;
        let ascender = face.ascender() as f64;
        let descender = face.descender() as f64;

        let mut cmap: BTreeMap<u16, u16> = BTreeMap::new();
        if let Some(subtable) = face.tables().cmap {
            for sub in subtable.subtables {
                if !sub.is_unicode() {
                    continue;
                }
                sub.codepoints(|code| {
                    if code <= 0xFFFF {
                        if let Some(gid) = sub.glyph_index(code) {
                            cmap.entry(code as u16).or_insert(gid.0);
                        }
                    }
                });
            }
        }
        if cmap.is_empty() {
            return Err(ReportError::Export(
                "yazı tipinde Unicode eşlemesi bulunamadı".to_string(),
            ));
        }
        let space_gid = cmap.get(&(' ' as u16)).copied().unwrap_or(3);

        Ok(Self {
            font_data,
            cmap,
            widths: BTreeMap::new(),
            units_per_em,
            ascender,
            descender,
            used_gids: Vec::new(),
            space_gid,
        })
    }

    /// Bir metni glif kimliklerine çevirir ve genişlikleri kaydeder.
    fn encode(&mut self, text: &str) -> Vec<u16> {
        let face = ttf_parser::Face::parse(&self.font_data, 0).ok();
        let mut gids = Vec::with_capacity(text.len());
        for ch in text.chars() {
            let code = ch as u32;
            let gid = if code <= 0xFFFF {
                self.cmap.get(&(code as u16)).copied().unwrap_or(0)
            } else {
                0
            };
            if !self.widths.contains_key(&gid) {
                let advance = face
                    .as_ref()
                    .and_then(|f| f.glyph_hor_advance(ttf_parser::GlyphId(gid)))
                    .unwrap_or((self.units_per_em * 0.5) as u16);
                let w = ((advance as f64 / self.units_per_em) * 1000.0).round() as u16;
                self.widths.insert(gid, w);
            }
            if !self.used_gids.contains(&gid) {
                self.used_gids.push(gid);
            }
            gids.push(gid);
        }
        gids
    }

    fn width_of(&self, gids: &[u16], size: f64) -> f64 {
        let total: f64 = gids
            .iter()
            .map(|g| self.widths.get(g).copied().unwrap_or(500) as f64)
            .sum();
        total / 1000.0 * size
    }

    /// Metni satırlara böler.
    fn wrap(&mut self, text: &str, size: f64, max_width: f64) -> Vec<Vec<u16>> {
        let mut lines: Vec<Vec<u16>> = Vec::new();
        let mut current: Vec<u16> = Vec::new();
        let mut current_width = 0.0;

        for word in text.split_inclusive(' ') {
            let mut gids = self.encode(word);
            let w = self.width_of(&gids, size);
            if current_width + w > max_width && !current.is_empty() {
                while current.last() == Some(&self.space_gid) {
                    current.pop();
                }
                lines.push(std::mem::take(&mut current));
                current_width = 0.0;
                while gids.first() == Some(&self.space_gid) {
                    gids.remove(0);
                }
            }
            current_width += self.width_of(&gids, size);
            current.extend(gids);
        }
        if !current.is_empty() {
            lines.push(current);
        }
        if lines.is_empty() {
            lines.push(Vec::new());
        }
        lines
    }

    fn render(&mut self, report: &Report) -> Result<Vec<u8>, ReportError> {
        let max_width = PAGE_WIDTH - 2.0 * MARGIN;
        let mut pages: Vec<String> = Vec::new();
        let mut page = String::new();
        let mut cursor = Cursor {
            page: &mut page,
            pages: &mut pages,
            y: PAGE_HEIGHT - MARGIN,
            max_width,
        };

        cursor.heading(self, &report.title, HEADING1_SIZE);
        cursor.heading_text(
            self,
            &format!(
                "Rapor oluşturma zamanı: {}",
                izses_core::timefmt::format_datetime(&report.generated_at)
            ),
            BODY_SIZE,
        );
        cursor.gap(8.0);

        if !report.checks.is_empty() {
            cursor.heading(self, "Rapor Öncesi Tutarlılık Kontrolü", HEADING2_SIZE);
            for check in &report.checks {
                let line = format!("[{}] {}", check.severity.label_tr(), check.message);
                cursor.heading_text(self, &line, BODY_SIZE);
            }
            cursor.gap(6.0);
        }

        for section in report.all_sections() {
            cursor.heading(self, &section.heading, HEADING2_SIZE);
            for p in &section.paragraphs {
                cursor.heading_text(self, p, BODY_SIZE);
            }
            for b in &section.bullets {
                cursor.heading_text(self, &format!("• {b}"), BODY_SIZE);
            }
            if let Some(table) = &section.table {
                cursor.table(self, table);
            }
            cursor.gap(6.0);
        }

        if !report.transcript_blocks.is_empty() {
            cursor.heading(self, "16. Zaman Damgalı Deşifre Metinleri", HEADING2_SIZE);
            for block in &report.transcript_blocks {
                cursor.heading(
                    self,
                    &format!("{} ({})", block.file_name, block.source_label),
                    HEADING2_SIZE,
                );
                cursor.heading_text(
                    self,
                    &format!(
                        "Süre: {} · SHA-256: {}",
                        block.duration_label,
                        block.sha256.clone().unwrap_or_else(|| "—".into())
                    ),
                    BODY_SIZE,
                );
                for seg in &block.segments {
                    let text = format!(
                        "{}–{}  {}{}",
                        izses_core::timefmt::format_ms(seg.start_ms),
                        izses_core::timefmt::format_ms(seg.end_ms),
                        seg.speaker
                            .as_ref()
                            .map(|s| format!("[{s}] "))
                            .unwrap_or_default(),
                        seg.effective_text()
                    );
                    cursor.heading_text(self, &text, BODY_SIZE);
                }
                cursor.gap(4.0);
            }
        }

        cursor.heading(self, "Ekler", HEADING2_SIZE);
        cursor.heading_text(
            self,
            "Bu rapora ilişkin delil kaynakları, dosya özetleri ve işlem geçmişi \
             inceleme klasöründe saklanmaktadır.",
            BODY_SIZE,
        );

        let page = std::mem::take(cursor.page);
        pages.push(page);

        self.assemble(pages)
    }

    fn assemble(&mut self, pages: Vec<String>) -> Result<Vec<u8>, ReportError> {
        let mut gid_list = self.used_gids.clone();
        gid_list.sort_unstable();
        gid_list.dedup();
        let max_gid = gid_list.last().copied().unwrap_or(0);
        let cid_count = max_gid as usize + 1;

        let mut cid_to_gid = vec![0u8; cid_count * 2];
        for gid in &gid_list {
            let idx = (*gid as usize) * 2;
            cid_to_gid[idx] = (gid >> 8) as u8;
            cid_to_gid[idx + 1] = (gid & 0xFF) as u8;
        }

        // W dizisi: CID 0..max_gid aralığı için genişlikler.
        let mut width_values = String::new();
        for cid in 0..=max_gid {
            let w = self.widths.get(&cid).copied().unwrap_or(500);
            width_values.push_str(&format!("{w} "));
        }
        let w_array = format!("[0 [{width_values}]]");

        let scale = 1000.0 / self.units_per_em;
        let bbox = [
            0i64,
            (self.descender * scale).round() as i64,
            (self.units_per_em * 0.6 * scale).round() as i64,
            (self.ascender * scale).round() as i64,
        ];
        let ascent = (self.ascender * scale).round() as i64;
        let descent = (self.descender * scale).round() as i64;
        let cap_height = (self.ascender * 0.7 * scale).round() as i64;

        let page_count = pages.len();
        // 1 Catalog, 2 Pages, 3 Type0 Font, 4 CIDFont, 5 Descriptor,
        // 6 FontFile2, 7 ToUnicode, 8 CIDToGIDMap, 9+ sayfalar.
        let first_page_obj = 9usize;
        let content_objs: Vec<usize> = (0..page_count).map(|i| first_page_obj + i * 2 + 1).collect();

        let mut objects: Vec<Vec<u8>> = Vec::new();

        objects.push(b"<< /Type /Catalog /Pages 2 0 R >>".to_vec());

        let kids: String = (0..page_count)
            .map(|i| format!("{} 0 R", first_page_obj + i * 2))
            .collect::<Vec<_>>()
            .join(" ");
        objects.push(format!("<< /Type /Pages /Count {page_count} /Kids [{kids}] >>").into_bytes());

        objects.push(
            b"<< /Type /Font /Subtype /Type0 /BaseFont /IZSESEmbedded /Encoding /Identity-H \
              /DescendantFonts [4 0 R] /ToUnicode 7 0 R >>"
                .to_vec(),
        );

        objects.push(
            format!(
                "<< /Type /Font /Subtype /CIDFontType2 /BaseFont /IZSESEmbedded \
                 /CIDSystemInfo << /Registry (Adobe) /Ordering (Identity) /Supplement 0 >> \
                 /FontDescriptor 5 0 R /DW 500 /W {w_array} /CIDToGIDMap 8 0 R >>"
            )
            .into_bytes(),
        );

        objects.push(
            format!(
                "<< /Type /FontDescriptor /FontName /IZSESEmbedded /Flags 4 \
                 /FontBBox [{a} {b} {c} {d}] /ItalicAngle 0 /Ascent {ascent} /Descent {descent} \
                 /CapHeight {cap_height} /StemV 80 /FontFile2 6 0 R >>",
                a = bbox[0],
                b = bbox[1],
                c = bbox[2],
                d = bbox[3],
            )
            .into_bytes(),
        );

        let compressed_font = deflate(&self.font_data)?;
        let mut font_stream = format!(
            "<< /Length {} /Filter /FlateDecode /Length1 {} >>\nstream\n",
            compressed_font.len(),
            self.font_data.len()
        )
        .into_bytes();
        font_stream.extend_from_slice(&compressed_font);
        font_stream.extend_from_slice(b"\nendstream");
        objects.push(font_stream);

        let tounicode = build_tounicode(&self.cmap);
        objects.push(stream_object(&tounicode)?);

        objects.push(stream_object_bytes(&cid_to_gid)?);

        for (i, content) in pages.iter().enumerate() {
            let content_obj = content_objs[i];
            let page_dict = format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {PAGE_WIDTH:.3} {PAGE_HEIGHT:.3}] \
                 /Resources << /Font << /F1 3 0 R >> >> /Contents {content_obj} 0 R >>"
            );
            objects.push(page_dict.into_bytes());
            objects.push(stream_object(content)?);
        }

        write_pdf(objects)
    }
}

/// Sıkıştırılmış bir akış nesnesi üretir.
fn stream_object(content: &str) -> Result<Vec<u8>, ReportError> {
    stream_object_bytes(content.as_bytes())
}

fn stream_object_bytes(data: &[u8]) -> Result<Vec<u8>, ReportError> {
    let compressed = deflate(data)?;
    let mut out = format!(
        "<< /Length {} /Filter /FlateDecode >>\nstream\n",
        compressed.len()
    )
    .into_bytes();
    out.extend_from_slice(&compressed);
    out.extend_from_slice(b"\nendstream");
    Ok(out)
}

/// Sayfa yerleşim imleci.
struct Cursor<'a> {
    page: &'a mut String,
    pages: &'a mut Vec<String>,
    y: f64,
    max_width: f64,
}

impl Cursor<'_> {
    fn new_page(&mut self) {
        let page = std::mem::take(self.page);
        self.pages.push(page);
        self.y = PAGE_HEIGHT - MARGIN;
    }

    fn ensure(&mut self, needed: f64) {
        if self.y - needed < MARGIN {
            self.new_page();
        }
    }

    fn gap(&mut self, amount: f64) {
        self.y -= amount;
    }

    fn heading(&mut self, b: &mut PdfBuilder, text: &str, size: f64) {
        self.ensure(size * LINE_HEIGHT);
        for line in b.wrap(text, size, self.max_width) {
            self.ensure(size * LINE_HEIGHT);
            self.draw_glyphs(&line, size);
        }
        self.gap(2.0);
    }

    fn heading_text(&mut self, b: &mut PdfBuilder, text: &str, size: f64) {
        for line in b.wrap(text, size, self.max_width) {
            self.ensure(size * LINE_HEIGHT);
            self.draw_glyphs(&line, size);
        }
    }

    fn draw_glyphs(&mut self, gids: &[u16], size: f64) {
        if !gids.is_empty() {
            let hex: String = gids.iter().map(|g| format!("{g:04X}")).collect();
            self.page.push_str(&format!(
                "BT /F1 {size} Tf 1 0 0 1 {x:.2} {y:.2} Tm <{hex}> Tj ET\n",
                x = MARGIN,
                y = self.y - size,
            ));
        }
        self.y -= size * LINE_HEIGHT;
    }

    fn table(&mut self, b: &mut PdfBuilder, table: &crate::model::Table) {
        let cols = table.headers.len().max(1);
        let col_width = self.max_width / cols as f64;
        let row_h = BODY_SIZE * LINE_HEIGHT;

        let rows: Vec<&Vec<String>> = std::iter::once(&table.headers)
            .chain(table.rows.iter())
            .collect();

        for cells in rows {
            self.ensure(row_h);
            let baseline = self.y - BODY_SIZE;
            for (i, cell) in cells.iter().take(cols).enumerate() {
                let x = MARGIN + i as f64 * col_width;
                let mut gids = b.encode(cell);
                while b.width_of(&gids, BODY_SIZE) > col_width - 6.0 && !gids.is_empty() {
                    gids.pop();
                }
                if !gids.is_empty() {
                    let hex: String = gids.iter().map(|g| format!("{g:04X}")).collect();
                    self.page.push_str(&format!(
                        "BT /F1 {BODY_SIZE} Tf 1 0 0 1 {x:.2} {baseline:.2} Tm <{hex}> Tj ET\n"
                    ));
                }
            }
            self.y -= row_h;
        }
        self.gap(4.0);
    }
}

/// Glif kimliklerini Unicode'a geri eşleyen ToUnicode tablosu.
fn build_tounicode(cmap: &BTreeMap<u16, u16>) -> String {
    let mut gid_to_unicode: BTreeMap<u16, u16> = BTreeMap::new();
    for (code, gid) in cmap {
        gid_to_unicode.entry(*gid).or_insert(*code);
    }

    let mut body = String::new();
    for (gid, code) in &gid_to_unicode {
        body.push_str(&format!("<{gid:04X}> <{code:04X}>\n"));
    }

    format!(
        "/CIDInit /ProcSet findresource begin\n\
         12 dict begin\n\
         begincmap\n\
         /CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def\n\
         /CMapName /Adobe-Identity-UCS def\n\
         /CMapType 2 def\n\
         1 begincodespacerange\n\
         <0000> <FFFF>\n\
         endcodespacerange\n\
         {} beginbfchar\n\
         {}endbfchar\n\
         endcmap\n\
         CMapName currentdict /CMap defineresource pop\n\
         end\n\
         end",
        gid_to_unicode.len(),
        body
    )
}

fn deflate(data: &[u8]) -> Result<Vec<u8>, ReportError> {
    use flate2::write::ZlibEncoder;
    use flate2::Compression;
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
    encoder
        .write_all(data)
        .map_err(|e| ReportError::Export(format!("sıkıştırma hatası: {e}")))?;
    encoder
        .finish()
        .map_err(|e| ReportError::Export(format!("sıkıştırma hatası: {e}")))
}

fn write_pdf(objects: Vec<Vec<u8>>) -> Result<Vec<u8>, ReportError> {
    let mut out: Vec<u8> = Vec::new();
    out.extend_from_slice(b"%PDF-1.7\n%\xE2\xE3\xCF\xD3\n");

    let mut offsets: Vec<usize> = vec![0];
    for (i, obj) in objects.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n", i + 1).as_bytes());
        out.extend_from_slice(obj);
        out.extend_from_slice(b"\nendobj\n");
    }

    let xref_offset = out.len();
    let count = objects.len() + 1;
    out.extend_from_slice(format!("xref\n0 {count}\n").as_bytes());
    out.extend_from_slice(b"0000000000 65535 f \n");
    for off in offsets.iter().skip(1) {
        out.extend_from_slice(format!("{off:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(
        format!("trailer\n<< /Size {count} /Root 1 0 R >>\nstartxref\n{xref_offset}\n%%EOF\n")
            .as_bytes(),
    );
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::build_report;
    use crate::model::ReportInput;
    use izses_core::model::Investigation;

    fn sample_report() -> Report {
        let mut inv = Investigation::new();
        inv.court = "Ankara 1. Asliye Ceza Mahkemesi".into();
        inv.case_number = "2026/125".into();
        inv.subject = "Ses kaydının çözümlenmesi çğıöşü".into();
        inv.expert_name = "A. Bilirkişi".into();
        inv.expert_registry_no = "12345".into();
        let input = ReportInput {
            investigation: &inv,
            sources: &[],
            files: &[],
            transcripts: Vec::new(),
            notes: &[],
            expert_assessment: "Değerlendirme".into(),
            conclusion: "Sonuç".into(),
            model_name: "Whisper large-v3-turbo".into(),
            model_revision: "main".into(),
            method_notes: String::new(),
            history_lines: Vec::new(),
        };
        build_report(&input).unwrap()
    }

    #[test]
    fn produces_valid_pdf_structure() {
        let report = sample_report();
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("rapor.pdf");
        if export_pdf(&report, &path).is_err() {
            return; // yazı tipi yoksa atla
        }
        let bytes = std::fs::read(&path).unwrap();
        assert!(bytes.starts_with(b"%PDF-1.7"));
        assert!(bytes.ends_with(b"%%EOF\n"));
        let text = String::from_utf8_lossy(&bytes);
        assert!(text.contains("/Type /Catalog"));
        assert!(text.contains("/Type /Page"));
        assert!(text.contains("/FontFile2"));
        assert!(text.contains("/ToUnicode"));
        assert!(text.contains("/CIDToGIDMap"));
        assert!(text.contains("startxref"));
    }

    #[test]
    fn font_parsing_and_cmap_work() {
        let font = match load_font_data() {
            Ok(f) => f,
            Err(_) => return,
        };
        let builder = PdfBuilder::new(font).unwrap();
        assert!(!builder.cmap.is_empty());
        assert!(builder.cmap.contains_key(&('ş' as u16)));
        assert!(builder.cmap.contains_key(&('İ' as u16)));
    }

    #[test]
    fn encoding_maps_turkish_characters_to_glyphs() {
        let font = match load_font_data() {
            Ok(f) => f,
            Err(_) => return,
        };
        let mut builder = PdfBuilder::new(font).unwrap();
        let gids = builder.encode("çğıöşü ÇĞİÖŞÜ");
        assert!(gids.iter().all(|&g| g != 0), "glifsiz karakter var: {gids:?}");
    }
}
