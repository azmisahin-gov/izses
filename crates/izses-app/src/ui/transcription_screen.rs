//! Otomatik Deşifre sekmesi: kuyruk durumu ve işlem kontrolü.

use egui::RichText;

use izses_core::transcript::TranscriptStatus;

use crate::app::IzsesApp;
use crate::services::transcription::{self, QueueState};
use crate::ui::{theme, widgets};

/// Otomatik deşifre sekmesini çizer.
pub fn show(app: &mut IzsesApp, ui: &mut egui::Ui) {
    ui.add_space(8.0);
    theme::heading(ui, "Otomatik Deşifre");
    theme::muted(
        ui,
        "Ses kayıtları yerel yapay zekâ ile metne dönüştürülür. İşlem sırasında \
         ses verisi dış sunucuya gönderilmez. Kesinti hâlinde tamamlanan parçalar korunur.",
    );
    ui.add_space(8.0);

    // Model durumu.
    model_status(app, ui);
    ui.add_space(12.0);

    // Kuyruk kontrolü.
    queue_controls(app, ui);
    ui.add_space(12.0);

    // İşlem listesi.
    job_list(app, ui);
}

fn model_status(app: &mut IzsesApp, ui: &mut egui::Ui) {
    card(ui, |ui| {
        theme::subheading(ui, "Model Durumu");
        ui.add_space(4.0);

        let model_store = izses_asr::model_store::ModelStore::new(app.settings.model_root.clone());
        let spec = &izses_asr::model_store::LARGE_V3_TURBO;
        let state = model_store.check_present(spec);

        ui.horizontal(|ui| {
            ui.label(RichText::new("Model:").strong());
            ui.label(spec.display_name);
            if let Some(device) = app.compute_device {
                theme::badge(ui, device.label_tr(), theme::ACCENT);
            }
        });

        let color = if state.is_ready() {
            theme::SUCCESS
        } else {
            theme::ERROR
        };
        theme::callout(ui, &state.label_tr(), color);

        if !state.is_ready() {
            ui.add_space(4.0);
            theme::muted(
                ui,
                "Model kurulmadan deşifre başlatılamaz. Kurulum paketi modeli \
                 otomatik yerleştirir.",
            );
        }
    });
}

fn queue_controls(app: &mut IzsesApp, ui: &mut egui::Ui) {
    let (pending, resumable) = match &app.session {
        Some(s) => {
            let sid = s.id().clone();
            let store_arc = app.store_handle();
            let store = store_arc.lock().unwrap();
            (
                transcription::pending_audio_items(&store, &sid).unwrap_or_default(),
                transcription::resumable_items(&store, &sid).unwrap_or_default(),
            )
        }
        None => (Vec::new(), Vec::new()),
    };

    card(ui, |ui| {
        theme::subheading(ui, "İşlem Kuyruğu");
        ui.add_space(4.0);

        let running = app.queue_state == QueueState::Running || app.transcriber.is_some();

        ui.horizontal(|ui| {
            if ui
                .add_enabled(
                    !running && !pending.is_empty(),
                    egui::Button::new(format!("Deşifreyi Başlat ({} kayıt)", pending.len())),
                )
                .clicked()
            {
                app.start_transcription(pending.clone());
            }

            if ui
                .add_enabled(
                    !running && !resumable.is_empty(),
                    egui::Button::new(format!("Kaldığı Yerden Sürdür ({})", resumable.len())),
                )
                .clicked()
            {
                app.start_transcription(resumable.clone());
            }

            if ui
                .add_enabled(running, egui::Button::new("Durdur"))
                .clicked()
            {
                app.stop_transcription();
            }
        });

        ui.add_space(4.0);
        theme::muted(
            ui,
            &format!(
                "Kuyrukta bekleyen: {} · Sürdürülebilir: {} · Durum: {}",
                pending.len(),
                resumable.len(),
                queue_state_label(app.queue_state)
            ),
        );

        if running {
            ui.add_space(6.0);
            theme::progress(
                ui,
                app.busy.fraction,
                if app.busy.label.is_empty() {
                    "İşleniyor…"
                } else {
                    &app.busy.label
                },
            );
        }
    });
}

fn job_list(app: &mut IzsesApp, ui: &mut egui::Ui) {
    let files = app
        .session
        .as_ref()
        .map(|s| s.audio_files().into_iter().cloned().collect::<Vec<_>>())
        .unwrap_or_default();

    card(ui, |ui| {
        theme::subheading(ui, &format!("Kayıtlar ({})", files.len()));
        ui.add_space(4.0);

        if files.is_empty() {
            widgets::empty_state(ui, "İncelemeye alınmış ses dosyası yok.");
            return;
        }

        for file in &files {
            let transcript = app
                .session
                .as_ref()
                .and_then(|s| s.transcript_for_file(&file.id).cloned());

            ui.horizontal(|ui| {
                ui.label(RichText::new(file.file_name()).size(12.5).strong());

                match &transcript {
                    Some(t) => {
                        let color = match t.status {
                            TranscriptStatus::Completed => theme::SUCCESS,
                            TranscriptStatus::Running => theme::ACCENT,
                            TranscriptStatus::Queued => theme::INFO,
                            TranscriptStatus::Failed => theme::ERROR,
                            TranscriptStatus::Cancelled => theme::WARNING,
                        };
                        theme::badge(ui, t.status.label_tr(), color);

                        if let Some(progress) = t.progress() {
                            theme::muted(
                                ui,
                                &format!(
                                    "{}/{} parça",
                                    t.completed_segments,
                                    t.total_segments.unwrap_or(0)
                                ),
                            );
                            let _ = progress;
                        }

                        if let Some(err) = &t.error {
                            theme::muted(ui, err);
                        }
                    }
                    None => {
                        theme::badge(ui, "Sırada değil", theme::INFO);
                    }
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.small_button("Kontrol Et").clicked() {
                        app.review_file = Some(file.id.clone());
                        app.tab = crate::app::Tab::Review;
                    }
                });
            });
            ui.add_space(2.0);
        }
    });
}

fn queue_state_label(state: QueueState) -> &'static str {
    match state {
        QueueState::Idle => "beklemede",
        QueueState::Running => "çalışıyor",
        QueueState::Stopping => "durduruluyor",
        QueueState::Finished => "tamamlandı",
    }
}

fn card(ui: &mut egui::Ui, contents: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::new()
        .fill(egui::Color32::WHITE)
        .stroke(egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(214, 218, 224)))
        .inner_margin(egui::Margin::same(12))
        .corner_radius(6.0)
        .show(ui, contents);
}
