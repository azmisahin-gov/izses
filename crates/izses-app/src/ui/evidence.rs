//! Delil Materyalleri sekmesi: aygıt keşfi, tarama ve güvenli aktarım.

use std::path::{Path, PathBuf};

use egui::RichText;

use izses_core::device::{self, DeviceInfo};
use izses_core::inventory::{self, ScanOptions};

use crate::app::{IzsesApp, NotificationKind, ScannedItem};
use crate::services::acquisition;
use crate::ui::{theme, widgets};

/// Delil sekmesini çizer.
pub fn show(app: &mut IzsesApp, ui: &mut egui::Ui) {
    ui.add_space(8.0);
    theme::heading(ui, "Delil Materyalleri");
    theme::muted(
        ui,
        "Bağlı aygıtı seçin, içindeki dosyaları tarayın ve incelemeye aktarın. \
         Kaynak dosyalar değiştirilmez; aktarım SHA-256 ile doğrulanır.",
    );
    ui.add_space(8.0);

    device_section(app, ui);
    ui.add_space(12.0);
    sources_section(app, ui);
    ui.add_space(12.0);
    scan_section(app, ui);
}

/// Aygıt keşfi ve seçimi.
fn device_section(app: &mut IzsesApp, ui: &mut egui::Ui) {
    card(ui, |ui| {
        ui.horizontal(|ui| {
            theme::subheading(ui, "Bağlı Depolama Aygıtları");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Aygıtları Yenile").clicked() {
                    app.devices = device::list_devices();
                    app.notify(NotificationKind::Info, "Aygıt listesi yenilendi");
                }
                if ui.button("Klasör Seç…").clicked() {
                    if let Some(dir) = rfd::FileDialog::new().pick_folder() {
                        if let Some(dev) = device::device_from_path(&dir) {
                            app.devices.insert(0, dev);
                            app.notify(NotificationKind::Info, "Klasör kaynak listesine eklendi");
                        }
                    }
                }
            });
        });
        ui.add_space(4.0);

        if app.devices.is_empty() {
            widgets::empty_state(
                ui,
                "Bağlı depolama aygıtı bulunamadı. Bir flash bellek, SD kart veya \
                 taşınabilir disk bağlayın; ardından 'Aygıtları Yenile' düğmesine basın.",
            );
            return;
        }

        let devices = app.devices.clone();
        let mut to_add: Option<DeviceInfo> = None;
        for dev in &devices {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(format!("💾 {}", dev.label))
                        .strong()
                        .size(13.5),
                );
                theme::muted(ui, &dev.summary());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if dev.readable {
                        if ui.button("İncelemeye Kaynak Olarak Ekle").clicked() {
                            to_add = Some(dev.clone());
                        }
                    } else {
                        ui.label(RichText::new("erişilemiyor").color(theme::ERROR).size(12.0));
                    }
                });
            });
            theme::muted(ui, &dev.root.to_string_lossy());
            ui.add_space(4.0);
        }

        if let Some(dev) = to_add {
            add_source(app, &dev);
        }
    });
}

/// İncelemeye kaynak ekler.
fn add_source(app: &mut IzsesApp, dev: &DeviceInfo) {
    let Some(session) = &app.session else { return };
    let inv_id = session.id().clone();

    let (ordinal, source) = {
        let store_arc = app.store_handle();
        let store = store_arc.lock().unwrap();
        match store.next_source_ordinal(&inv_id) {
            Ok(ord) => {
                let mut src = izses_core::transcript::EvidenceSource::new(
                    &inv_id,
                    dev.label.clone(),
                    dev.root.clone(),
                    dev.kind,
                    ord,
                );
                src.filesystem = dev.filesystem.clone();
                src.total_bytes = dev.total_bytes;
                (ord, src)
            }
            Err(e) => {
                app.notify(NotificationKind::Error, format!("Kaynak eklenemedi: {e}"));
                return;
            }
        }
    };

    let result = {
        let store_arc = app.store_handle();
        let store = store_arc.lock().unwrap();
        store.insert_source(&source).and_then(|_| {
            store.insert_audit(&izses_core::audit::AuditEntry::new(
                &inv_id,
                izses_core::audit::ActionKind::EvidenceSourceAdded,
                format!("Delil kaynağı eklendi: {}", source.label),
            ))
        })
    };

    match result {
        Ok(()) => {
            app.notify(
                NotificationKind::Success,
                format!("Kaynak eklendi: {} (kaynak_{:03})", source.label, ordinal),
            );
            app.refresh_session();
        }
        Err(e) => app.notify(NotificationKind::Error, format!("Kaynak eklenemedi: {e}")),
    }
}

