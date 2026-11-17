//! Uygulama durumu ve ana pencere.
//!
//! Arayüz tek bir ana iş akışı izler: inceleme oluştur → delil al →
//! deşifre et → kontrol et → raporla. Bilirkişi her zaman hangi incelemede
//! bulunduğunu görür.

use std::sync::{Arc, Mutex};

use izses_asr::model_store::{ModelStore, LARGE_V3_TURBO};
use izses_asr::transcriber::{ComputeDevice, TranscriberConfig};
use izses_core::error::CoreResult;
use izses_core::investigation::StorageRoot;
use izses_core::model::{Id, Investigation};
use izses_core::store::Store;
use izses_report::{Report, ReportInput};

use crate::services::session::Session;
use crate::services::transcription::{self, Event, QueueState, Transcriber};
use crate::settings::{self, Settings};
use crate::ui;

/// Çalışma alanı sekmeleri.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Overview,
    Evidence,
    Audio,
    Transcription,
    Review,
    Notes,
    Report,
}

impl Tab {
    pub const ALL: [Tab; 7] = [
        Tab::Overview,
        Tab::Evidence,
        Tab::Audio,
        Tab::Transcription,
        Tab::Review,
        Tab::Notes,
        Tab::Report,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Tab::Overview => "Genel Bakış",
            Tab::Evidence => "Delil Materyalleri",
            Tab::Audio => "Ses Kayıtları",
            Tab::Transcription => "Otomatik Deşifre",
            Tab::Review => "Metin Kontrolü",
            Tab::Notes => "Notlar ve Bulgular",
            Tab::Report => "Rapor",
        }
    }
}

/// Hangi üst düzey ekranın gösterildiği.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    /// İnceleme yönetimi (ana ekran).
    Home,
    /// Açık incelemenin çalışma alanı.
    Workspace,
    /// Ayarlar.
    Settings,
}

/// Kullanıcıya gösterilecek bildirim.
#[derive(Debug, Clone)]
pub struct Notification {
    pub text: String,
    pub kind: NotificationKind,
    pub created: std::time::Instant,
}

/// Arayüzde seçilebilir taranmış dosya.
#[derive(Debug, Clone)]
pub struct ScannedItem {
    pub inner: izses_core::inventory::ScannedFile,
    pub selected: bool,
}

impl ScannedItem {
    pub fn new(inner: izses_core::inventory::ScannedFile) -> Self {
        Self {
            inner,
            selected: false,
        }
    }

    pub fn display_name(&self) -> String {
        self.inner.relative_path.to_string_lossy().to_string()
    }

    pub fn size_bytes(&self) -> u64 {
        self.inner.size_bytes
    }

    pub fn category(&self) -> izses_core::inventory::FileCategory {
        self.inner.category
    }

    pub fn source_path(&self) -> std::path::PathBuf {
        self.inner.source_path.clone()
    }

