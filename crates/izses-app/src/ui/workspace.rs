//! Çalışma alanı: açık incelemenin sekmeli arayüzü.
//!
//! Bilirkişi her zaman hangi incelemede bulunduğunu görür. Sekmeler tek bir
//! iş akışını izler: Genel Bakış → Delil → Ses → Deşifre → Kontrol → Notlar → Rapor.

use egui::RichText;

use crate::app::{IzsesApp, NotificationKind, Screen, Tab};
use crate::ui::{
    audio_player, evidence, overview, report_screen, review, theme, transcription_screen, widgets,
};

/// Çalışma alanını çizer.
pub fn show(app: &mut IzsesApp, ctx: &egui::Context) {
    let Some(session) = app.session.as_ref() else {
        app.screen = Screen::Home;
        return;
    };

    let title = session.investigation.display_title();
    let status = session.investigation.status;
    let storage_key = session.storage_key.clone();
    let progress = session.progress();

    egui::TopBottomPanel::top("calisma_ust_bar").show(ctx, |ui| {
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            if ui.button("← İncelemeler").clicked() {
                app.close_investigation();
                return;
            }
            ui.separator();
            ui.label(RichText::new(&title).size(15.0).strong());
            ui.separator();
            theme::badge(ui, status.label_tr(), status_color(status));
            ui.separator();
            theme::muted(ui, &format!("Klasör: {storage_key}"));

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Ayarlar").clicked() {
                    app.screen = Screen::Settings;
                }
                if ui.button("Klasörü Aç").clicked() {
                    if let Some(dir) = app.investigation_dir() {
                        open_folder(&dir);
                    }
                }
            });
        });

        ui.add_space(4.0);

        // Sekmeler.
        ui.horizontal_wrapped(|ui| {
            for tab in Tab::ALL {
                let selected = app.tab == tab;
                let label = tab_label(app, tab);
                if ui.selectable_label(selected, label).clicked() {
                    app.tab = tab;
                }
            }
        });

        ui.add_space(4.0);

        // İlerleme özeti.
        ui.horizontal(|ui| {
            theme::muted(
                ui,
                &format!(
                    "Dosya: {} · Doğrulanan: {} · Ses: {} · Deşifre: {} · Bölüm: {} (düzeltilen {})",
                    progress.total_files,
                    progress.verified_files,
                    progress.audio_files,
                    progress.transcribed_files,
                    progress.total_segments,
                    progress.edited_segments
                ),
            );
        });

        ui.add_space(6.0);
    });

    // Çalışma alanının alt kısmı: durum çubuğu.
    egui::TopBottomPanel::bottom("durum_cubugu").show(ctx, |ui| {
        ui.add_space(3.0);
        ui.horizontal(|ui| {
            if app.busy.active {
                theme::progress(ui, app.busy.fraction, &app.busy.label);
            } else if let Some(device) = app.compute_device {
                theme::muted(ui, &format!("Çıkarım cihazı: {}", device.label_tr()));
            } else {
                theme::muted(ui, "Hazır");
            }
        });
        ui.add_space(3.0);
    });

    egui::CentralPanel::default().show(ctx, |ui| {
        egui::ScrollArea::vertical().show(ui, |ui| match app.tab {
            Tab::Overview => overview::show(app, ui),
            Tab::Evidence => evidence::show(app, ui),
            Tab::Audio => audio_player::show(app, ui),
            Tab::Transcription => transcription_screen::show(app, ui),
            Tab::Review => review::show(app, ui),
            Tab::Notes => notes_placeholder(app, ui),
            Tab::Report => report_screen::show(app, ui),
        });
    });
}

/// Sekme etiketleri (sayaçlarla).
fn tab_label(app: &IzsesApp, tab: Tab) -> String {
    let Some(session) = &app.session else {
        return tab.label().to_string();
    };
    let count = match tab {
        Tab::Evidence => Some(session.sources.len()),
        Tab::Audio => Some(session.audio_files().len()),
        Tab::Transcription => {
            let pending = session
                .audio_files()
                .iter()
                .filter(|f| session.transcript_for_file(&f.id).is_none())
                .count();
            Some(pending)
        }
        Tab::Notes => Some(session.notes.len()),
        _ => None,
    };
    match count {
        Some(n) if n > 0 => format!("{} ({n})", tab.label()),
        _ => tab.label().to_string(),
    }
}

fn status_color(status: izses_core::model::InvestigationStatus) -> egui::Color32 {
    use izses_core::model::InvestigationStatus::*;
    match status {
        Draft => theme::INFO,
        InProgress => theme::ACCENT,
        Completed => theme::SUCCESS,
        Archived => egui::Color32::GRAY,
    }
}

