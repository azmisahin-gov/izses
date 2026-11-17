//! Genel Bakış sekmesi: inceleme bilgileri ve işlem durumu.

use egui::RichText;

use crate::app::{IzsesApp, NotificationKind};
use crate::ui::{theme, widgets};

/// Genel bakış sekmesini çizer.
pub fn show(app: &mut IzsesApp, ui: &mut egui::Ui) {
    ui.add_space(8.0);

    let Some(session) = &app.session else { return };
    let inv = session.investigation.clone();
    let progress = session.progress();
    let outstanding = session.outstanding_work();
    let history: Vec<_> = session.history.iter().rev().take(12).cloned().collect();

    theme::heading(ui, "Genel Bakış");
    ui.add_space(4.0);

    ui.columns(2, |cols| {
        // Sol sütun: inceleme bilgileri.
        card(&mut cols[0], |ui| {
            theme::subheading(ui, "İnceleme Bilgileri");
            ui.add_space(4.0);
            theme::key_value(ui, "Dosya numarası", &dash(&inv.case_number));
            theme::key_value(ui, "Görevlendiren merci", &dash(&inv.court));
            theme::key_value(
                ui,
                "Görevlendirme tarihi",
                &inv.assignment_date
                    .map(|d| d.format("%d.%m.%Y").to_string())
                    .unwrap_or_else(|| "—".into()),
            );
            theme::key_value(
                ui,
                "Son teslim tarihi",
                &inv.due_date
                    .map(|d| d.format("%d.%m.%Y").to_string())
                    .unwrap_or_else(|| "—".into()),
            );
            theme::key_value(ui, "Bilirkişi", &dash(&inv.expert_name));
            theme::key_value(ui, "Sicil numarası", &dash(&inv.expert_registry_no));
            theme::key_value(ui, "Uzmanlık alanı", &dash(&inv.expertise_field));
            ui.add_space(4.0);
            theme::muted(ui, "İnceleme konusu");
            ui.label(RichText::new(dash(&inv.subject)).size(13.0));
        });

        // Sağ sütun: ilerleme ve eksik işler.
        card(&mut cols[1], |ui| {
            theme::subheading(ui, "İşlem Durumu");
            ui.add_space(4.0);

            progress_row(ui, "Dosya doğrulama", progress.verification_fraction(),
                &format!("{}/{}", progress.verified_files, progress.total_files));
            progress_row(ui, "Otomatik deşifre", progress.transcription_fraction(),
                &format!("{}/{}", progress.transcribed_files, progress.audio_files));

            ui.add_space(6.0);
            theme::key_value(ui, "Toplam metin bölümü", &progress.total_segments.to_string());
            theme::key_value(ui, "Düzeltilen bölüm", &progress.edited_segments.to_string());
            theme::key_value(ui, "İşaretli bölüm", &progress.flagged_segments.to_string());

            ui.add_space(8.0);
            if outstanding.is_empty() {
                theme::callout(
                    ui,
                    "Eksik işlem görünmüyor. Rapor sekmesinden raporu oluşturabilirsiniz.",
                    theme::SUCCESS,
                );
            } else {
                theme::subheading(ui, "Dikkat Gerektiren İşler");
                for item in &outstanding {
                    ui.label(RichText::new(format!("• {item}")).size(12.5).color(theme::WARNING));
                }
            }
        });
    });

    ui.add_space(12.0);

    // Taraf bilgileri ve sorular.
    ui.columns(2, |cols| {
        card(&mut cols[0], |ui| {
            theme::subheading(ui, "Taraflar");
            if inv.parties.is_empty() {
                widgets::empty_state(ui, "Taraf bilgisi girilmedi.");
            } else {
                for p in &inv.parties {
                    theme::key_value(ui, &p.role, &p.name);
                }
            }
        });

        card(&mut cols[1], |ui| {
            theme::subheading(ui, "Bilirkişiye Yöneltilen Sorular");
            if inv.questions.is_empty() {
                widgets::empty_state(ui, "Soru girilmedi.");
            } else {
                for (i, q) in inv.questions.iter().enumerate() {
                    ui.label(RichText::new(format!("{}. {q}", i + 1)).size(12.5));
                }
            }
        });
    });

    ui.add_space(12.0);

    // İşlem geçmişi.
    card(ui, |ui| {
        ui.horizontal(|ui| {
            theme::subheading(ui, "Son İşlemler");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Dışa Aktar").clicked() {
                    export_history(app);
                }
            });
        });
        ui.add_space(4.0);
        if history.is_empty() {
            widgets::empty_state(ui, "İşlem kaydı yok.");
        } else {
            for entry in &history {
                ui.horizontal(|ui| {
                    theme::muted(
                        ui,
                        &izses_core::timefmt::format_datetime(&entry.at),
                    );
                    ui.label(RichText::new(entry.kind.label_tr()).size(12.5).strong());
                    ui.label(RichText::new(&entry.summary).size(12.0));
                });
            }
        }
    });

    ui.add_space(12.0);

    // İnceleme durumu değiştirme.
    card(ui, |ui| {
        theme::subheading(ui, "İnceleme Durumu");
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            for status in [
                izses_core::model::InvestigationStatus::InProgress,
                izses_core::model::InvestigationStatus::Completed,
                izses_core::model::InvestigationStatus::Archived,
            ] {
                let selected = app
                    .session
                    .as_ref()
                    .map(|s| s.investigation.status == status)
                    .unwrap_or(false);
                if ui.selectable_label(selected, status.label_tr()).clicked() && !selected {
                    let result = {
                        let store_arc = app.store_handle();
                        let store = store_arc.lock().unwrap();
                        app.session
                            .as_mut()
                            .unwrap()
                            .set_status(&store, status)
                    };
                    match result {
                        Ok(()) => {
                            app.notify(NotificationKind::Info, "Durum güncellendi");
                            app.refresh_session();
                        }
                        Err(e) => {
                            app.notify(NotificationKind::Error, format!("Güncellenemedi: {e}"))
                        }
                    }
                }
            }
        });
    });
}

