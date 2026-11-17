//! Rapor derleme ve dışa aktarma.
//!
//! Rapor, inceleme boyunca girilen bilgilerden otomatik derlenir. Yapılmayan
//! işlemler rapora yapılmış gibi eklenmez; eksik ve doğrulanmamış işler
//! tutarlılık kontrolünde açıkça gösterilir.

use izses_core::inventory::{EvidenceFile, VerificationState};
use izses_core::timefmt;
use izses_core::transcript::NoteKind;

use crate::model::{
    CheckSeverity, Report, ReportCheck, ReportInput, Section, Table, TranscriptBlock,
};

/// Rapor derleme sırasında oluşan hata.
#[derive(Debug, thiserror::Error)]
pub enum ReportError {
    #[error("giriş/çıkış hatası: {0}")]
    Io(#[from] std::io::Error),

    #[error("rapor derlenemedi: {0}")]
    Build(String),

    #[error("dışa aktarma hatası: {0}")]
    Export(String),
}

pub type ReportResult<T> = Result<T, ReportError>;

/// Raporda kullanılacak dışa aktarma biçimi.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    Docx,
    Pdf,
    Html,
}

impl ExportFormat {
    pub fn extension(self) -> &'static str {
        match self {
            Self::Docx => "docx",
            Self::Pdf => "pdf",
            Self::Html => "html",
        }
    }

    pub fn label_tr(self) -> &'static str {
        match self {
            Self::Docx => "Word belgesi (DOCX)",
            Self::Pdf => "PDF belgesi",
            Self::Html => "HTML belgesi",
        }
    }
}

/// Girdiden raporu derler.
pub fn build_report(input: &ReportInput<'_>) -> ReportResult<Report> {
    let inv = input.investigation;
    let title = format!(
        "BİLİRKİŞİ RAPORU – {}",
        if inv.case_number.is_empty() {
            "Dosya numarası girilmedi".to_string()
        } else {
            inv.case_number.clone()
        }
    );

    let mut sections = Vec::new();
    sections.push(section_assignment(inv));
    sections.push(section_expert(inv));
    sections.push(section_subject(input));
    sections.push(section_questions(inv));
    sections.push(section_sources(input));
    sections.push(section_inventory(input));
    sections.push(section_integrity(input));
    sections.push(section_method(input));
    sections.push(section_model(input));
    sections.push(section_expert_edits(input));
    sections.push(section_flagged(input));
    sections.push(section_observations(input));
    sections.push(section_history(input));
    sections.push(section_expert_assessment(input));
    sections.push(section_conclusion(input));

    let transcript_blocks = build_transcript_blocks(input);
    let checks = run_checks(input);

    Ok(Report {
        title,
        generated_at: chrono::Utc::now(),
        investigation: inv.clone(),
        sections,
        transcript_blocks,
        checks,
    })
}

fn section_assignment(inv: &izses_core::model::Investigation) -> Section {
    let mut s = Section::new("1. Görevlendiren Merci ve Dosya Bilgileri");
    s.paragraphs.push(format!("Görevlendiren merci: {}", dash(&inv.court)));
    s.paragraphs
        .push(format!("Dosya / soruşturma numarası: {}", dash(&inv.case_number)));
    s.paragraphs.push(format!(
        "Görevlendirme tarihi: {}",
        inv.assignment_date
            .map(|d| d.format("%d.%m.%Y").to_string())
            .unwrap_or_else(|| "—".to_string())
    ));
    s.paragraphs.push(format!(
        "Son teslim tarihi: {}",
        inv.due_date
            .map(|d| d.format("%d.%m.%Y").to_string())
            .unwrap_or_else(|| "—".to_string())
    ));
    if let Some(days) = inv.allowed_days {
        s.paragraphs.push(format!("Verilen süre: {days} gün"));
    }
    if !inv.parties.is_empty() {
        let list: Vec<String> = inv
            .parties
            .iter()
            .map(|p| format!("{}: {}", p.role, p.name))
            .collect();
        s.bullets.push(format!("Taraflar: {}", list.join("; ")));
    }
    s
}

fn section_expert(inv: &izses_core::model::Investigation) -> Section {
    let mut s = Section::new("2. Bilirkişi Bilgileri");
    s.paragraphs
        .push(format!("Adı ve soyadı: {}", dash(&inv.expert_name)));
    s.paragraphs
        .push(format!("Sicil numarası: {}", dash(&inv.expert_registry_no)));
    s.paragraphs
        .push(format!("Uzmanlık alanı: {}", dash(&inv.expertise_field)));
    s
}

