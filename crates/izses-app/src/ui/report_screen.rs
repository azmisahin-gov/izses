//! Rapor sekmesi: önizleme ve dışa aktarma.

use egui::RichText;

use izses_report::{CheckSeverity, ExportFormat};

use crate::app::{IzsesApp, NotificationKind};
use crate::ui::{theme, widgets};

/// Rapor sekmesini çizer.
pub fn show(app: &mut IzsesApp, ui: &mut egui::Ui) {
    ui.add_space(8.0);
    theme::heading(ui, "Rapor");
    theme::muted(
        ui,
        "Rapor, inceleme boyunca girilen bilgilerden otomatik derlenir. \
         Eksik ve doğrulanmamış işler raporda yapılmış gibi gösterilmez.",
    );
    ui.add_space(8.0);

    // Bilirkişinin kendi metinleri.
    card(ui, |ui| {
        theme::subheading(ui, "Bilirkişinin Kendi Değerlendirmesi");
        theme::muted(
            ui,
            "Bu bölümler sizin mesleki değerlendirmenizdir; otomatik derlenen \
             bölümlerden ayrı tutulur.",
        );
        ui.add_space(4.0);

        ui.label("İnceleme yöntemi notları (isteğe bağlı):");
        ui.add(
            egui::TextEdit::multiline(&mut app.method_notes)
                .desired_rows(2)
                .desired_width(f32::INFINITY),
        );

        ui.add_space(4.0);
        ui.label("Bilirkişinin değerlendirmesi:");
        ui.add(
            egui::TextEdit::multiline(&mut app.expert_assessment)
                .desired_rows(4)
                .desired_width(f32::INFINITY),
        );

        ui.add_space(4.0);
        ui.label("Gerekçeli sonuç:");
        ui.add(
            egui::TextEdit::multiline(&mut app.conclusion)
                .desired_rows(3)
                .desired_width(f32::INFINITY),
        );

        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if ui.button("Rapor Taslağını Oluştur").clicked() {
                app.build_report();
            }
            if !app.report_status.is_empty() {
                theme::muted(ui, &app.report_status);
            }
        });
    });

    ui.add_space(12.0);

    let Some(report) = app.report.clone() else {
        card(ui, |ui| {
            widgets::empty_state(
                ui,
                "Henüz rapor taslağı oluşturulmadı. Yukarıdaki düğmeye basın.",
            );
        });
        return;
    };
    let report = &report;

    // Tutarlılık kontrolü.
    let checks: Vec<(CheckSeverity, String)> = report
        .checks
        .iter()
        .map(|c| (c.severity, c.message.clone()))
        .collect();

    card(ui, |ui| {
        theme::subheading(ui, "Rapor Öncesi Tutarlılık Kontrolü");
        ui.add_space(4.0);
        for (severity, message) in &checks {
            let color = match severity {
                CheckSeverity::Blocker => theme::ERROR,
                CheckSeverity::Warning => theme::WARNING,
                CheckSeverity::Info => theme::SUCCESS,
            };
            ui.horizontal(|ui| {
                theme::badge(ui, severity.label_tr(), color);
                ui.label(RichText::new(message).size(12.5));
            });
        }
        ui.add_space(4.0);
        if report.has_blockers() {
            theme::callout(
                ui,
                "Engelleyici eksikler var. Rapor üretilebilir, ancak eksikler \
                 giderilmeden nihai rapor olarak kullanılmamalıdır.",
                theme::ERROR,
            );
        }
    });

    ui.add_space(12.0);

    // Dışa aktarma.
    card(ui, |ui| {
        theme::subheading(ui, "Dışa Aktarma");
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            if ui.button("DOCX (Word) olarak kaydet").clicked() {
                export(app, ExportFormat::Docx);
            }
            if ui.button("PDF olarak kaydet").clicked() {
                export(app, ExportFormat::Pdf);
            }
            if ui.button("HTML olarak kaydet").clicked() {
                export(app, ExportFormat::Html);
            }
        });
        ui.add_space(4.0);
        theme::muted(
            ui,
            "Raporlar incelemenin 'raporlar' klasörüne kaydedilir. \
             E-imza üretilmez ve hiçbir resmi portala gönderim yapılmaz.",
        );
    });

    ui.add_space(12.0);

    // Önizleme.
    card(ui, |ui| {
        theme::subheading(ui, "Rapor Önizlemesi");
        ui.add_space(4.0);
        ui.label(RichText::new(&report.title).size(15.0).strong());
        ui.add_space(6.0);

        egui::ScrollArea::vertical()
            .max_height(500.0)
            .show(ui, |ui| {
                for section in report.all_sections() {
                    ui.label(
                        RichText::new(&section.heading)
                            .size(13.5)
                            .strong()
                            .color(theme::ACCENT),
                    );
                    for p in &section.paragraphs {
                        ui.label(RichText::new(p).size(12.5));
                    }
                    for b in &section.bullets {
                        ui.label(RichText::new(format!("• {b}")).size(12.5));
                    }
                    if let Some(table) = &section.table {
                        ui.add_space(2.0);
                        egui::Grid::new(format!("tablo_{}", section.heading))
                            .striped(true)
                            .spacing([10.0, 4.0])
                            .show(ui, |ui| {
                                for h in &table.headers {
                                    ui.label(RichText::new(h).strong().size(11.5));
                                }
                                ui.end_row();
                                for row in &table.rows {
                                    for c in row {
                                        ui.label(RichText::new(c).size(11.5));
                                    }
                                    ui.end_row();
                                }
                            });
                    }
                    ui.add_space(6.0);
                }

                if !report.transcript_blocks.is_empty() {
                    ui.label(
                        RichText::new("Zaman Damgalı Deşifre Metinleri")
                            .size(13.5)
                            .strong()
                            .color(theme::ACCENT),
                    );
                    for block in &report.transcript_blocks {
                        ui.label(
                            RichText::new(format!(
                                "{} ({})",
                                block.file_name, block.source_label
                            ))
                            .size(12.5)
                            .strong(),
                        );
                        for seg in &block.segments {
                            ui.label(
                                RichText::new(format!(
                                    "{}–{}  {}",
                                    izses_core::timefmt::format_ms(seg.start_ms),
                                    izses_core::timefmt::format_ms(seg.end_ms),
                                    seg.effective_text()
                                ))
                                .size(12.0),
                            );
                        }
                        ui.add_space(4.0);
                    }
                }
            });
    });
}

fn export(app: &mut IzsesApp, format: ExportFormat) {
    let Some(session) = &app.session else { return };
    let dir = app.root.sub_dir(
        &session.storage_key,
        izses_core::investigation::SubDir::Reports,
    );
    if let Err(e) = std::fs::create_dir_all(&dir) {
        app.notify(
            NotificationKind::Error,
            format!("Rapor klasörü oluşturulamadı: {e}"),
        );
        return;
    }

    let default_name = format!(
        "rapor_{}_{}.{}",
        session.storage_key,
        chrono::Local::now().format("%Y%m%d_%H%M%S"),
        format.extension()
    );

    let Some(path) = rfd::FileDialog::new()
        .set_file_name(&default_name)
        .set_directory(&dir)
        .add_filter(format.label_tr(), &[format.extension()])
        .save_file()
    else {
        return;
    };

    app.export_report(format, &path);
}

fn card(ui: &mut egui::Ui, contents: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::new()
        .fill(egui::Color32::WHITE)
        .stroke(egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(214, 218, 224)))
        .inner_margin(egui::Margin::same(12))
        .corner_radius(6.0)
        .show(ui, contents);
}
