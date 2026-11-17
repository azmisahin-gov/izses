//! Deşifre servisi entegrasyon testi (gerçek model gerekir).
//!
//! Bu test, gerçek bir Whisper modeli ile kuyruk, kalıcılık ve kaldığı
//! yerden sürdürme davranışını doğrular. Model dosyaları depoda
//! bulunmadığı için test yalnızca `IZSES_TEST_MODEL` ortam değişkeni bir
//! model klasörüne işaret ettiğinde çalışır; aksi hâlde atlanır.
//!
//! Kullanım:
//!   IZSES_TEST_MODEL=/yol/whisper-tiny \
//!   IZSES_TEST_WAV=/yol/ornek.wav \
//!   cargo test -p izses-app --test transcription_service -- --nocapture

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use izses_app::services::acquisition;
use izses_app::services::session::create_investigation;
use izses_app::services::transcription::{self, Command, Event, QueueItem};
use izses_asr::model_store::{ModelSpec, ModelStore};
use izses_asr::transcriber::TranscriberConfig;
use izses_core::investigation::StorageRoot;
use izses_core::model::Investigation;
use izses_core::store::Store;
use izses_core::transcript::TranscriptStatus;

const SPEC: ModelSpec = ModelSpec {
    display_name: "Test modeli",
    repo: "verify/model",
    weights_file: "model.safetensors",
    tokenizer_file: "tokenizer.json",
    config_file: "config.json",
    num_mel_bins: 80,
};

#[test]
fn transcription_service_persists_and_resumes() {
    let model_dir = match std::env::var("IZSES_TEST_MODEL") {
        Ok(p) => PathBuf::from(p),
        Err(_) => {
            eprintln!("IZSES_TEST_MODEL ayarlanmadı; test atlanıyor.");
            return;
        }
    };
    let wav = match std::env::var("IZSES_TEST_WAV") {
        Ok(p) => PathBuf::from(p),
        Err(_) => {
            eprintln!("IZSES_TEST_WAV ayarlanmadı; test atlanıyor.");
            return;
        }
    };

    // Model deposunu hazırla.
    let model_root = model_dir.parent().unwrap_or(&model_dir).to_path_buf();
    let model_store = ModelStore::new(&model_root);
    let expected = model_store.model_dir(&SPEC);
    if expected != model_dir {
        std::fs::create_dir_all(&expected).unwrap();
        for f in ["config.json", "tokenizer.json", "model.safetensors"] {
            std::fs::copy(model_dir.join(f), expected.join(f)).unwrap();
        }
    }
    assert!(model_store.check_present(&SPEC).is_ready());

    // İnceleme ve sentetik delil kaynağı.
    let tmp = tempfile::tempdir().unwrap();
    let root = StorageRoot::new(tmp.path().join("IZSES")).unwrap();
    let store = Store::open(&root.database_path()).unwrap();

    let mut inv = Investigation::new();
    inv.case_number = "2026/900".into();
    inv.subject = "Deşifre servisi testi".into();
    inv.expert_name = "Test".into();
    inv.expert_registry_no = "1".into();
    inv.court = "Test Mahkemesi".into();
    let session = create_investigation(&store, &root, inv).unwrap();

    // WAV'ı incelemeye aktar.
    let src_dir = tmp.path().join("kaynak");
    std::fs::create_dir_all(&src_dir).unwrap();
    let src_file = src_dir.join("ornek.wav");
    std::fs::copy(&wav, &src_file).unwrap();

    let src = izses_core::transcript::EvidenceSource::new(
        session.id(),
        "TEST",
        &src_dir,
        izses_core::device::DeviceKind::Removable,
        1,
    );
    store.insert_source(&src).unwrap();
    let outcome = acquisition::acquire_file(
        &root,
        &store,
        session.id(),
        &session.storage_key,
        &src.id,
        &src_file,
        std::path::Path::new("ornek.wav"),
    )
    .unwrap();
    assert!(outcome.verified);

    let store = Arc::new(Mutex::new(store));
    let items = {
        let s = store.lock().unwrap();
        transcription::pending_audio_items(&s, session.id()).unwrap()
    };
    assert_eq!(items.len(), 1);

    let cfg = TranscriberConfig {
        language: "en".to_string(),
        timestamps: true,
        ..Default::default()
    };

    let transcriber = transcription::Transcriber::spawn(
        Arc::clone(&store),
        root.clone(),
        session.storage_key.clone(),
        session.id().clone(),
        model_store,
        &SPEC,
        cfg,
        false,
        items.clone(),
    );
    transcriber.send(Command::Start);

    // Olayları bekle (en fazla 180 saniye).
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(180);
    let mut finished = false;
    let mut saw_chunk = false;
    let mut error: Option<String> = None;
    while std::time::Instant::now() < deadline {
        for ev in transcriber.poll_events() {
            match ev {
                Event::EngineReady { .. } => eprintln!("model hazır"),
                Event::Progress { message, .. } => eprintln!("{message}"),
                Event::ChunkSaved { .. } => saw_chunk = true,
                Event::FileCompleted { segments, .. } => {
                    eprintln!("dosya tamamlandı: {segments} bölüm")
                }
                Event::FileFailed { error: e, .. } => error = Some(e),
                Event::QueueFinished { .. } => finished = true,
                Event::Fatal { error: e } => {
                    error = Some(e);
                    finished = true;
                }
            }
        }
        if finished {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(200));
    }
    drop(transcriber);

    assert!(error.is_none(), "deşifre hatası: {error:?}");
    assert!(finished, "kuyruk zaman aşımına uğradı");
    assert!(saw_chunk, "hiç parça kaydedilmedi");

    // Sonuçlar kalıcı olmalı.
    let s = store.lock().unwrap();
    let transcripts = s.list_transcripts(session.id()).unwrap();
    assert_eq!(transcripts.len(), 1);
    assert_eq!(transcripts[0].status, TranscriptStatus::Completed);
    let segs = s.list_segments(&transcripts[0].id).unwrap();
    assert!(!segs.is_empty(), "hiç metin bölümü kaydedilmedi");

    // Metin gerçekten üretilmiş olmalı.
    let full: String = segs
        .iter()
        .map(|x| x.auto_text.clone())
        .collect::<Vec<_>>()
        .join(" ");
    eprintln!("üretilen metin: {full}");
    assert!(full.split_whitespace().count() >= 3, "metin çok kısa: {full}");

    // Zaman damgaları artan olmalı.
    for pair in segs.windows(2) {
        assert!(
            pair[1].start_ms >= pair[0].start_ms,
            "zaman damgaları azalıyor"
        );
    }

    // Kaldığı yerden sürdürme: tamamlanan iş yeniden kuyruğa girmemeli.
    let pending = transcription::pending_audio_items(&s, session.id()).unwrap();
    assert!(pending.is_empty(), "tamamlanan dosya yeniden kuyruğa girdi");

    // Yeniden yükleme sonrası sonuçlar yerinde olmalı.
    let reloaded = izses_app::services::session::Session::load(&s, session.id()).unwrap();
    assert_eq!(reloaded.transcripts.len(), 1);
    assert_eq!(reloaded.segments.len(), 1);
    let _: Vec<QueueItem> = items;
}