fn section_subject(input: &ReportInput<'_>) -> Section {
    let inv = input.investigation;
    let mut s = Section::new("3. İncelemenin Konusu ve Kapsamı");
    s.paragraphs
        .push(format!("İnceleme konusu: {}", dash(&inv.subject)));
    if !inv.scope_notes.trim().is_empty() {
        s.paragraphs.push(inv.scope_notes.clone());
    }
    s
}

fn section_questions(inv: &izses_core::model::Investigation) -> Section {
    let mut s = Section::new("4. Bilirkişiye Yöneltilen Sorular");
    if inv.questions.is_empty() {
        s.paragraphs
            .push("Görevlendirme yazısında ayrıca soru belirtilmemiştir.".to_string());
    } else {
        for (i, q) in inv.questions.iter().enumerate() {
            s.bullets.push(format!("{}. {}", i + 1, q));
        }
    }
    s
}

fn section_sources(input: &ReportInput<'_>) -> Section {
    let mut s = Section::new("5. İncelemeye Alınan Delil Kaynakları");
    if input.sources.is_empty() {
        s.paragraphs
            .push("İncelemeye alınmış delil kaynağı bulunmamaktadır.".to_string());
        return s;
    }
    let mut table = Table {
        headers: vec![
            "Sıra".into(),
            "Kaynak".into(),
            "Tür".into(),
            "Dosya Sistemi".into(),
            "Kapasite".into(),
        ],
        rows: Vec::new(),
    };
    for src in input.sources {
        table.rows.push(vec![
            format!("{:03}", src.ordinal),
            src.label.clone(),
            src.kind.label_tr().to_string(),
            src.filesystem.clone().unwrap_or_else(|| "—".into()),
            src.total_bytes
                .map(izses_core::device::format_size)
                .unwrap_or_else(|| "—".into()),
        ]);
    }
    s.table = Some(table);
    s
}

fn section_inventory(input: &ReportInput<'_>) -> Section {
    let mut s = Section::new("6. Ses Dosyalarının Teknik Envanteri");
    let audio: Vec<&EvidenceFile> = input.files.iter().filter(|f| f.is_audio()).collect();
    if audio.is_empty() {
        s.paragraphs
            .push("İncelemeye alınmış ses dosyası bulunmamaktadır.".to_string());
        return s;
    }
    let mut table = Table {
        headers: vec![
            "Dosya".into(),
            "Biçim".into(),
            "Süre".into(),
            "Kanal".into(),
            "Örnekleme".into(),
            "Bit".into(),
            "Boyut".into(),
        ],
        rows: Vec::new(),
    };
    for f in &audio {
        let info = f.audio.clone().unwrap_or_default();
        table.rows.push(vec![
            f.file_name(),
            info.container.clone().unwrap_or_else(|| "—".into()),
            info.duration_label(),
            info.channels
                .map(|c| c.to_string())
                .unwrap_or_else(|| "—".into()),
            info.sample_rate
                .map(|r| format!("{r} Hz"))
                .unwrap_or_else(|| "—".into()),
            info.bit_depth
                .map(|b| b.to_string())
                .unwrap_or_else(|| "—".into()),
            izses_core::device::format_size(f.size_bytes),
        ]);
    }
    s.table = Some(table);
    s
}

fn section_integrity(input: &ReportInput<'_>) -> Section {
    let mut s = Section::new("7. Dosyaların SHA-256 ve Aktarım Doğrulama Bilgileri");
    if input.files.is_empty() {
        s.paragraphs
            .push("Doğrulanacak dosya bulunmamaktadır.".to_string());
        return s;
    }
    let mut table = Table {
        headers: vec![
            "Dosya".into(),
            "Kaynak SHA-256".into(),
            "Kopya SHA-256".into(),
            "Durum".into(),
        ],
        rows: Vec::new(),
    };
    for f in input.files {
        table.rows.push(vec![
            f.file_name(),
            f.source_sha256
                .clone()
                .unwrap_or_else(|| "—".into()),
            f.copy_sha256.clone().unwrap_or_else(|| "—".into()),
            f.verification.label_tr().to_string(),
        ]);
    }
    s.table = Some(table);
    s
}

fn section_method(input: &ReportInput<'_>) -> Section {
    let mut s = Section::new("8. İnceleme Yöntemi");
    s.paragraphs.push(
        "İncelemeye alınan dosyalar kaynak ortamdan salt okunur biçimde okunmuş, \
         çalışma alanına kopyalanmış ve kaynak/kopya SHA-256 değerleri karşılaştırılarak \
         aktarım doğrulanmıştır. Kaynak dosyalar üzerinde hiçbir değişiklik yapılmamıştır."
            .to_string(),
    );
    s.paragraphs.push(
        "Ses kayıtlarının metne dönüştürülmesi, tamamen yerel olarak çalışan bir \
         konuşma tanıma modeli ile gerçekleştirilmiştir. İşlem sırasında ses verisi \
         dış bir sunucuya gönderilmemiştir."
            .to_string(),
    );
    if !input.method_notes.trim().is_empty() {
        s.paragraphs.push(input.method_notes.clone());
    }
    s
}

