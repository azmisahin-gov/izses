//! Ses Kayıtları sekmesi: teknik envanter ve dinleme araçları.

use egui::RichText;

use izses_core::inventory::EvidenceFile;

use crate::app::{IzsesApp, NotificationKind};
use crate::ui::{theme, widgets};

/// Ses kayıtları sekmesini çizer.
pub fn show(app: &mut IzsesApp, ui: &mut egui::Ui) {
    ui.add_space(8.0);
    theme::heading(ui, "Ses Kayıtları");
    theme::muted(
        ui,
        "İncelemeye alınan ses dosyalarının teknik envanteri. Orijinal dosyalar \
         değiştirilmez; iyileştirme gerektiğinde türetilmiş kopya oluşturulur.",
    );
    ui.add_space(8.0);

    let files: Vec<EvidenceFile> = app
        .session
        .as_ref()
        .map(|s| s.files.iter().filter(|f| f.is_audio()).cloned().collect())
        .unwrap_or_default();

    if files.is_empty() {
        card(ui, |ui| {
            widgets::empty_state(
                ui,
                "Henüz ses dosyası yok. Delil Materyalleri sekmesinden ses dosyalarını \
                 incelemeye aktarın.",
            );
        });
        return;
    }

    // Envanter tablosu.
    card(ui, |ui| {
        theme::subheading(ui, &format!("Teknik Envanter ({})", files.len()));
        ui.add_space(6.0);

        egui::ScrollArea::horizontal().show(ui, |ui| {
            egui::Grid::new("ses_envanteri")
                .num_columns(9)
                .striped(true)
                .spacing([12.0, 6.0])
                .show(ui, |ui| {
                    for h in [
                        "Dosya",
                        "Biçim",
                        "Kodek",
                        "Süre",
                        "Kanal",
                        "Örnekleme",
                        "Bit",
                        "Boyut",
                        "Doğrulama",
                    ] {
                        ui.label(RichText::new(h).strong().size(12.5));
                    }
                    ui.end_row();

                    for f in &files {
                        let info = f.audio.clone().unwrap_or_default();
                        ui.label(RichText::new(f.file_name()).size(12.0));
                        ui.label(info.container.clone().unwrap_or_else(|| "—".into()));
                        ui.label(info.codec.clone().unwrap_or_else(|| "—".into()));
                        ui.label(info.duration_label());
                        ui.label(
                            info.channels
                                .map(|c| c.to_string())
                                .unwrap_or_else(|| "—".into()),
                        );
                        ui.label(
                            info.sample_rate
                                .map(|r| format!("{r} Hz"))
                                .unwrap_or_else(|| "—".into()),
                        );
                        ui.label(
                            info.bit_depth
                                .map(|b| b.to_string())
                                .unwrap_or_else(|| "—".into()),
                        );
                        ui.label(izses_core::device::format_size(f.size_bytes));
                        ui.horizontal(|ui| widgets::verification_badge(ui, f.verification));
                        ui.end_row();
                    }
                });
        });
    });

    ui.add_space(12.0);

    // Sorunlu dosyalar.
    let problematic: Vec<&EvidenceFile> = files
        .iter()
        .filter(|f| {
            f.issue.is_some()
                || f.audio
                    .as_ref()
                    .and_then(|a| a.probe_error.as_ref())
                    .is_some()
        })
        .collect();
    if !problematic.is_empty() {
        card(ui, |ui| {
            theme::subheading(ui, "Desteklenmeyen veya Hasarlı Dosyalar");
            ui.add_space(4.0);
            for f in problematic {
                let reason = f
                    .issue
                    .clone()
                    .or_else(|| f.audio.as_ref().and_then(|a| a.probe_error.clone()))
                    .unwrap_or_default();
                theme::callout(ui, &format!("{}: {reason}", f.file_name()), theme::WARNING);
            }
        });
        ui.add_space(12.0);
    }

    // Dinleme ve yardımcı araçlar.
    card(ui, |ui| {
        theme::subheading(ui, "Dinleme ve İnceleme Araçları");
        theme::muted(
            ui,
            "Bir kaydı seçip dinleyebilir, dalga formunu ve spektrogramı \
             inceleyebilirsiniz. Metin kontrolü Metin Kontrolü sekmesinde yapılır.",
        );
        ui.add_space(6.0);

        let selected_index = app.audio_selected;
        egui::ComboBox::from_id_salt("ses_secimi")
            .selected_text(
                selected_index
                    .and_then(|i| files.get(i))
                    .map(|f| f.file_name())
                    .unwrap_or_else(|| "Kayıt seçin".to_string()),
            )
            .width(320.0)
            .show_ui(ui, |ui| {
                for (i, f) in files.iter().enumerate() {
                    if ui
                        .selectable_label(selected_index == Some(i), f.file_name())
                        .clicked()
                    {
                        app.audio_selected = Some(i);
                        app.review_file = Some(f.id.clone());
                    }
                }
            });

        ui.add_space(8.0);

        if let Some(i) = selected_index {
            if let Some(file) = files.get(i) {
                let path = app.session.as_ref().map(|s| s.local_path(&app.root, file));

                if let Some(path) = path {
                    // Dalga formu ve spektrogram.
                    match izses_audio::decode_to_mono_16k(&path) {
                        Ok(audio) => {
                            ui.horizontal(|ui| {
                                theme::key_value(
                                    ui,
                                    "Süre",
                                    &izses_core::timefmt::format_ms(audio.duration_ms),
                                );
                                theme::key_value(
                                    ui,
                                    "Kaynak kanal",
                                    &audio.source_channels.to_string(),
                                );
                                theme::key_value(
                                    ui,
                                    "Kaynak örnekleme",
                                    &format!("{} Hz", audio.source_sample_rate),
                                );
                            });
                            ui.add_space(6.0);

                            theme::muted(ui, "Dalga formu");
                            draw_waveform(ui, &audio.samples);

                            ui.add_space(8.0);
                            theme::muted(ui, "Spektrogram");
                            draw_spectrogram(ui, &audio.samples);

                            ui.add_space(8.0);
                            if audio.source_channels >= 2 && audio.channels.len() >= 2 {
                                theme::muted(
                                    ui,
                                    "Stereo kanal dinleme: sol/sağ kanalı ayrı \
                                     dinlemek için aşağıdaki seçimi kullanın.",
                                );
                                ui.horizontal(|ui| {
                                    for (label, ch) in [("Sol", 0usize), ("Sağ", 1)] {
                                        if ui.button(label).clicked() {
                                            app.notify(
                                                NotificationKind::Info,
                                                format!(
                                                    "{label} kanal seçildi ({} örnek)",
                                                    audio.channels[ch].len()
                                                ),
                                            );
                                        }
                                    }
                                });
                            }
                        }
                        Err(e) => {
                            theme::callout(
                                ui,
                                &format!("Ses dosyası çözülemedi: {e}"),
                                theme::ERROR,
                            );
                        }
                    }
                }

                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button("Metin Kontrolü Sekmesine Git").clicked() {
                        app.review_file = Some(file.id.clone());
                        app.tab = crate::app::Tab::Review;
                    }
                });
            }
        } else {
            widgets::empty_state(ui, "İncelemek için bir kayıt seçin.");
        }
    });
}

