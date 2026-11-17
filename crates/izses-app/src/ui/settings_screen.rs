//! Ayarlar ekranı: depolama, model ve uygulama tercihleri.

use egui::RichText;

use izses_asr::model_store::{ModelStore, LARGE_V3_TURBO};

use crate::app::{IzsesApp, NotificationKind, Screen};
use crate::ui::theme;

/// Ayarlar ekranını çizer.
pub fn show(app: &mut IzsesApp, ctx: &egui::Context) {
    egui::TopBottomPanel::top("ayar_ust_bar").show(ctx, |ui| {
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            if ui.button("← Geri").clicked() {
                app.screen = if app.session.is_some() {
                    Screen::Workspace
                } else {
                    Screen::Home
                };
            }
            ui.label(RichText::new("Ayarlar").size(18.0).strong());
        });
        ui.add_space(6.0);
    });

    egui::CentralPanel::default().show(ctx, |ui| {
        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.add_space(8.0);

            storage_section(app, ui);
            ui.add_space(12.0);
            model_section(app, ui);
            ui.add_space(12.0);
            app_section(app, ui);
        });
    });
}

fn storage_section(app: &mut IzsesApp, ui: &mut egui::Ui) {
    card(ui, |ui| {
        theme::subheading(ui, "Depolama");
        theme::muted(
            ui,
            "İnceleme verileri bu klasörde saklanır. Her inceleme kendi alt klasörüne sahiptir.",
        );
        ui.add_space(4.0);

        ui.horizontal(|ui| {
            ui.label("İnceleme kök klasörü:");
            let mut text = app.settings.storage_root.to_string_lossy().to_string();
            if ui
                .add(egui::TextEdit::singleline(&mut text).desired_width(420.0))
                .changed()
            {
                app.settings.storage_root = std::path::PathBuf::from(text);
            }
            if ui.button("Gözat…").clicked() {
                if let Some(dir) = rfd::FileDialog::new().pick_folder() {
                    app.settings.storage_root = dir;
                }
            }
        });

        ui.horizontal(|ui| {
            if ui.button("Kaydet ve Klasörleri Hazırla").clicked() {
                match crate::settings::ensure_dirs(&app.settings)
                    .and_then(|_| app.settings.save())
                {
                    Ok(()) => {
                        app.notify(NotificationKind::Success, "Ayarlar kaydedildi");
                        // Depolama kökü değiştiyse yeni kökü aç.
                        match izses_core::investigation::StorageRoot::new(
                            app.settings.storage_root.clone(),
                        ) {
                            Ok(root) => {
                                app.root = root;
                                app.reload_investigations();
                            }
                            Err(e) => app.notify(
                                NotificationKind::Error,
                                format!("Klasör hazırlanamadı: {e}"),
                            ),
                        }
                    }
                    Err(e) => app.notify(
                        NotificationKind::Error,
                        format!("Ayarlar kaydedilemedi: {e}"),
                    ),
                }
            }
        });
    });
}

fn model_section(app: &mut IzsesApp, ui: &mut egui::Ui) {
    card(ui, |ui| {
        theme::subheading(ui, "Konuşma Çözümleme Modeli");
        theme::muted(
            ui,
            "Model kurulum sırasında yerel bilgisayara yerleştirilir. \
             Uygulama çalışırken internetten indirme yapılmaz.",
        );
        ui.add_space(4.0);

        ui.horizontal(|ui| {
            ui.label("Model klasörü:");
            let mut text = app.settings.model_root.to_string_lossy().to_string();
            if ui
                .add(egui::TextEdit::singleline(&mut text).desired_width(420.0))
                .changed()
            {
                app.settings.model_root = std::path::PathBuf::from(text);
            }
            if ui.button("Gözat…").clicked() {
                if let Some(dir) = rfd::FileDialog::new().pick_folder() {
                    app.settings.model_root = dir;
                }
            }
        });

        ui.add_space(6.0);

        let model_store = ModelStore::new(app.settings.model_root.clone());
        let state = model_store.check_present(&LARGE_V3_TURBO);

        ui.horizontal(|ui| {
            ui.label(RichText::new("Model:").strong());
            ui.label(LARGE_V3_TURBO.display_name);
        });

        let color = if state.is_ready() {
            theme::SUCCESS
        } else {
            theme::WARNING
        };
        theme::callout(ui, &format!("Durum: {}", state.label_tr()), color);

        if state.is_ready() {
            // Ağırlık dosyası özeti (rapora ve işlem geçmişine yazılır).
            if let Ok(digest) = model_store.weights_digest(&LARGE_V3_TURBO) {
                ui.add_space(4.0);
                theme::muted(ui, &format!("Ağırlık dosyası SHA-256: {digest}"));
            }
        } else {
            ui.add_space(6.0);
            theme::callout(
                ui,
                "Model dosyaları eksik. Kurulum paketi model dosyalarını \
                 otomatik yerleştirir; elle kurulum için model klasörüne \
                 model.safetensors, tokenizer.json ve config.json dosyalarını koyun.",
                theme::INFO,
            );
        }

        ui.add_space(6.0);
        ui.horizontal(|ui| {
            if ui
                .checkbox(&mut app.settings.prefer_gpu, "GPU hızlandırmayı tercih et")
                .changed()
            {
                let _ = app.settings.save();
            }
            ui.label(
                RichText::new(
                    "(GPU kullanılamıyorsa uygulama otomatik olarak CPU ile devam eder.)",
                )
                .size(11.5)
                .color(theme::INFO),
            );
        });
    });
}

fn app_section(app: &mut IzsesApp, ui: &mut egui::Ui) {
    card(ui, |ui| {
        theme::subheading(ui, "Uygulama Tercihleri");
        ui.add_space(4.0);

        ui.horizontal(|ui| {
            ui.label("Konuşma dili:");
            egui::ComboBox::from_id_salt("dil")
                .selected_text(language_label(&app.settings.language))
                .show_ui(ui, |ui| {
                    for (code, label) in [("tr", "Türkçe"), ("en", "İngilizce")] {
                        if ui
                            .selectable_label(app.settings.language == code, label)
                            .clicked()
                        {
                            app.settings.language = code.to_string();
                            let _ = app.settings.save();
                        }
                    }
                });
        });

        ui.horizontal(|ui| {
            if ui
                .checkbox(
                    &mut app.settings.timestamps,
                    "Deşifrede zaman damgası üret",
                )
                .changed()
            {
                let _ = app.settings.save();
            }
        });

        ui.add_space(6.0);
        theme::callout(
            ui,
            "Uygulama tamamen yereldir. Ses dosyaları, deşifre metinleri, \
             notlar ve raporlar hiçbir dış sunucuya gönderilmez. \
             Çeviri yapılmaz; metin konuşulan dilde üretilir.",
            theme::INFO,
        );
    });
}

fn language_label(code: &str) -> &'static str {
    match code {
        "en" => "İngilizce",
        _ => "Türkçe",
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn language_labels_are_turkish() {
        assert_eq!(language_label("tr"), "Türkçe");
        assert_eq!(language_label("en"), "İngilizce");
        assert_eq!(language_label("xx"), "Türkçe");
    }
}