fn section_model(input: &ReportInput<'_>) -> Section {
    let mut s = Section::new("9. Kullanılan Otomatik Konuşma Çözümleme Modeli");
    s.paragraphs.push(format!(
        "Model: {}",
        dash(&input.model_name)
    ));
    if !input.model_revision.is_empty() {
        s.paragraphs
            .push(format!("Model sürümü / kaynağı: {}", input.model_revision));
    }
    s.paragraphs.push(
        "Modelin ürettiği metin bir taslaktır; nihai metin bilirkişi tarafından \
         dinlenerek kontrol edilmiştir."
            .to_string(),
    );
    s
}

fn section_expert_edits(input: &ReportInput<'_>) -> Section {
    let mut s = Section::new("10. Bilirkişinin Kontrol ve Düzeltmeleri");
    let mut total = 0usize;
    let mut edited = 0usize;
    for (_, segs) in &input.transcripts {
        for seg in *segs {
            total += 1;
            if seg.is_edited() {
                edited += 1;
            }
        }
    }
    s.paragraphs.push(format!(
        "İncelenen toplam metin bölümü: {total}"
    ));
    s.paragraphs
        .push(format!("Bilirkişi tarafından düzeltilen bölüm: {edited}"));
    s
}

fn section_flagged(input: &ReportInput<'_>) -> Section {
    let mut s = Section::new("11. Anlaşılmayan veya Özel Olarak İşaretlenen Bölümler");
    let mut found = false;
    for (file, segs) in &input.transcripts {
        for seg in *segs {
            if seg.has_attention_flag() {
                found = true;
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
                s.bullets.push(format!(
                    "{} · {} · [{}] {}",
                    file.file_name(),
                    seg.time_label(),
                    tags.join(", "),
                    seg.effective_text()
                ));
            }
        }
    }
    if !found {
        s.paragraphs
            .push("Özel olarak işaretlenmiş bölüm bulunmamaktadır.".to_string());
    }
    s
}

fn section_observations(input: &ReportInput<'_>) -> Section {
    let mut s = Section::new("12. Teknik Gözlemler ve Notlar");
    let mut count = 0;
    for note in input.notes {
        if note.kind == NoteKind::Observation || note.kind == NoteKind::Technical {
            count += 1;
            s.bullets
                .push(format!("[{}] {}: {}", note.kind.label_tr(), note.title, note.body));
        }
    }
    if count == 0 {
        s.paragraphs
            .push("Kaydedilmiş teknik gözlem bulunmamaktadır.".to_string());
    }
    s
}

fn section_history(input: &ReportInput<'_>) -> Section {
    let mut s = Section::new("13. İnceleme Sırasında Yapılan İşlemlerin Özeti");
    if input.history_lines.is_empty() {
        s.paragraphs.push("İşlem geçmişi kaydı bulunmamaktadır.".to_string());
    } else {
        for line in &input.history_lines {
            s.bullets.push(line.clone());
        }
    }
    s
}

fn section_expert_assessment(input: &ReportInput<'_>) -> Section {
    let mut s = Section::new("14. Bilirkişinin Kendi Değerlendirmesi").expert();
    if input.expert_assessment.trim().is_empty() {
        s.paragraphs.push(
            "[Bilirkişi değerlendirmesi girilmemiştir. Bu bölüm bilirkişi tarafından \
             doldurulmalıdır.]"
                .to_string(),
        );
    } else {
        s.paragraphs.push(input.expert_assessment.clone());
    }
    s
}

fn section_conclusion(input: &ReportInput<'_>) -> Section {
    let mut s = Section::new("15. Gerekçeli Sonuç").expert();
    if input.conclusion.trim().is_empty() {
        s.paragraphs.push(
            "[Gerekçeli sonuç girilmemiştir. Bu bölüm bilirkişi tarafından \
             doldurulmalıdır.]"
                .to_string(),
        );
    } else {
        s.paragraphs.push(input.conclusion.clone());
    }
    s
}

fn build_transcript_blocks(input: &ReportInput<'_>) -> Vec<TranscriptBlock> {
    let mut blocks = Vec::new();
    for (file, segs) in &input.transcripts {
        let source_label = input
            .sources
            .iter()
            .find(|s| s.id == file.source_id)
            .map(|s| s.label.clone())
            .unwrap_or_else(|| "—".to_string());
        blocks.push(TranscriptBlock {
            file_name: file.file_name(),
            source_label,
            sha256: file.copy_sha256.clone().or_else(|| file.source_sha256.clone()),
            duration_label: file
                .audio
                .as_ref()
                .map(|a| a.duration_label())
                .unwrap_or_else(|| "—".to_string()),
            segments: segs.to_vec(),
        });
    }
    blocks
}