/// Eklenen kaynakların listesi.
fn sources_section(app: &mut IzsesApp, ui: &mut egui::Ui) {
    let sources = app
        .session
        .as_ref()
        .map(|s| s.sources.clone())
        .unwrap_or_default();

    card(ui, |ui| {
        theme::subheading(ui, &format!("İnceleme Kaynakları ({})", sources.len()));
        ui.add_space(4.0);

        if sources.is_empty() {
            widgets::empty_state(ui, "Henüz delil kaynağı eklenmedi.");
            return;
        }

        let mut to_scan: Option<(izses_core::model::Id, PathBuf, String)> = None;
        let mut to_remove: Option<izses_core::model::Id> = None;

        for src in &sources {
            let file_count = app
                .session
                .as_ref()
                .map(|s| s.files_for_source(&src.id).len())
                .unwrap_or(0);

            egui::Frame::new()
                .fill(egui::Color32::from_rgb(250, 251, 252))
                .stroke(egui::Stroke::new(
                    1.0_f32,
                    egui::Color32::from_rgb(226, 230, 236),
                ))
                .inner_margin(egui::Margin::same(10))
                .corner_radius(4.0)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(format!("KAYNAK {:03} · {}", src.ordinal, src.label))
                                .strong()
                                .size(13.5),
                        );
                        theme::badge(ui, src.kind.label_tr(), theme::INFO);
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.button("Kaldır").clicked() {
                                to_remove = Some(src.id.clone());
                            }
                            if ui.button("Tara").clicked() {
                                to_scan = Some((
                                    src.id.clone(),
                                    src.root_path.clone(),
                                    src.folder_name(),
                                ));
                            }
                        });
                    });
                    theme::muted(
                        ui,
                        &format!(
                            "{} · Eklenen dosya: {} · Son tarama: {}",
                            src.root_path.to_string_lossy(),
                            file_count,
                            src.last_scan_at
                                .map(|t| izses_core::timefmt::format_datetime(&t))
                                .unwrap_or_else(|| "—".into())
                        ),
                    );
                });
            ui.add_space(6.0);
        }

        if let Some((id, path, folder)) = to_scan {
            scan_source(app, &id, &path, &folder);
        }
        if let Some(id) = to_remove {
            remove_source(app, &id);
        }
    });
}

/// Kaynağı tarar ve bulunan dosyaları gösterir.
fn scan_source(app: &mut IzsesApp, source_id: &izses_core::model::Id, path: &Path, _folder: &str) {
    let inv_id = app.session.as_ref().map(|s| s.id().clone());
    let Some(inv_id) = inv_id else { return };

    {
        let store_arc = app.store_handle();
        let store = store_arc.lock().unwrap();
        let _ = store.insert_audit(&izses_core::audit::AuditEntry::new(
            &inv_id,
            izses_core::audit::ActionKind::ScanStarted,
            "Kaynak taraması başladı",
        ));
    }

    match inventory::scan_source(path, &ScanOptions::default()) {
        Ok(files) => {
            app.scanned = files.into_iter().map(ScannedItem::new).collect();
            app.scanned_source = Some(source_id.clone());

            let count = app.scanned.len();
            let now = chrono::Utc::now();
            {
                let store_arc = app.store_handle();
                let store = store_arc.lock().unwrap();
                let _ = store.update_source_scan(source_id, now, count as u32);
                let _ = store.insert_audit(&izses_core::audit::AuditEntry::new(
                    &inv_id,
                    izses_core::audit::ActionKind::ScanCompleted,
                    format!("Tarama tamamlandı: {count} dosya bulundu"),
                ));
            }
            app.notify(
                NotificationKind::Success,
                format!("Tarama tamamlandı: {count} dosya"),
            );
            app.refresh_session();
        }
        Err(e) => {
            app.notify(NotificationKind::Error, format!("Tarama başarısız: {e}"));
        }
    }
}

/// Kaynağı kaldırır.
fn remove_source(app: &mut IzsesApp, id: &izses_core::model::Id) {
    let inv_id = app.session.as_ref().map(|s| s.id().clone());
    let result = {
        let store_arc = app.store_handle();
        let store = store_arc.lock().unwrap();
        store.delete_source(id).and_then(|_| {
            if let Some(inv_id) = &inv_id {
                store.insert_audit(&izses_core::audit::AuditEntry::new(
                    inv_id,
                    izses_core::audit::ActionKind::EvidenceSourceRemoved,
                    "Delil kaynağı kaldırıldı",
                ))
            } else {
                Ok(())
            }
        })
    };
    match result {
        Ok(()) => {
            app.notify(NotificationKind::Info, "Kaynak kaldırıldı");
            app.refresh_session();
        }
        Err(e) => app.notify(NotificationKind::Error, format!("Kaldırılamadı: {e}")),
    }
}

