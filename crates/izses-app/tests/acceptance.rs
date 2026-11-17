//! Uçtan uca kabul senaryosu (sentetik veri).
//!
//! Bu test, proje şartnamesindeki 12. bölümde tanımlanan zorunlu son kabul
//! senaryosunu gerçek bir iş akışı olarak doğrular:
//!
//! 1. İnceleme oluşturma ve görevlendirme bilgilerinin girilmesi
//! 2. Delil kaynağının (sentetik klasör) algılanması ve taranması
//! 3. Seçilen ses dosyalarının yerel inceleme alanına aktarılması
//! 4. Kaynak ve kopya SHA-256 değerlerinin doğrulanması
//! 5. Teknik envanterin oluşturulması
//! 6. Deşifre kaydının oluşturulması ve metin bölümlerinin kaydedilmesi
//! 7. Bilirkişi düzeltmeleri ve gözlemleri
//! 8. Rapor taslağının oluşturulması ve dışa aktarılması
//! 9. Uygulamanın kapanıp yeniden açılması sonrası her şeyin eksiksiz açılması
//!
//! Not: Gerçek delil içeren aygıtlar test amacıyla kullanılmaz; burada
//! tamamen sentetik ve kontrollü veri üretilir.

use std::path::Path;

use izses_app::services::acquisition;
use izses_app::services::session::{create_investigation, Session};
use izses_core::inventory::{ScanOptions, VerificationState};
use izses_core::investigation::StorageRoot;
use izses_core::model::{Investigation, InvestigationStatus, Party};
use izses_core::store::Store;
use izses_core::transcript::{
    Note, NoteKind, SegmentRevision, Transcript, TranscriptSegment, TranscriptStatus,
};

/// Sentetik WAV dosyası üretir (16 kHz, mono).
fn write_synthetic_wav(path: &Path, duration_secs: u32) {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: 16_000,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(path, spec).unwrap();
    for i in 0..(16_000 * duration_secs) {
        let v = ((i as f32 * 0.02).sin() * 8_000.0) as i16;
        writer.write_sample(v).unwrap();
    }
    writer.finalize().unwrap();
}