/// Rapor öncesi tutarlılık kontrollerini çalıştırır.
pub fn run_checks(input: &ReportInput<'_>) -> Vec<ReportCheck> {
    let mut checks = Vec::new();
    let inv = input.investigation;

    for field in inv.missing_required_fields() {
        checks.push(ReportCheck {
            severity: CheckSeverity::Blocker,
            message: format!("Zorunlu bilgi eksik: {field}"),
        });
    }

    if input.sources.is_empty() {
        checks.push(ReportCheck {
            severity: CheckSeverity::Blocker,
            message: "İncelemeye alınmış delil kaynağı yok.".to_string(),
        });
    }

    let audio: Vec<&EvidenceFile> = input.files.iter().filter(|f| f.is_audio()).collect();
    if audio.is_empty() {
        checks.push(ReportCheck {
            severity: CheckSeverity::Blocker,
            message: "İncelemeye alınmış ses dosyası yok.".to_string(),
        });
    }

    // Aktarımı doğrulanmamış dosyalar.
    for f in input.files {
        match f.verification {
            VerificationState::Verified => {}
            VerificationState::Mismatch => checks.push(ReportCheck {
                severity: CheckSeverity::Blocker,
                message: format!(
                    "{}: SHA-256 uyuşmazlığı var; aktarım doğrulanmadı.",
                    f.file_name()
                ),
            }),
            VerificationState::Failed => checks.push(ReportCheck {
                severity: CheckSeverity::Blocker,
                message: format!("{}: aktarım başarısız.", f.file_name()),
            }),
            VerificationState::InProgress => checks.push(ReportCheck {
                severity: CheckSeverity::Blocker,
                message: format!("{}: aktarım tamamlanmadı.", f.file_name()),
            }),
            VerificationState::NotAcquired => checks.push(ReportCheck {
                severity: CheckSeverity::Blocker,
                message: format!("{}: dosya incelemeye alınmadı.", f.file_name()),
            }),
        }
        if f.issue.is_some() {
            checks.push(ReportCheck {
                severity: CheckSeverity::Warning,
                message: format!(
                    "{}: {}",
                    f.file_name(),
                    f.issue.clone().unwrap_or_default()
                ),
            });
        }
    }

    // Deşifre edilmemiş ses dosyaları.
    let transcribed_ids: Vec<&izses_core::model::Id> =
        input.transcripts.iter().map(|(f, _)| &f.id).collect();
    for f in &audio {
        if !transcribed_ids.contains(&&f.id) {
            checks.push(ReportCheck {
                severity: CheckSeverity::Warning,
                message: format!("{}: otomatik deşifre bulunmuyor.", f.file_name()),
            });
        }
    }

    // Kontrol edilmemiş bölümler.
    let mut unchecked = 0usize;
    let mut flagged = 0usize;
    for (_, segs) in &input.transcripts {
        for seg in *segs {
            if seg.has_attention_flag() {
                flagged += 1;
            }
            if seg.effective_text().trim().is_empty() && !seg.is_edited() {
                unchecked += 1;
            }
        }
    }
    if flagged > 0 {
        checks.push(ReportCheck {
            severity: CheckSeverity::Warning,
            message: format!("{flagged} metin bölümü bilirkişi tarafından işaretlenmiş."),
        });
    }
    if unchecked > 0 {
        checks.push(ReportCheck {
            severity: CheckSeverity::Warning,
            message: format!("{unchecked} metin bölümü boş görünüyor."),
        });
    }

    if input.expert_assessment.trim().is_empty() {
        checks.push(ReportCheck {
            severity: CheckSeverity::Warning,
            message: "Bilirkişinin kendi değerlendirmesi girilmemiş.".to_string(),
        });
    }
    if input.conclusion.trim().is_empty() {
        checks.push(ReportCheck {
            severity: CheckSeverity::Warning,
            message: "Gerekçeli sonuç girilmemiş.".to_string(),
        });
    }

    if checks.is_empty() {
        checks.push(ReportCheck {
            severity: CheckSeverity::Info,
            message: "Tutarlılık kontrolünde eksik bulunmadı.".to_string(),
        });
    }

    checks
}

fn dash(s: &str) -> String {
    if s.trim().is_empty() {
        "—".to_string()
    } else {
        s.to_string()
    }
}

/// Raporun oluşturulma zamanını okunabilir biçimde verir.
pub fn generated_label(report: &Report) -> String {
    timefmt::format_datetime(&report.generated_at)
}
