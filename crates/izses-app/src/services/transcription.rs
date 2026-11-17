//! Otomatik deşifre kuyruğu servisi.
//!
//! Ses kayıtları sıraya alınır ve yerel model ile sırayla çözülür. Her parça
//! tamamlandığında sonuç kalıcı olarak kaydedilir. Kesinti hâlinde tamamlanan
//! parçalar korunur ve işleme kaldığı yerden devam edilir.

use std::sync::mpsc::{Receiver, Sender};
use std::sync::Arc;

use chrono::Utc;
use izses_asr::model_store::{ModelSpec, ModelStore};
use izses_asr::transcriber::{
    CancelToken, ComputeDevice, Progress, TranscriberConfig, TranscriptEngine,
};
use izses_core::audit::{ActionKind, AuditEntry};
use izses_core::error::{CoreError, CoreResult};
use izses_core::inventory::EvidenceFile;
use izses_core::investigation::{StorageRoot, SubDir};
use izses_core::model::Id;
use izses_core::store::Store;
use izses_core::transcript::{Transcript, TranscriptSegment, TranscriptStatus};

/// Arka plan işçisine gönderilen komutlar.
#[derive(Debug)]
pub enum Command {
    /// Kuyruğu başlat veya sürdür.
    Start,
    /// İşlemi durdur (tamamlanan parçalar korunur).
    Stop,
    /// Uygulama kapanıyor.
    Shutdown,
}

/// İşçiden arayüze gelen olaylar.
#[derive(Debug, Clone)]
pub enum Event {
    /// Model yüklendi.
    EngineReady { device: ComputeDevice },
    /// Bir dosya için ilerleme.
    Progress {
        file_id: Id,
        file_name: String,
        fraction: f32,
        message: String,
    },
    /// Bir parça tamamlandı ve kaydedildi.
    ChunkSaved {
        file_id: Id,
        chunk_index: u32,
        total_chunks: u32,
    },
    /// Bir dosya tamamlandı.
    FileCompleted {
        file_id: Id,
        file_name: String,
        segments: u32,
    },
    /// Bir dosya başarısız oldu.
    FileFailed {
        file_id: Id,
        file_name: String,
        error: String,
    },
    /// Kuyruk tamamen bitti.
    QueueFinished { completed: u32, failed: u32 },
    /// Ölümcül hata (ör. model yüklenemedi).
    Fatal { error: String },
}

/// Kuyruk durumu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueueState {
    Idle,
    Running,
    Stopping,
    Finished,
}

/// Kuyruğa alınacak bir iş.
#[derive(Debug, Clone)]
pub struct QueueItem {
    pub file_id: Id,
    pub file_name: String,
    /// İnceleme içindeki yerel dosya yolu.
    pub local_path: std::path::PathBuf,
}