/// Notlar sekmesi burada tanımlanır (ayrı modül gerektirmez).
fn notes_placeholder(app: &mut IzsesApp, ui: &mut egui::Ui) {
    use izses_core::transcript::{Note, NoteKind};

    ui.add_space(8.0);
    theme::heading(ui, "Notlar ve Bulgular");
    theme::muted(
        ui,
        "İnceleme sırasında aldığınız notlar ve teknik gözlemler burada tutulur.",
    );
    ui.add_space(8.0);

    egui::Frame::new()
        .fill(egui::Color32::WHITE)
        .stroke(egui::Stroke::new(
            1.0_f32,
            egui::Color32::from_rgb(214, 218, 224),
        ))
        .inner_margin(egui::Margin::same(12))
        .corner_radius(6.0)
        .show(ui, |ui| {
            theme::subheading(ui, "Yeni Not");

            // Yeni not durumu, app içinde geçici olarak tutulur.
            let mut kind = app.new_note_kind;
            let mut title = app.new_note_title.clone();
            let mut body = app.new_note_body.clone();

            ui.horizontal(|ui| {
                ui.label("Tür:");
                egui::ComboBox::from_id_salt("not_turu")
                    .selected_text(kind.label_tr())
                    .show_ui(ui, |ui| {
                        for k in [
                            NoteKind::General,
                            NoteKind::Technical,
                            NoteKind::Observation,
                            NoteKind::Finding,
                        ] {
                            ui.selectable_value(&mut kind, k, k.label_tr());
                        }
                    });
            });
            ui.horizontal(|ui| {
                ui.label("Başlık:");
                ui.add(egui::TextEdit::singleline(&mut title).desired_width(f32::INFINITY));
            });
            ui.label("İçerik:");
            ui.add(
                egui::TextEdit::multiline(&mut body)
                    .desired_rows(3)
                    .desired_width(f32::INFINITY),
            );

            app.new_note_kind = kind;
            app.new_note_title = title;
            app.new_note_body = body;

            ui.add_space(4.0);
            let can_add =
                !app.new_note_body.trim().is_empty() || !app.new_note_title.trim().is_empty();
            if ui
                .add_enabled(can_add, egui::Button::new("Notu Ekle"))
                .clicked()
            {
                let note = Note::new(
                    app.session.as_ref().unwrap().id(),
                    app.new_note_kind,
                    if app.new_note_title.trim().is_empty() {
                        "Not".to_string()
                    } else {
                        app.new_note_title.clone()
                    },
                    app.new_note_body.clone(),
                );
                let result = {
                    let store_arc = app.store_handle();
                    let store = store_arc.lock().unwrap();
                    app.session.as_mut().unwrap().add_note(&store, note)
                };
                match result {
                    Ok(_) => {
                        app.new_note_title.clear();
                        app.new_note_body.clear();
                        app.notify(NotificationKind::Success, "Not eklendi");
                    }
                    Err(e) => app.notify(NotificationKind::Error, format!("Not eklenemedi: {e}")),
                }
            }
        });

    ui.add_space(12.0);
    theme::subheading(ui, "Kayıtlı Notlar");

    let notes: Vec<_> = app
        .session
        .as_ref()
        .map(|s| s.notes.clone())
        .unwrap_or_default();

    if notes.is_empty() {
        widgets::empty_state(ui, "Henüz not eklenmedi.");
        return;
    }

    let mut to_delete = None;
    for note in &notes {
        egui::Frame::new()
            .fill(egui::Color32::WHITE)
            .stroke(egui::Stroke::new(
                1.0_f32,
                egui::Color32::from_rgb(226, 230, 236),
            ))
            .inner_margin(egui::Margin::same(10))
            .corner_radius(4.0)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    theme::badge(ui, note.kind.label_tr(), theme::INFO);
                    ui.label(RichText::new(&note.title).strong().size(13.5));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button("Sil").clicked() {
                            to_delete = Some(note.id.clone());
                        }
                        theme::muted(ui, &izses_core::timefmt::format_datetime(&note.created_at));
                    });
                });
                ui.label(RichText::new(&note.body).size(12.5));
            });
        ui.add_space(4.0);
    }

    if let Some(id) = to_delete {
        let result = {
            let store_arc = app.store_handle();
            let store = store_arc.lock().unwrap();
            app.session.as_mut().unwrap().delete_note(&store, &id)
        };
        match result {
            Ok(()) => app.notify(NotificationKind::Info, "Not silindi"),
            Err(e) => app.notify(NotificationKind::Error, format!("Silinemedi: {e}")),
        }
    }
}

/// İşletim sistemi klasör açma komutu.
pub fn open_folder(path: &std::path::Path) {
    #[cfg(windows)]
    {
        let _ = std::process::Command::new("explorer").arg(path).spawn();
    }
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open").arg(path).spawn();
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let _ = std::process::Command::new("xdg-open").arg(path).spawn();
    }
}