/// Tarama sonuçları ve aktarım.
fn scan_section(app: &mut IzsesApp, ui: &mut egui::Ui) {
    if app.scanned.is_empty() {
        return;
    }

    card(ui, |ui| {
        ui.horizontal(|ui| {
            theme::subheading(ui, &format!("Tarama Sonucu ({} dosya)", app.scanned.len()));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Tüm Ses Dosyalarını Seç").clicked() {
                    for f in app.scanned.iter_mut() {
                        f.selected = f.category() == izses_core::inventory::FileCategory::Audio;
                    }
                }
                if ui.button("Seçimi Temizle").clicked() {
                    for f in app.scanned.iter_mut() {
                        f.selected = false;
                    }
                }
            });
        });
        ui.add_space(4.0);

        let mut total_selected = 0usize;
        let mut total_bytes = 0u64;
        for f in &app.scanned {
            if f.selected {
                total_selected += 1;
                total_bytes += f.size_bytes();
            }
        }

        egui::ScrollArea::vertical()
            .max_height(280.0)
            .show(ui, |ui| {
                for f in app.scanned.iter_mut() {
                    ui.horizontal(|ui| {
                        ui.checkbox(&mut f.selected, "");
                        widgets::category_badge(ui, f.category());
                        ui.label(RichText::new(f.display_name()).size(12.5));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            theme::muted(ui, &device::format_size(f.size_bytes()));
                        });
                    });
                }
            });

        ui.add_space(6.0);
        ui.horizontal(|ui| {
            theme::muted(
                ui,
                &format!(
                    "Seçili: {total_selected} dosya · {}",
                    device::format_size(total_bytes)
                ),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .add_enabled(
                        total_selected > 0,
                        egui::Button::new(format!(
                            "Seçili {total_selected} Dosyayı İncelemeye Aktar"
                        )),
                    )
                    .clicked()
                {
                    acquire_selected(app);
                }
            });
        });
    });
}

/// Seçili dosyaları incelemeye aktarır.
fn acquire_selected(app: &mut IzsesApp) {
    let Some(session) = &app.session else { return };
    let inv_id = session.id().clone();
    let storage_key = session.storage_key.clone();
    let Some(source_id) = app.scanned_source.clone() else {
        app.notify(NotificationKind::Error, "Kaynak seçilmedi");
        return;
    };

    let selected: Vec<ScannedItem> = app.scanned.iter().filter(|f| f.selected).cloned().collect();

    if selected.is_empty() {
        return;
    }

    let total = selected.len();
    let mut succeeded = 0usize;
    let mut failed = 0usize;
    let mut mismatch = 0usize;

    {
        let store_arc = app.store_handle();
        let store = store_arc.lock().unwrap();
        let _ = store.insert_audit(&izses_core::audit::AuditEntry::new(
            &inv_id,
            izses_core::audit::ActionKind::AcquisitionStarted,
            format!("{total} dosya için aktarım başladı"),
        ));
    }

    for file in &selected {
        let result = {
            let store_arc = app.store_handle();
            let store = store_arc.lock().unwrap();
            acquisition::acquire_file(
                &app.root,
                &store,
                &inv_id,
                &storage_key,
                &source_id,
                &file.source_path(),
                &file.relative_path(),
            )
        };
        match result {
            Ok(outcome) => {
                if outcome.verified {
                    succeeded += 1;
                } else {
                    mismatch += 1;
                }
            }
            Err(e) => {
                failed += 1;
                app.notify(
                    NotificationKind::Error,
                    format!("{}: {e}", file.display_name()),
                );
            }
        }
    }

    // Aktarım sonrası eksik kayıtları temizle.
    let _ = acquisition::clean_partial_files(&app.root, &storage_key);

    app.scanned.retain(|f| !f.selected);
    app.scanned_source = None;

    let msg =
        format!("Aktarım bitti: {succeeded} doğrulandı, {mismatch} uyuşmazlık, {failed} hata");
    let kind = if failed > 0 || mismatch > 0 {
        NotificationKind::Warning
    } else {
        NotificationKind::Success
    };
    app.notify(kind, msg);
    app.refresh_session();
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