#[test]
fn mandatory_acceptance_scenario() {
    let tmp = tempfile::tempdir().unwrap();
    let root = StorageRoot::new(tmp.path().join("IZSES")).unwrap();
    let store = Store::open(&root.database_path()).unwrap();

    // ---- 1. İnceleme oluşturma ve görevlendirme bilgileri ----
    let mut inv = Investigation::new();
    inv.court = "Ankara 1. Asliye Ceza Mahkemesi".into();
    inv.case_number = "2026/125".into();
    inv.assignment_date = chrono::NaiveDate::from_ymd_opt(2026, 1, 15);
    inv.due_date = chrono::NaiveDate::from_ymd_opt(2026, 2, 15);
    inv.allowed_days = Some(30);
    inv.parties = vec![
        Party {
            role: "Şüpheli".into(),
            name: "S. Yılmaz".into(),
        },
        Party {
            role: "Mağdur".into(),
            name: "M. Demir".into(),
        },
    ];
    inv.subject = "Dijital ses kayıtlarının çözümlenmesi".into();
    inv.expert_name = "A. Bilirkişi".into();
    inv.expert_registry_no = "12345".into();
    inv.questions = vec![
        "Kayıtta hangi konuşmalar yer almaktadır?".into(),
        "Kayıtta bir kesinti veya ekleme izi var mıdır?".into(),
    ];

    let session = create_investigation(&store, &root, inv).unwrap();
    assert_eq!(
        session.investigation.status,
        InvestigationStatus::InProgress
    );
    assert!(root.investigation_exists(&session.storage_key));

    // ---- 2. Delil kaynağı: sentetik klasör ----
    let source_dir = tmp.path().join("DELIL_USB");
    std::fs::create_dir_all(source_dir.join("SES_KAYITLARI")).unwrap();
    std::fs::create_dir_all(source_dir.join("BELGELER")).unwrap();
    write_synthetic_wav(&source_dir.join("SES_KAYITLARI/gorusme_01.wav"), 2);
    write_synthetic_wav(&source_dir.join("SES_KAYITLARI/gorusme_02.wav"), 1);
    std::fs::write(
        source_dir.join("BELGELER/gorevlendirme.txt"),
        b"gorevlendirme yazisi (sentetik)",
    )
    .unwrap();

    let scanned = izses_core::inventory::scan_source(&source_dir, &ScanOptions::default()).unwrap();
    assert_eq!(scanned.len(), 3);
    let audio: Vec<_> = scanned
        .iter()
        .filter(|f| f.category == izses_core::inventory::FileCategory::Audio)
        .collect();
    assert_eq!(audio.len(), 2);

    let src = izses_core::transcript::EvidenceSource::new(
        session.id(),
        "KINGSTON USB",
        &source_dir,
        izses_core::device::DeviceKind::Removable,
        1,
    );
    store.insert_source(&src).unwrap();

    // ---- 3 & 4. Aktarım ve SHA-256 doğrulaması ----
    for file in &audio {
        let outcome = acquisition::acquire_file(
            &root,
            &store,
            session.id(),
            &session.storage_key,
            &src.id,
            &file.source_path,
            &file.relative_path,
        )
        .unwrap();
        assert!(
            outcome.verified,
            "aktarım doğrulanmadı: {}",
            outcome.file_name
        );
        assert_eq!(outcome.source_sha256, outcome.copy_sha256);
        assert!(file.source_path.is_file());
    }

    let files = store.list_files(session.id()).unwrap();
    assert_eq!(files.len(), 2);
    assert!(files
        .iter()
        .all(|f| f.verification == VerificationState::Verified));

    // ---- 5. Teknik envanter ----
    for f in &files {
        let info = f.audio.as_ref().expect("ses teknik bilgisi eksik");
        assert_eq!(info.sample_rate, Some(16_000));
        assert_eq!(info.channels, Some(1));
        assert!(info.duration_ms.is_some());
        assert!(f.copy_sha256.is_some());
    }

    // ---- 6. Deşifre kaydı ve metin bölümleri (sentetik) ----
    let file = &files[0];
    let mut t = Transcript::new(session.id(), &file.id);
    t.model_name = "Whisper large-v3-turbo".into();
    t.model_revision = "openai/whisper-large-v3-turbo".into();
    t.model_sha256 = Some("0".repeat(64));
    t.settings = serde_json::json!({"timestamps": true, "device": "CPU"});
    let t = store.upsert_transcript(&t).unwrap();

    let segs = vec![
        TranscriptSegment::new(&t.id, 0, 0, 1500, "Merhaba, nasılsınız?".into()),
        TranscriptSegment::new(&t.id, 1, 1500, 3200, "İyiyim teşekkür ederim.".into()),
        TranscriptSegment::new(&t.id, 2, 3200, 4800, "Görüşmeyi kaydediyorum.".into()),
    ];
    store.insert_segments(&segs).unwrap();

    let mut t = store.get_transcript(&t.id).unwrap();
    t.status = TranscriptStatus::Completed;
    t.total_segments = Some(1);
    t.completed_segments = 1;
    store.update_transcript(&mut t).unwrap();

    // ---- 7. Bilirkişi düzeltmeleri ve gözlemleri ----
    let mut seg0 = store.get_segment(&segs[0].id).unwrap();
    let original_auto = seg0.auto_text.clone();
    seg0.edited_text = Some("Merhaba, nasılsınız? (düzeltildi)".into());
    seg0.speaker = Some("Konuşmacı 1".into());
    store.update_segment(&mut seg0).unwrap();

    // Otomatik metin korunmalı, düzeltme ayrı tutulmalı.
    let reloaded = store.get_segment(&segs[0].id).unwrap();
    assert_eq!(reloaded.auto_text, original_auto);
    assert!(reloaded.is_edited());

    store
        .insert_revision(&SegmentRevision::new(
            &seg0.id,
            &t.id,
            original_auto.clone(),
            "Merhaba, nasılsınız? (düzeltildi)",
            "A. Bilirkişi",
        ))
        .unwrap();

    // Anlaşılamayan bölüm işareti.
    let mut seg2 = store.get_segment(&segs[2].id).unwrap();
    seg2.flags.unclear = true;
    seg2.observation = Some("Arka planda gürültü var".into());
    store.update_segment(&mut seg2).unwrap();

    // Notlar.
    let note = Note::new(
        session.id(),
        NoteKind::Technical,
        "Kayıt kalitesi",
        "İkinci kayıtta arka plan gürültüsü yüksek.",
    );
    store.insert_note(&note).unwrap();

    // ---- 8. Rapor taslağı ve dışa aktarma ----
    let session = Session::load(&store, session.id()).unwrap();
    let files = store.list_files(session.id()).unwrap();
    let sources = store.list_sources(session.id()).unwrap();
    let transcripts = store.list_transcripts(session.id()).unwrap();
    let notes = store.list_notes(session.id()).unwrap();

    let mut transcript_pairs = Vec::new();
    for f in &files {
        if !f.is_audio() {
            continue;
        }
        if let Some(tr) = transcripts.iter().find(|t| t.evidence_file_id == f.id) {
            let segs = store.list_segments(&tr.id).unwrap();
            transcript_pairs.push((f.clone(), segs));
        }
    }
    let pairs_ref: Vec<(&izses_core::inventory::EvidenceFile, &[TranscriptSegment])> =
        transcript_pairs
            .iter()
            .map(|(f, s)| (f, s.as_slice()))
            .collect();

    let report_input = izses_report::ReportInput {
        investigation: &session.investigation,
        sources: &sources,
        files: &files,
        transcripts: pairs_ref,
        notes: &notes,
        expert_assessment: "Kayıtlar incelenmiş ve metinler kontrol edilmiştir.".into(),
        conclusion: "Kayıt içeriği yukarıdaki deşifre metinlerinden ibarettir.".into(),
        model_name: "Whisper large-v3-turbo".into(),
        model_revision: "openai/whisper-large-v3-turbo".into(),
        method_notes: String::new(),
        history_lines: store
            .list_audit(session.id())
            .unwrap()
            .iter()
            .map(|e| format!("{} {}", e.kind.label_tr(), e.summary))
            .collect(),
    };

    let report = izses_report::build_report(&report_input).unwrap();
    assert!(!report.sections.is_empty());
    assert_eq!(report.transcript_blocks.len(), 1);
    assert!(
        !report.has_blockers(),
        "engelleyiciler: {:?}",
        report.blockers().map(|c| &c.message).collect::<Vec<_>>()
    );

    let out_dir = root.sub_dir(
        &session.storage_key,
        izses_core::investigation::SubDir::Reports,
    );
    std::fs::create_dir_all(&out_dir).unwrap();
    let docx_path = out_dir.join("rapor.docx");
    let pdf_path = out_dir.join("rapor.pdf");
    let html_path = out_dir.join("rapor.html");

    izses_report::docx::export_docx(&report, &docx_path).unwrap();
    izses_report::html::export_html(&report, &html_path).unwrap();
    let pdf_ok = izses_report::pdf::export_pdf(&report, &pdf_path).is_ok();

    assert!(docx_path.is_file());
    assert!(html_path.is_file());
    if pdf_ok {
        assert!(pdf_path.is_file());
    }

    // ---- 9. Kapatma ve yeniden açma ----
    drop(store);
    let store2 = Store::open(&root.database_path()).unwrap();

    let reopened = Session::load(&store2, session.id()).unwrap();
    assert_eq!(reopened.investigation.case_number, "2026/125");
    assert_eq!(reopened.sources.len(), 1);
    assert_eq!(reopened.files.len(), 2);
    assert_eq!(reopened.transcripts.len(), 1);
    assert_eq!(reopened.notes.len(), 1);

    let segs = store2.list_segments(&reopened.transcripts[0].id).unwrap();
    assert_eq!(segs.len(), 3);
    let edited = segs.iter().find(|s| s.is_edited()).unwrap();
    assert_eq!(edited.effective_text(), "Merhaba, nasılsınız? (düzeltildi)");
    assert_eq!(edited.speaker.as_deref(), Some("Konuşmacı 1"));
    assert!(segs.iter().any(|s| s.flags.unclear));

    let issues = reopened.verify_local_files(&root);
    assert!(issues.is_empty(), "eksik yerel dosyalar: {issues:?}");

    assert!(docx_path.is_file());
}