    pub fn relative_path(&self) -> std::path::PathBuf {
        self.inner.relative_path.clone()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotificationKind {
    Info,
    Success,
    Warning,
    Error,
}

/// Devam eden uzun işlemin durumu.
#[derive(Debug, Clone, Default)]
pub struct BusyState {
    pub active: bool,
    pub label: String,
    pub fraction: f32,
}

/// Uygulamanın tam durumu.
pub struct IzsesApp {
    pub settings: Settings,
    pub store: Arc<Mutex<Store>>,
    pub root: StorageRoot,
    pub screen: Screen,
    pub tab: Tab,

    /// Açık inceleme.
    pub session: Option<Session>,
    /// Genel bakış / delil ekranı için liste.
    pub investigations: Vec<Investigation>,

    /// Deşifre işçisi.
    pub transcriber: Option<Transcriber>,
    pub queue_state: QueueState,
    pub compute_device: Option<ComputeDevice>,

    /// Metin kontrolü ekranının seçili dosyası ve bölümü.
    pub review_file: Option<Id>,
    pub review_selected: Option<usize>,
    pub review_playhead_ms: u64,
    pub review_playing: bool,
    pub review_speed: f32,
    /// Ses Kayıtları sekmesinde seçili dosya.
    pub audio_selected: Option<usize>,

    /// Rapor ekranının durumu.
    pub report: Option<Report>,
    pub expert_assessment: String,
    pub conclusion: String,
    pub method_notes: String,
    pub report_status: String,

    /// Yeni inceleme formu.
    pub new_investigation: Investigation,
    pub new_questions_text: String,

    /// Yeni not formu.
    pub new_note_kind: izses_core::transcript::NoteKind,
    pub new_note_title: String,
    pub new_note_body: String,

    /// Bağlı aygıtlar ve tarama durumu.
    pub devices: Vec<izses_core::device::DeviceInfo>,
    pub scanned: Vec<ScannedItem>,
    pub scanned_source: Option<Id>,

    /// Genel bildirimler ve iş durumu.
    pub notifications: Vec<Notification>,
    pub busy: BusyState,
}

impl IzsesApp {
    /// Uygulamayı başlatır: klasörleri hazırlar, veritabanını açar.
    pub fn new() -> CoreResult<Self> {
        let settings = Settings::load_or_default();
        settings::ensure_dirs(&settings).ok();

        let root = StorageRoot::new(settings.storage_root.clone())?;
        let store = Store::open(&root.database_path())?;

        let investigations = store.list_investigations().unwrap_or_default();

        Ok(Self {
            settings,
            store: Arc::new(Mutex::new(store)),
            root,
            screen: Screen::Home,
            tab: Tab::Overview,
            session: None,
            investigations,
            transcriber: None,
            queue_state: QueueState::Idle,
            compute_device: None,
            review_file: None,
            review_selected: None,
            review_playhead_ms: 0,
            review_playing: false,
            review_speed: 1.0,
            audio_selected: None,
            report: None,
            expert_assessment: String::new(),
            conclusion: String::new(),
            method_notes: String::new(),
            report_status: String::new(),
            new_investigation: Investigation::new(),
            new_questions_text: String::new(),
            new_note_kind: izses_core::transcript::NoteKind::General,
            new_note_title: String::new(),
            new_note_body: String::new(),
            devices: izses_core::device::list_devices(),
            scanned: Vec::new(),
            scanned_source: None,
            notifications: Vec::new(),
            busy: BusyState::default(),
        })
    }

    pub fn notify(&mut self, kind: NotificationKind, text: impl Into<String>) {
        self.notifications.push(Notification {
            text: text.into(),
            kind,
            created: std::time::Instant::now(),
        });
        // Ekranda en fazla 6 bildirim tutulur.
        if self.notifications.len() > 6 {
            self.notifications.remove(0);
        }
    }

    /// Veritabanına erişim için paylaşılan tutamaç.
    ///
    /// Tutamacı klonlamak `self` üzerindeki ödünç almayı bitirir; böylece
    /// kilit tutulurken arayüz durumu da değiştirilebilir.
    pub fn store_handle(&self) -> Arc<Mutex<Store>> {
        Arc::clone(&self.store)
    }

    /// Kısa ömürlü veritabanı erişimi.
    ///
    /// Kilit yalnızca ifade boyunca tutulur; bu yüzden çağıran kod aynı
    /// anda `self` üzerinde değişiklik yapamaz. Uzun işlemler için
    /// `store_handle` kullanın.
    pub fn with_store<T>(&self, f: impl FnOnce(&Store) -> T) -> T {
        let guard = self.store.lock().unwrap();
        f(&guard)
    }

    pub fn store(&self) -> std::sync::MutexGuard<'_, Store> {
        self.store.lock().unwrap()
    }

    /// İncelemeleri yeniden yükler.
    pub fn reload_investigations(&mut self) {
        let result = self.with_store(|s| s.list_investigations());
        match result {
            Ok(list) => self.investigations = list,
            Err(e) => self.notify(NotificationKind::Error, format!("İncelemeler yüklenemedi: {e}")),
        }
    }

    /// İncelemeyi açar.
    pub fn open_investigation(&mut self, id: &Id) {
        let loaded = self.with_store(|store| Session::load(store, id));
        match loaded {
            Ok(session) => {
                let key = session.storage_key.clone();
                self.settings.push_recent(&key);
                let _ = self.settings.save();
                self.session = Some(session);
                self.screen = Screen::Workspace;
                self.tab = Tab::Overview;
                self.review_file = None;
                self.report = None;
                self.queue_state = QueueState::Idle;
                self.notify(NotificationKind::Info, "İnceleme açıldı");
            }
            Err(e) => self.notify(NotificationKind::Error, format!("İnceleme açılamadı: {e}")),
        }
    }

    /// Depolama anahtarından incelemeyi açar.
    pub fn open_investigation_by_key(&mut self, key: &str) {
        let loaded = self.with_store(|store| Session::load_by_key(store, key));
        match loaded {
            Ok(session) => {
                self.settings.push_recent(key);
                let _ = self.settings.save();
                self.session = Some(session);
                self.screen = Screen::Workspace;
                self.tab = Tab::Overview;
                self.review_file = None;
                self.report = None;
                self.notify(NotificationKind::Info, "İnceleme açıldı");
            }
            Err(e) => self.notify(NotificationKind::Error, format!("İnceleme açılamadı: {e}")),
        }
    }

    /// Açık incelemeyi kapatır.
    pub fn close_investigation(&mut self) {
        self.stop_transcription();
        self.session = None;
        self.screen = Screen::Home;
        self.review_file = None;
        self.report = None;
        self.reload_investigations();
    }

    /// Açık incelemeyi yeniden yükler.
    pub fn refresh_session(&mut self) {
        let Some(session) = &self.session else { return };
        let id = session.investigation.id.clone();
        let loaded = self.with_store(|store| Session::load(store, &id));
        match loaded {
            Ok(s) => self.session = Some(s),
            Err(e) => self.notify(NotificationKind::Error, format!("Yenilenemedi: {e}")),
        }
    }

    /// Otomatik deşifre kuyruğunu başlatır.
    pub fn start_transcription(&mut self, items: Vec<transcription::QueueItem>) {
        if items.is_empty() {
            self.notify(NotificationKind::Info, "Deşifre edilecek ses dosyası yok");
            return;
        }
        let Some(session) = &self.session else {
            self.notify(NotificationKind::Error, "Önce bir inceleme açın");
            return;
        };

        let model_store = ModelStore::new(self.settings.model_root.clone());
        let state = model_store.check_present(&LARGE_V3_TURBO);
        if !state.is_ready() {
            self.notify(
                NotificationKind::Error,
                format!(
                    "Konuşma çözümleme modeli hazır değil. {} \
                     Model dosyalarını Ayarlar ekranından kurun.",
                    state.label_tr()
                ),
            );
            return;
        }

        let cfg = TranscriberConfig {
            language: self.settings.language.clone(),
            timestamps: self.settings.timestamps,
            ..Default::default()
        };

        let transcriber = Transcriber::spawn(
            Arc::clone(&self.store),
            self.root.clone(),
            session.storage_key.clone(),
            session.investigation.id.clone(),
            model_store,
            &LARGE_V3_TURBO,
            cfg,
            self.settings.prefer_gpu,
            items,
        );
        self.transcriber = Some(transcriber);
        self.queue_state = QueueState::Running;
        self.notify(NotificationKind::Info, "Deşifre kuyruğu başlatıldı");
    }

    /// Deşifreyi durdurur; tamamlanan parçalar korunur.
    pub fn stop_transcription(&mut self) {
        if let Some(t) = &self.transcriber {
            t.send(transcription::Command::Stop);
            t.cancel_token().cancel();
            self.queue_state = QueueState::Stopping;
            self.notify(
                NotificationKind::Warning,
                "Deşifre durduruluyor; tamamlanan parçalar korunacak",
            );
        }
    }

    /// İşçiden gelen olayları işler.
    pub fn poll_transcriber(&mut self) {
        let events = match &self.transcriber {
            Some(t) => t.poll_events(),
            None => Vec::new(),
        };
        for ev in events {
            match ev {
                Event::EngineReady { device } => {
                    self.compute_device = Some(device);
                    self.notify(
                        NotificationKind::Info,
                        format!("Model yüklendi (cihaz: {})", device.label_tr()),
                    );
                }
                Event::Progress {
                    fraction, message, ..
                } => {
                    self.busy.active = true;
                    self.busy.label = message;
                    self.busy.fraction = fraction;
                }
                Event::ChunkSaved { .. } => {
                    // Parça kaydedildi; arayüz ilerlemesi zaten güncelleniyor.
                }
                Event::FileCompleted {
                    file_name, segments, ..
                } => {
                    self.notify(
                        NotificationKind::Success,
                        format!("{file_name}: deşifre tamamlandı ({segments} bölüm)"),
                    );
                    self.refresh_session();
                }
                Event::FileFailed {
                    file_name, error, ..
                } => {
                    self.notify(
                        NotificationKind::Error,
                        format!("{file_name}: {error}"),
                    );
                    self.refresh_session();
                }
                Event::QueueFinished { completed, failed } => {
                    self.queue_state = QueueState::Finished;
                    self.busy = BusyState::default();
                    self.transcriber = None;
                    self.refresh_session();
                    self.notify(
                        if failed > 0 {
                            NotificationKind::Warning
                        } else {
                            NotificationKind::Success
                        },
                        format!("Kuyruk bitti: {completed} tamamlandı, {failed} başarısız"),
                    );
                }
                Event::Fatal { error } => {
                    self.queue_state = QueueState::Idle;
                    self.busy = BusyState::default();
                    self.transcriber = None;
                    self.notify(NotificationKind::Error, error);
                }
            }
        }
    }

    /// Raporu derler.
    pub fn build_report(&mut self) {
        let Some(session) = &self.session else {
            self.notify(NotificationKind::Error, "Açık inceleme yok");
            return;
        };

        // Deşifre bölümlerini dosyalara bağla.
        let mut transcripts = Vec::new();
        for file in &session.files {
            if !file.is_audio() {
                continue;
            }
            if let Some(t) = session.transcript_for_file(&file.id) {
                if let Some(segs) = session.segments.get(&t.id) {
                    transcripts.push((file, segs.as_slice()));
                }
            }
        }

        let input = ReportInput {
            investigation: &session.investigation,
            sources: &session.sources,
            files: &session.files,
            transcripts,
            notes: &session.notes,
            expert_assessment: self.expert_assessment.clone(),
            conclusion: self.conclusion.clone(),
            model_name: LARGE_V3_TURBO.display_name.to_string(),
            model_revision: LARGE_V3_TURBO.repo.to_string(),
            method_notes: self.method_notes.clone(),
            history_lines: session.history_lines(),
        };

        match izses_report::build_report(&input) {
            Ok(report) => {
                let blockers = report.blockers().count();
                let warnings = report.warnings().count();
                self.report_status = if blockers > 0 {
                    format!("{blockers} engelleyici eksik, {warnings} uyarı")
                } else if warnings > 0 {
                    format!("{warnings} uyarı")
                } else {
                    "Tutarlılık kontrolü temiz".to_string()
                };
                self.report = Some(report);
                let sid = session.id().clone();
                let _ = self.with_store(|store| {
                    store.insert_audit(&izses_core::audit::AuditEntry::new(
                        &sid,
                        izses_core::audit::ActionKind::ReportDrafted,
                        "Rapor taslağı oluşturuldu",
                    ))
                });
                self.notify(NotificationKind::Success, "Rapor taslağı oluşturuldu");
            }
            Err(e) => self.notify(NotificationKind::Error, format!("Rapor derlenemedi: {e}")),
        }
    }

    /// Raporu dosyaya aktarır.
    pub fn export_report(&mut self, format: izses_report::ExportFormat, path: &std::path::Path) {
        let Some(report) = &self.report else {
            self.notify(NotificationKind::Error, "Önce rapor taslağını oluşturun");
            return;
        };
        let result = match format {
            izses_report::ExportFormat::Docx => izses_report::docx::export_docx(report, path),
            izses_report::ExportFormat::Pdf => izses_report::pdf::export_pdf(report, path),
            izses_report::ExportFormat::Html => izses_report::html::export_html(report, path),
        };
        match result {
            Ok(()) => {
                if let Some(session) = &self.session {
                    let sid = session.id().clone();
                    let _ = self.with_store(|store| {
                        store.insert_audit(&izses_core::audit::AuditEntry::new(
                            &sid,
                            izses_core::audit::ActionKind::ReportExported,
                            format!("Rapor dışa aktarıldı: {}", format.label_tr()),
                        ))
                    });
                }
                self.notify(
                    NotificationKind::Success,
                    format!("Rapor kaydedildi: {}", format.label_tr()),
                );
            }
            Err(e) => self.notify(NotificationKind::Error, format!("Dışa aktarma hatası: {e}")),
        }
    }

    /// İncelemenin yerel klasörünü açar.
    pub fn investigation_dir(&self) -> Option<std::path::PathBuf> {
        self.session
            .as_ref()
            .map(|s| s.dir(&self.root))
    }
}

impl eframe::App for IzsesApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_transcriber();

        // Bildirimleri zaman aşımına göre temizle.
        self.notifications
            .retain(|n| n.created.elapsed().as_secs() < 12);

        ui::theme::apply(ctx);

        match self.screen {
            Screen::Home => ui::home::show(self, ctx),
            Screen::Workspace => ui::workspace::show(self, ctx),
            Screen::Settings => ui::settings_screen::show(self, ctx),
        }

        ui::notifications::show(self, ctx);

        // Uzun işlem sürerken arayüzü canlı tut.
        if self.busy.active || self.transcriber.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_millis(200));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tab_labels_are_turkish() {
        assert_eq!(Tab::Overview.label(), "Genel Bakış");
        assert_eq!(Tab::Review.label(), "Metin Kontrolü");
        assert_eq!(Tab::ALL.len(), 7);
    }

    #[test]
    fn app_starts_on_home_screen() {
        let tmp = tempfile::tempdir().unwrap();
        // Ayar kökünü geçici dizine yönlendirmek için ortam değişkenini kullan.
        std::env::set_var("HOME", tmp.path());
        let app = IzsesApp::new().unwrap();
        assert_eq!(app.screen, Screen::Home);
        assert!(app.session.is_none());
    }
}