/// Dalga formunu çizer.
fn draw_waveform(ui: &mut egui::Ui, samples: &[f32]) {
    let bins = izses_audio::dsp::waveform(samples, 600);
    let height = 80.0;
    let width = ui.available_width().min(900.0);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 3.0, egui::Color32::from_rgb(250, 251, 252));

    let mid = rect.center().y;
    let half = rect.height() / 2.0 - 2.0;
    let bar_width = rect.width() / bins.len().max(1) as f32;

    for (i, bin) in bins.iter().enumerate() {
        let x = rect.left() + i as f32 * bar_width;
        let h = bin.peak * half;
        painter.line_segment(
            [egui::pos2(x, mid - h), egui::pos2(x, mid + h)],
            egui::Stroke::new(bar_width.max(0.5), theme::ACCENT.gamma_multiply(0.8)),
        );
    }
    painter.line_segment(
        [egui::pos2(rect.left(), mid), egui::pos2(rect.right(), mid)],
        egui::Stroke::new(0.5_f32, egui::Color32::from_gray(180)),
    );
}

/// Spektrogramı çizer.
fn draw_spectrogram(ui: &mut egui::Ui, samples: &[f32]) {
    let rows = 100;
    let spec = izses_audio::dsp::spectrogram(samples, 256, 128, rows);
    if spec.is_empty() {
        return;
    }
    let height = 140.0;
    let width = ui.available_width().min(900.0);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::hover());
    let painter = ui.painter_at(rect);

    // Maksimum değeri bul (görünümü normalize etmek için).
    let max_val = spec
        .iter()
        .flat_map(|r| r.iter())
        .copied()
        .fold(0f32, f32::max)
        .max(1e-9);

    let row_h = rect.height() / spec.len() as f32;
    for (ri, row) in spec.iter().enumerate() {
        if row.is_empty() {
            continue;
        }
        let col_w = rect.width() / row.len() as f32;
        for (ci, &v) in row.iter().enumerate() {
            let intensity = (v / max_val).clamp(0.0, 1.0).powf(0.35);
            let color = heat_color(intensity);
            let x = rect.left() + ci as f32 * col_w;
            let y = rect.bottom() - (ri + 1) as f32 * row_h;
            painter.rect_filled(
                egui::Rect::from_min_size(egui::pos2(x, y), egui::vec2(col_w + 0.5, row_h + 0.5)),
                0.0,
                color,
            );
        }
    }
}

/// Isı haritası rengi (düşük: koyu mavi, yüksek: sarı/kırmızı).
fn heat_color(t: f32) -> egui::Color32 {
    let t = t.clamp(0.0, 1.0);
    let r = (t * 255.0).min(255.0) as u8;
    let g = (t.powf(0.7) * 200.0) as u8;
    let b = ((1.0 - t) * 120.0) as u8;
    egui::Color32::from_rgb(r, g, b)
}

fn card(ui: &mut egui::Ui, contents: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::new()
        .fill(egui::Color32::WHITE)
        .stroke(egui::Stroke::new(
            1.0_f32,
            egui::Color32::from_rgb(214, 218, 224),
        ))
        .inner_margin(egui::Margin::same(12))
        .corner_radius(6.0)
        .show(ui, contents);
}