/// Arka planda çalışan deşifre işçisi.
pub struct Transcriber {
    commands: Sender<Command>,
    events: Receiver<Event>,
    cancel: CancelToken,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl Transcriber {
    /// İşçiyi başlatır. Model yüklemesi arka planda yapılır.
    #[allow(clippy::too_many_arguments)]
    pub fn spawn(
        store: Arc<std::sync::Mutex<Store>>,
        root: StorageRoot,
        storage_key: String,
        investigation_id: Id,
        model_store: ModelStore,
        spec: &'static ModelSpec,
        cfg: TranscriberConfig,
        prefer_gpu: bool,
        items: Vec<QueueItem>,
    ) -> Self {
        let (cmd_tx, cmd_rx) = std::sync::mpsc::channel();
        let (evt_tx, evt_rx) = std::sync::mpsc::channel();
        let cancel = CancelToken::new();
        let cancel_worker = cancel.clone();

        let handle = std::thread::spawn(move || {
            run_worker(
                store,
                root,
                storage_key,
                investigation_id,
                model_store,
                spec,
                cfg,
                prefer_gpu,
                items,
                cmd_rx,
                evt_tx,
                cancel_worker,
            );
        });

        Self {
            commands: cmd_tx,
            events: evt_rx,
            cancel,
            handle: Some(handle),
        }
    }

    pub fn send(&self, cmd: Command) {
        let _ = self.commands.send(cmd);
    }

    /// Bekleyen olayları (varsa) toplar.
    pub fn poll_events(&self) -> Vec<Event> {
        let mut out = Vec::new();
        while let Ok(ev) = self.events.try_recv() {
            out.push(ev);
        }
        out
    }

    pub fn cancel_token(&self) -> &CancelToken {
        &self.cancel
    }
}

impl Drop for Transcriber {
    fn drop(&mut self) {
        self.cancel.cancel();
        let _ = self.commands.send(Command::Shutdown);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn run_worker(
    store: Arc<std::sync::Mutex<Store>>,
    root: StorageRoot,
    storage_key: String,
    investigation_id: Id,
    model_store: ModelStore,
    spec: &'static ModelSpec,
    cfg: TranscriberConfig,
    prefer_gpu: bool,
    items: Vec<QueueItem>,
    commands: Receiver<Command>,
    events: Sender<Event>,
    cancel: CancelToken,
) {
    // Model durumunu denetle.
    let state = model_store.check_present(spec);
    if !state.is_ready() {
        let _ = events.send(Event::Fatal {
            error: format!("Konuşma çözümleme modeli hazır değil. {}", state.label_tr()),
        });
        return;
    }

    // İlk komutu bekle.
    match commands.recv() {
        Ok(Command::Start) => {}
        Ok(Command::Shutdown) | Err(_) => return,
        Ok(Command::Stop) => return,
    }

    let mut engine = match TranscriptEngine::load(&model_store, spec, &cfg, prefer_gpu) {
        Ok(e) => e,
        Err(e) => {
            let _ = events.send(Event::Fatal {
                error: format!("Model yüklenemedi: {e}"),
            });
            return;
        }
    };
    let _ = events.send(Event::EngineReady {
        device: engine.compute_device(),
    });

    let mut completed = 0u32;
    let mut failed = 0u32;

    for item in &items {
        // Durdurma isteklerini kontrol et.
        let mut stop = false;
        while let Ok(cmd) = commands.try_recv() {
            match cmd {
                Command::Stop => {
                    stop = true;
                }
                Command::Shutdown => {
                    return;
                }
                Command::Start => {}
            }
        }
        if stop || cancel.is_cancelled() {
            break;
        }

        match transcribe_one(
            &store,
            &root,
            &storage_key,
            &investigation_id,
            &mut engine,
            item,
            &cfg,
            &events,
            &cancel,
            &commands,
        ) {
            Ok(segments) => {
                completed += 1;
                let _ = events.send(Event::FileCompleted {
                    file_id: item.file_id.clone(),
                    file_name: item.file_name.clone(),
                    segments,
                });
            }
            Err(WorkerError::Cancelled) => {
                // İptal: tamamlanan parçalar korunur.
                let _ = events.send(Event::FileFailed {
                    file_id: item.file_id.clone(),
                    file_name: item.file_name.clone(),
                    error: "İşlem durduruldu; tamamlanan parçalar korundu.".to_string(),
                });
                break;
            }
            Err(WorkerError::Core(e)) => {
                failed += 1;
                let _ = events.send(Event::FileFailed {
                    file_id: item.file_id.clone(),
                    file_name: item.file_name.clone(),
                    error: e.to_string(),
                });
            }
        }
    }

    let _ = events.send(Event::QueueFinished { completed, failed });
}

enum WorkerError {
    Cancelled,
    Core(CoreError),
}

impl From<CoreError> for WorkerError {
    fn from(e: CoreError) -> Self {
        WorkerError::Core(e)
    }
}

#[allow(clippy::too_many_arguments)]
fn transcribe_one(
    store: &Arc<std::sync::Mutex<Store>>,
    root: &StorageRoot,
    storage_key: &str,
    investigation_id: &Id,
    engine: &mut TranscriptEngine,
    item: &QueueItem,
    cfg: &TranscriberConfig,
    events: &Sender<Event>,
    cancel: &CancelToken,
    commands: &Receiver<Command>,
) -> Result<u32, WorkerError> {
    let file_path = root.sub_dir(storage_key, SubDir::OriginalFiles).join(
        item.local_path
            .strip_prefix(SubDir::OriginalFiles.name())
            .unwrap_or(&item.local_path),
    );

    // Deşifre kaydını oluştur veya mevcut olanı al.
    let mut transcript = {
        let s = store.lock().unwrap();
        let t = Transcript::new(investigation_id, &item.file_id);
        s.upsert_transcript(&t)?
    };

    // Ses dosyasını çöz.
    let audio = izses_audio::decode_to_mono_16k(&file_path)?;
    let total_chunks = TranscriptEngine::plan_chunk_count(audio.samples.len());

    // Kaldığı yerden devam et: tamamlanan parçaları belirle.
    let existing = {
        let s = store.lock().unwrap();
        s.list_segments(&transcript.id)?
    };
    let resume_from = if existing.is_empty() {
        0
    } else {
        // Tamamlanan parça sayısı, kaydedilmiş bölümlerin kapsadığı parça sayısıdır.
        let max_ms = existing.iter().map(|s| s.end_ms).max().unwrap_or(0);
        let per_ms = 30_000u64;
        ((max_ms / per_ms) as u32).min(total_chunks)
    };

    // Durumu güncelle.
    {
        let s = store.lock().unwrap();
        transcript.model_name = engine.model_name().to_string();
        transcript.model_revision = izses_asr::model_store::LARGE_V3_TURBO.repo.to_string();
        transcript.model_sha256 = engine.model_sha256().map(|s| s.to_string());
        transcript.language = cfg.language.clone();
        transcript.task = "transcribe".to_string();
        transcript.settings = serde_json::json!({
            "timestamps": cfg.timestamps,
            "seed": cfg.seed,
            "device": engine.compute_device().label_tr(),
        });
        transcript.status = TranscriptStatus::Running;
        transcript.total_segments = Some(total_chunks);
        transcript.completed_segments = resume_from;
        transcript.error = None;
        s.update_transcript(&mut transcript)?;

        if resume_from > 0 {
            s.insert_audit(&AuditEntry::new(
                investigation_id,
                ActionKind::TranscriptResumed,
                format!(
                    "{}: deşifre {} parçadan sürdürülüyor",
                    item.file_name, resume_from
                ),
            ))?;
        } else {
            s.insert_audit(&AuditEntry::new(
                investigation_id,
                ActionKind::TranscriptStarted,
                format!(
                    "{}: deşifre başladı ({} parça)",
                    item.file_name, total_chunks
                ),
            ))?;
        }
    }

    let file_id = item.file_id.clone();
    let file_name = item.file_name.clone();
    let transcript_id = transcript.id.clone();
    let store_for_cb = Arc::clone(store);
    let inv_for_cb = investigation_id.clone();
    let mut segment_index: u32 = {
        let s = store.lock().unwrap();
        s.list_segments(&transcript_id)?.len() as u32
    };

    // Parça tamamlandıkça kaydet.
    let result = izses_asr::transcriber::transcribe(
        engine,
        &audio.samples,
        resume_from,
        cancel,
        |p: Progress| {
            let _ = events.send(Event::Progress {
                file_id: file_id.clone(),
                file_name: file_name.clone(),
                fraction: p.fraction(),
                message: p.message,
            });
        },
        |chunk_index: u32, segments: &[izses_asr::transcriber::SegmentData]| {
            let segs: Vec<TranscriptSegment> = segments
                .iter()
                .filter(|s| !s.text.trim().is_empty())
                .map(|s| {
                    let mut seg = TranscriptSegment::new(
                        &transcript_id,
                        segment_index,
                        s.start_ms,
                        s.end_ms,
                        s.text.clone(),
                    );
                    seg.avg_logprob = s.avg_logprob;
                    seg.no_speech_prob = s.no_speech_prob;
                    segment_index += 1;
                    seg
                })
                .collect();

            let s = store_for_cb.lock().unwrap();
            s.insert_segments(&segs)?;

            // Parça tamamlandı bilgisini kalıcı hâle getir.
            let mut t = s.get_transcript(&transcript_id)?;
            t.completed_segments = chunk_index + 1;
            s.update_transcript(&mut t)?;

            let _ = events.send(Event::ChunkSaved {
                file_id: file_id.clone(),
                chunk_index,
                total_chunks,
            });
            let _ = s.insert_audit(&AuditEntry::new(
                &inv_for_cb,
                ActionKind::TranscriptSegmentCompleted,
                format!(
                    "{file_name}: parça {}/{} tamamlandı",
                    chunk_index + 1,
                    total_chunks
                ),
            ));
            Ok(())
        },
    );

    // Komut kuyruğunu kontrol et.
    let mut cancelled = cancel.is_cancelled();
    while let Ok(cmd) = commands.try_recv() {
        match cmd {
            Command::Stop => {
                cancel.cancel();
                cancelled = true;
            }
            Command::Shutdown => {
                cancel.cancel();
                cancelled = true;
            }
            Command::Start => {}
        }
    }

    match result {
        Ok(_completed_chunks) => {
            if cancelled {
                let s = store.lock().unwrap();
                let mut t = s.get_transcript(&transcript_id)?;
                t.status = TranscriptStatus::Cancelled;
                t.error = Some("İşlem durduruldu; tamamlanan parçalar korundu.".to_string());
                s.update_transcript(&mut t)?;
                return Err(WorkerError::Cancelled);
            }

            let count = {
                let s = store.lock().unwrap();
                let mut t = s.get_transcript(&transcript_id)?;
                t.status = TranscriptStatus::Completed;
                t.completed_segments = total_chunks;
                s.update_transcript(&mut t)?;
                s.insert_audit(&AuditEntry::new(
                    investigation_id,
                    ActionKind::TranscriptCompleted,
                    format!("{file_name}: deşifre tamamlandı"),
                ))?;
                s.count_segments(&transcript_id)?
            };
            Ok(count)
        }
        Err(e) => {
            let s = store.lock().unwrap();
            let mut t = s.get_transcript(&transcript_id)?;
            t.status = if cancelled {
                TranscriptStatus::Cancelled
            } else {
                TranscriptStatus::Failed
            };
            t.error = Some(e.to_string());
            s.update_transcript(&mut t)?;
            s.insert_audit(&AuditEntry::new(
                investigation_id,
                ActionKind::TranscriptFailed,
                format!("{file_name}: deşifre hatası"),
            ))?;
            if cancelled {
                Err(WorkerError::Cancelled)
            } else {
                Err(WorkerError::Core(e))
            }
        }
    }
}

/// Bir incelemedeki, deşifresi olmayan ses dosyalarını kuyruk için hazırlar.
pub fn pending_audio_items(store: &Store, investigation_id: &Id) -> CoreResult<Vec<QueueItem>> {
    let audio = store.list_audio_files(investigation_id)?;
    let transcripts = store.list_transcripts(investigation_id)?;
    let mut out = Vec::new();
    for file in audio {
        if !file.is_verified() {
            continue;
        }
        let done = transcripts
            .iter()
            .any(|t| t.evidence_file_id == file.id && t.status == TranscriptStatus::Completed);
        if done {
            continue;
        }
        out.push(QueueItem {
            file_id: file.id.clone(),
            file_name: file.file_name(),
            local_path: file.local_relative_path.clone(),
        });
    }
    Ok(out)
}

/// Tamamlanmamış deşifreleri (yeniden başlatma sonrası) listeler.
pub fn resumable_items(store: &Store, investigation_id: &Id) -> CoreResult<Vec<QueueItem>> {
    let transcripts = store.find_resumable_transcripts(investigation_id)?;
    let mut out = Vec::new();
    for t in transcripts {
        if let Ok(file) = store.get_file(&t.evidence_file_id) {
            if file.is_verified() {
                out.push(QueueItem {
                    file_id: file.id.clone(),
                    file_name: file.file_name(),
                    local_path: file.local_relative_path.clone(),
                });
            }
        }
    }
    Ok(out)
}

/// Bir ses dosyasının inceleme içindeki tam yolu.
pub fn local_audio_path(
    root: &StorageRoot,
    storage_key: &str,
    file: &EvidenceFile,
) -> std::path::PathBuf {
    let rel = file
        .local_relative_path
        .strip_prefix(SubDir::OriginalFiles.name())
        .unwrap_or(&file.local_relative_path);
    root.sub_dir(storage_key, SubDir::OriginalFiles).join(rel)
}

/// Deşifre tamamlanma oranını hesaplar.
pub fn transcript_progress(t: &Transcript) -> Option<f32> {
    t.progress()
}

/// Şu anki zamanı döndürür (test edilebilirlik için ayrı tutuldu).
pub fn now() -> chrono::DateTime<Utc> {
    Utc::now()
}

#[cfg(test)]
mod tests {
    use super::*;
    use izses_core::model::Investigation;
    use izses_core::transcript::EvidenceSource;
    use std::path::PathBuf;

    fn setup() -> (Store, Id) {
        let store = Store::open_in_memory().unwrap();
        let mut inv = Investigation::new();
        inv.case_number = "2026/125".into();
        store.insert_investigation(&inv).unwrap();
        (store, inv.id)
    }

    fn add_audio(store: &Store, inv_id: &Id, name: &str, verified: bool) -> Id {
        let src = EvidenceSource::new(
            inv_id,
            "USB",
            "/tmp/usb",
            izses_core::device::DeviceKind::Removable,
            1,
        );
        let _ = store.insert_source(&src);
        let src = store
            .list_sources(inv_id)
            .unwrap()
            .into_iter()
            .next()
            .unwrap();
        let file = EvidenceFile {
            id: Id::new(),
            investigation_id: inv_id.clone(),
            source_id: src.id.clone(),
            relative_path: PathBuf::from(name),
            local_relative_path: PathBuf::from(format!("orijinal_dosyalar/{name}")),
            size_bytes: 100,
            extension: "wav".into(),
            category: izses_core::inventory::FileCategory::Audio,
            source_modified: None,
            source_created: None,
            acquired_at: None,
            source_sha256: None,
            copy_sha256: None,
            verification: if verified {
                izses_core::inventory::VerificationState::Verified
            } else {
                izses_core::inventory::VerificationState::Failed
            },
            audio: None,
            issue: None,
        };
        store.insert_file(&file).unwrap();
        file.id
    }

    #[test]
    fn pending_items_excludes_unverified_and_completed() {
        let (store, inv_id) = setup();
        add_audio(&store, &inv_id, "a.wav", true);
        add_audio(&store, &inv_id, "b.wav", false);

        let items = pending_audio_items(&store, &inv_id).unwrap();
        assert_eq!(items.len(), 1, "yalnızca doğrulanmış dosya kuyruğa girmeli");
        assert_eq!(items[0].file_name, "a.wav");
    }

    #[test]
    fn pending_items_excludes_transcribed() {
        let (store, inv_id) = setup();
        let file_id = add_audio(&store, &inv_id, "a.wav", true);

        let mut t = Transcript::new(&inv_id, &file_id);
        t.status = TranscriptStatus::Completed;
        store.upsert_transcript(&t).unwrap();

        let items = pending_audio_items(&store, &inv_id).unwrap();
        assert!(
            items.is_empty(),
            "tamamlanan dosya tekrar kuyruğa girmemeli"
        );
    }

    #[test]
    fn resumable_items_found_after_crash() {
        let (store, inv_id) = setup();
        let file_id = add_audio(&store, &inv_id, "a.wav", true);

        let mut t = Transcript::new(&inv_id, &file_id);
        t.status = TranscriptStatus::Running;
        store.upsert_transcript(&t).unwrap();

        let items = resumable_items(&store, &inv_id).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].file_name, "a.wav");
    }
}