fn export_history(app: &mut IzsesApp) {
    let Some(session) = &app.session else { return };
    let dir = app.root.sub_dir(
        &session.storage_key,
        izses_core::investigation::SubDir::History,
    );
    if std::fs::create_dir_all(&dir).is_err() {
        app.notify(NotificationKind::Error, "Geçmiş klasörü oluşturulamadı");
        return;
    }
    let text = {
        let store_arc = app.store_handle();
        let store = store_arc.lock().unwrap();
        store.export_audit_text(session.id())
    };
    match text {
        Ok(text) => {
            let path = dir.join(format!(
                "islem_gecmisi_{}.txt",
                chrono::Local::now().format("%Y%m%d_%H%M%S")
            ));
            match std::fs::write(&path, text) {
                Ok(()) => app.notify(
                    NotificationKind::Success,
                    format!("İşlem geçmişi kaydedildi: {}", path.display()),
                ),
                Err(e) => app.notify(NotificationKind::Error, format!("Yazılamadı: {e}")),
            }
        }
        Err(e) => app.notify(NotificationKind::Error, format!("Geçmiş alınamadı: {e}")),
    }
}

fn progress_row(ui: &mut egui::Ui, label: &str, fraction: f32, detail: &str) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(format!("{label}:")).size(12.5));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            theme::muted(ui, detail);
        });
    });
    ui.add(
        egui::ProgressBar::new(fraction.clamp(0.0, 1.0))
            .desired_width(ui.available_width())
            .desired_height(10.0),
    );
    ui.add_space(4.0);
}

fn dash(s: &str) -> String {
    if s.trim().is_empty() {
        "—".to_string()
    } else {
        s.to_string()
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
