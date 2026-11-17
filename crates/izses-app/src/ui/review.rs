//! Metin Kontrolü sekmesi.
//!
//! Bilirkişinin ana çalışma alanı: ses kaydını dinler, otomatik deşifreyi
//! karşılaştırır, düzeltmeleri yapar, konuşmacı etiketler ve gözlem notları
//! ekler.
//!
//! Otomatik üretilen metin ile bilirkişinin düzelttiği metin her zaman ayrı
//! tutulur ve ikisi de görünür.

use egui::RichText;

use izses_core::inventory::EvidenceFile;
use izses_core::transcript::{SegmentFlags, SegmentRevision, TranscriptSegment};

use crate::app::{IzsesApp, NotificationKind};
use crate::ui::{theme, widgets};

/// Metin kontrolü sekmesini çizer.
pub fn show(app: &mut IzsesApp, ui: &mut egui::Ui) {
    ui.add_space(8.0);
    theme::heading(ui, "Metin Kontrolü");
    theme::muted(
        ui,
        "Otomatik deşifreyi kaydı dinleyerek kontrol edin. Otomatik metin \
         değiştirilmez; düzeltmeleriniz ayrı saklanır.",
    );
    ui.add_space(8.0);

    let files: Vec<EvidenceFile> = app
        .session
        .as_ref()
        .map(|s| s.audio_files().into_iter().cloned().collect())
        .unwrap_or_default();

    if files.is_empty() {
        card(ui, |ui| {
            widgets::empty_state(ui, "Kontrol edilecek ses dosyası yok.");
        });
        return;
    }

    // Dosya seçimi.
    let selected_index = app
        .review_file
        .as_ref()
        .and_then(|id| files.iter().position(|f| &f.id == id));

    card(ui, |ui| {
        ui.horizontal(|ui| {
            theme::subheading(ui, "Kayıt");
            egui::ComboBox::from_id_salt("kontrol_dosya")
                .selected_text(
                    selected_index
                        .and_then(|i| files.get(i))
                        .map(|f| f.file_name())
                        .unwrap_or_else(|| "Kayıt seçin".to_string()),
                )
                .width(340.0)
                .show_ui(ui, |ui| {
                    for (i, f) in files.iter().enumerate() {
                        if ui
                            .selectable_label(selected_index == Some(i), f.file_name())
                            .clicked()
                        {
                            app.review_file = Some(f.id.clone());
                            app.review_selected = None;
                        }
                    }
                });

            if let Some(i) = selected_index {
                if let Some(file) = files.get(i) {
                    let t = app
                        .session
                        .as_ref()
                        .and_then(|s| s.transcript_for_file(&file.id));
                    if let Some(t) = t {
                        theme::badge(ui, t.status.label_tr(), theme::ACCENT);
                    } else {
                        theme::badge(ui, "Deşifre yok", theme::WARNING);
                    }
                }
            }
        });
    });

    ui.add_space(8.0);

    let Some(index) = selected_index else {
        widgets::empty_state(ui, "Kontrol etmek için bir kayıt seçin.");
        return;
    };
    let file = files[index].clone();

    player_panel(app, ui, &file);
    ui.add_space(8.0);
    segments_panel(app, ui, &file);
}

/// Oynatma paneli ve yardımcı kontroller.
fn player_panel(app: &mut IzsesApp, ui: &mut egui::Ui, file: &EvidenceFile) {
    let path = app.session.as_ref().map(|s| s.local_path(&app.root, file));

    card(ui, |ui| {
        theme::subheading(ui, "Oynatma");
        ui.add_space(4.0);

        let segments = app
            .session
            .as_ref()
            .map(|s| {
                s.segments_for_file(&file.id)
                    .into_iter()
                    .cloned()
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        let duration_ms = file
            .audio
            .as_ref()
            .and_then(|a| a.duration_ms)
            .or_else(|| {
                path.as_ref()
                    .and_then(|p| izses_audio::probe(p).duration_ms)
            })
            .unwrap_or_else(|| segments.iter().map(|s| s.end_ms).max().unwrap_or(0));

        // Dalga formu (varsa).
        if let Some(path) = &path {
            if let Ok(audio) = izses_audio::decode_to_mono_16k(path) {
                draw_waveform_with_playhead(
                    ui,
                    &audio.samples,
                    app.review_playhead_ms,
                    duration_ms,
                    &segments,
                );
            }
        }

        ui.add_space(6.0);

        ui.horizontal(|ui| {
            if ui.button("⏮ Başa").clicked() {
                app.review_playhead_ms = 0;
            }
            if ui.button("⏪ -5 sn").clicked() {
                app.review_playhead_ms = app.review_playhead_ms.saturating_sub(5000);
            }
            let play_label = if app.review_playing {
                "⏸ Duraklat"
            } else {
                "▶ Oynat"
            };
            if ui.button(play_label).clicked() {
                app.review_playing = !app.review_playing;
                if app.review_playing {
                    #[cfg(windows)]
                    {
                        start_playback(app, file);
                    }
                    #[cfg(not(windows))]
                    {
                        app.notify(
                            NotificationKind::Info,
                            "Oynatma bu geliştirme ortamında kullanılamaz; \
                             Windows sürümünde etkindir.",
                        );
                    }
                }
            }
            if ui.button("⏩ +5 sn").clicked() {
                app.review_playhead_ms = (app.review_playhead_ms + 5000).min(duration_ms);
            }
            if ui.button("⏭ Sona").clicked() {
                app.review_playhead_ms = duration_ms;
            }
        });

        ui.horizontal(|ui| {
            ui.label("Konum:");
            ui.add(
                egui::Slider::new(&mut app.review_playhead_ms, 0..=duration_ms.max(1))
                    .custom_formatter(|v, _| izses_core::timefmt::format_ms(v as u64))
                    .show_value(true),
            );
            ui.label(
                RichText::new(format!("/ {}", izses_core::timefmt::format_ms(duration_ms)))
                    .size(12.0)
                    .color(theme::INFO),
            );
        });

        ui.horizontal(|ui| {
            ui.label("Oynatma hızı:");
            egui::ComboBox::from_id_salt("hiz")
                .selected_text(format!("{:.2}x", app.review_speed))
                .show_ui(ui, |ui| {
                    for speed in [0.5f32, 0.75, 1.0, 1.25, 1.5, 2.0] {
                        if ui
                            .selectable_label(
                                (app.review_speed - speed).abs() < 0.01,
                                format!("{speed:.2}x"),
                            )
                            .clicked()
                        {
                            app.review_speed = speed;
                        }
                    }
                });
        });

        if !path.map(|p| p.is_file()).unwrap_or(false) {
            theme::callout(
                ui,
                "Ses dosyasının yerel kopyası bulunamadı. İnceleme klasörünü denetleyin.",
                theme::WARNING,
            );
        }
    });
}

/// Metin bölümleri paneli.
fn segments_panel(app: &mut IzsesApp, ui: &mut egui::Ui, file: &EvidenceFile) {
    let transcript = app
        .session
        .as_ref()
        .and_then(|s| s.transcript_for_file(&file.id).cloned());

    let Some(transcript) = transcript else {
        card(ui, |ui| {
            widgets::empty_state(
                ui,
                "Bu kayıt için otomatik deşifre bulunmuyor. Otomatik Deşifre \
                 sekmesinden deşifreyi başlatın.",
            );
        });
        return;
    };

    let segments: Vec<TranscriptSegment> = app
        .session
        .as_ref()
        .and_then(|s| s.segments.get(&transcript.id).cloned())
        .unwrap_or_default();

    if segments.is_empty() {
        card(ui, |ui| {
            widgets::empty_state(ui, "Deşifre henüz metin üretmedi.");
        });
        return;
    }

    card(ui, |ui| {
        ui.horizontal(|ui| {
            theme::subheading(ui, &format!("Metin Bölümleri ({})", segments.len()));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                theme::muted(ui, "Kısayol: ↑/↓ bölüm geçişi, Enter oynat");
            });
        });
        ui.add_space(4.0);

        // Klavye ile bölüm geçişi.
        let mut move_selection: Option<isize> = None;
        ui.input(|i| {
            if i.key_pressed(egui::Key::ArrowDown) {
                move_selection = Some(1);
            }
            if i.key_pressed(egui::Key::ArrowUp) {
                move_selection = Some(-1);
            }
        });
        if let Some(delta) = move_selection {
            let current = app.review_selected.unwrap_or(0) as isize;
            let next = (current + delta).clamp(0, segments.len() as isize - 1) as usize;
            app.review_selected = Some(next);
            app.review_playhead_ms = segments[next].start_ms;
        }

        let mut changed_segment: Option<TranscriptSegment> = None;
        let mut revert_segment: Option<izses_core::model::Id> = None;
        let mut delete_segment: Option<izses_core::model::Id> = None;
        let mut split_segment: Option<izses_core::model::Id> = None;
        let mut merge_with_next: Option<izses_core::model::Id> = None;

        for (idx, seg) in segments.iter().enumerate() {
            let selected = app.review_selected == Some(idx);
            let frame = egui::Frame::new()
                .fill(if selected {
                    theme::ACCENT.gamma_multiply(0.10)
                } else if seg.has_attention_flag() {
                    theme::HIGHLIGHT_BG
                } else {
                    egui::Color32::TRANSPARENT
                })
                .inner_margin(egui::Margin::same(8))
                .corner_radius(4.0);

            frame.show(ui, |ui| {
                // Başlık satırı.
                ui.horizontal(|ui| {
                    if ui
                        .selectable_label(
                            selected,
                            RichText::new(seg.time_label()).size(12.0).strong(),
                        )
                        .clicked()
                    {
                        app.review_selected = Some(idx);
                        app.review_playhead_ms = seg.start_ms;
                    }

                    if seg.is_edited() {
                        theme::badge(ui, "Düzeltildi", theme::WARNING);
                    }
                    if seg.flags.unclear {
                        theme::badge(ui, "Anlaşılamadı", theme::ERROR);
                    }
                    if seg.flags.relisten {
                        theme::badge(ui, "Yeniden dinle", theme::ACCENT);
                    }
                    if seg.flags.noise {
                        theme::badge(ui, "Gürültü", theme::INFO);
                    }
                    if seg.flags.overlap {
                        theme::badge(ui, "Üst üste", theme::INFO);
                    }

                    if let Some(speaker) = &seg.speaker {
                        theme::badge(ui, speaker, theme::SUCCESS);
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button("Oynat").clicked() {
                            app.review_playhead_ms = seg.start_ms;
                            app.review_playing = true;
                        }
                    });
                });

                ui.add_space(2.0);

                // Otomatik metin (değiştirilemez).
                ui.horizontal_wrapped(|ui| {
                    theme::muted(ui, "Otomatik:");
                    ui.label(
                        RichText::new(if seg.auto_text.is_empty() {
                            "—"
                        } else {
                            &seg.auto_text
                        })
                        .size(12.5),
                    );
                });

                // Bilirkişi düzeltmesi.
                let mut edited = seg.edited_text.clone().unwrap_or_default();
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Düzeltme:").size(12.0).color(theme::INFO));
                    let response = ui.add(
                        egui::TextEdit::singleline(&mut edited)
                            .desired_width(f32::INFINITY)
                            .hint_text("Gerekirse düzeltin…"),
                    );
                    if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        let mut updated = seg.clone();
                        updated.edited_text = if edited.trim().is_empty() {
                            None
                        } else {
                            Some(edited.clone())
                        };
                        changed_segment = Some(updated);
                    }
                });

                // Konuşmacı ve gözlem.
                ui.horizontal(|ui| {
                    let mut speaker = seg.speaker.clone().unwrap_or_default();
                    ui.label(RichText::new("Konuşmacı:").size(12.0));
                    if ui
                        .add(
                            egui::TextEdit::singleline(&mut speaker)
                                .desired_width(120.0)
                                .hint_text("ör. Konuşmacı 1"),
                        )
                        .lost_focus()
                    {
                        let mut updated = seg.clone();
                        updated.speaker = if speaker.trim().is_empty() {
                            None
                        } else {
                            Some(speaker)
                        };
                        changed_segment = Some(updated);
                    }

                    let mut observation = seg.observation.clone().unwrap_or_default();
                    ui.label(RichText::new("Gözlem:").size(12.0));
                    if ui
                        .add(
                            egui::TextEdit::singleline(&mut observation)
                                .desired_width(240.0)
                                .hint_text("ör. arka plan gürültüsü"),
                        )
                        .lost_focus()
                    {
                        let mut updated = seg.clone();
                        updated.observation = if observation.trim().is_empty() {
                            None
                        } else {
                            Some(observation)
                        };
                        changed_segment = Some(updated);
                    }
                });

                // İşaretler.
                ui.horizontal(|ui| {
                    let mut flags = seg.flags.clone();
                    let before = flags.clone();
                    ui.checkbox(&mut flags.unclear, "Anlaşılamadı");
                    ui.checkbox(&mut flags.relisten, "Yeniden dinle");
                    ui.checkbox(&mut flags.noise, "Gürültü");
                    ui.checkbox(&mut flags.overlap, "Üst üste konuşma");
                    if flags_changed(&before, &flags) {
                        let mut updated = seg.clone();
                        updated.flags = flags;
                        changed_segment = Some(updated);
                    }
                });

                // Bölüm işlemleri.
                ui.horizontal(|ui| {
                    if ui.small_button("Kaydet").clicked() {
                        let mut updated = seg.clone();
                        updated.edited_text = if edited.trim().is_empty() {
                            None
                        } else {
                            Some(edited.clone())
                        };
                        changed_segment = Some(updated);
                    }
                    if seg.is_edited() && ui.small_button("Düzeltmeyi Geri Al").clicked() {
                        revert_segment = Some(seg.id.clone());
                    }
                    if ui.small_button("Böl").clicked() {
                        split_segment = Some(seg.id.clone());
                    }
                    if idx + 1 < segments.len() && ui.small_button("Sonrakiyle Birleştir").clicked()
                    {
                        merge_with_next = Some(seg.id.clone());
                    }
                    if ui.small_button("Sil").clicked() {
                        delete_segment = Some(seg.id.clone());
                    }
                });
            });
            ui.add_space(4.0);
        }

        // Değişiklikleri uygula.
        if let Some(updated) = changed_segment {
            apply_segment_change(app, &updated);
        }
        if let Some(id) = revert_segment {
            apply_revert(app, &id);
        }
        if let Some(id) = split_segment {
            apply_split(app, &transcript.id, &id);
        }
        if let Some(id) = merge_with_next {
            apply_merge(app, &transcript.id, &id);
        }
        if let Some(id) = delete_segment {
            apply_delete(app, &id);
        }
    });
}

/// Segment değişikliğini kaydeder ve revizyon geçmişine ekler.
fn apply_segment_change(app: &mut IzsesApp, updated: &TranscriptSegment) {
    let Some(session) = &app.session else { return };
    let inv_id = session.id().clone();
    let author = session.investigation.expert_name.clone();

    let result = {
        let store_arc = app.store_handle();
        let store = store_arc.lock().unwrap();
        let before = store
            .get_segment(&updated.id)
            .map(|s| s.effective_text().to_string())
            .unwrap_or_default();

        store.update_segment(&mut updated.clone()).and_then(|_| {
            let after = updated.effective_text().to_string();
            if before != after {
                let mut rev = SegmentRevision::new(
                    &updated.id,
                    &updated.transcript_id,
                    before,
                    after,
                    if author.is_empty() {
                        "Bilirkişi"
                    } else {
                        &author
                    },
                );
                rev.note = Some("düzeltme".to_string());
                store.insert_revision(&rev)?;
                store.insert_audit(&izses_core::audit::AuditEntry::new(
                    &inv_id,
                    izses_core::audit::ActionKind::SegmentEdited,
                    "Metin bölümü düzeltildi",
                ))?;
            }
            Ok(())
        })
    };

    match result {
        Ok(()) => {
            app.refresh_session();
            app.notify(NotificationKind::Success, "Değişiklik kaydedildi");
        }
        Err(e) => app.notify(NotificationKind::Error, format!("Kaydedilemedi: {e}")),
    }
}

fn apply_revert(app: &mut IzsesApp, id: &izses_core::model::Id) {
    let result = {
        let store_arc = app.store_handle();
        let store = store_arc.lock().unwrap();
        store.revert_segment(id)
    };
    match result {
        Ok(()) => {
            app.refresh_session();
            app.notify(NotificationKind::Info, "Düzeltme geri alındı");
        }
        Err(e) => app.notify(NotificationKind::Error, format!("Geri alınamadı: {e}")),
    }
}

/// Bölümü ikiye ayırır.
fn apply_split(
    app: &mut IzsesApp,
    transcript_id: &izses_core::model::Id,
    id: &izses_core::model::Id,
) {
    let result = {
        let store_arc = app.store_handle();
        let store = store_arc.lock().unwrap();
        (|| -> izses_core::error::CoreResult<()> {
            let seg = store.get_segment(id)?;
            let mid = seg.start_ms + seg.duration_ms() / 2;
            if mid <= seg.start_ms || mid >= seg.end_ms {
                return Err(izses_core::error::CoreError::invalid(
                    "bölüm çok kısa, ayrılamaz",
                ));
            }
            // İlk yarı.
            let mut first = seg.clone();
            first.end_ms = mid;
            store.update_segment(&mut first)?;

            // İkinci yarı (yeni bölüm).
            let mut second =
                TranscriptSegment::new(transcript_id, 0, mid, seg.end_ms, String::new());
            second.edited_text = if seg.auto_text.is_empty() {
                None
            } else {
                Some(String::new())
            };
            store.insert_segment(&second)?;
            store.reindex_segments(transcript_id)?;
            store.insert_audit(&izses_core::audit::AuditEntry::new(
                &seg.transcript_id,
                izses_core::audit::ActionKind::SegmentSplit,
                "Metin bölümü ayrıldı",
            ))?;
            Ok(())
        })()
    };
    match result {
        Ok(()) => {
            app.refresh_session();
            app.notify(NotificationKind::Info, "Bölüm ayrıldı");
        }
        Err(e) => app.notify(NotificationKind::Error, format!("Ayrılamadı: {e}")),
    }
}

/// Bir bölümü sonrakiyle birleştirir.
fn apply_merge(
    app: &mut IzsesApp,
    transcript_id: &izses_core::model::Id,
    id: &izses_core::model::Id,
) {
    let result = {
        let store_arc = app.store_handle();
        let store = store_arc.lock().unwrap();
        (|| -> izses_core::error::CoreResult<()> {
            let segs = store.list_segments(transcript_id)?;
            let Some(pos) = segs.iter().position(|s| &s.id == id) else {
                return Err(izses_core::error::CoreError::not_found("metin bölümü"));
            };
            if pos + 1 >= segs.len() {
                return Err(izses_core::error::CoreError::invalid(
                    "birleştirilecek sonraki bölüm yok",
                ));
            }
            let current = segs[pos].clone();
            let next = segs[pos + 1].clone();

            let mut merged = current.clone();
            merged.end_ms = next.end_ms;
            merged.auto_text = format!("{} {}", current.auto_text, next.auto_text)
                .trim()
                .to_string();
            merged.edited_text = match (current.edited_text.as_ref(), next.edited_text.as_ref()) {
                (None, None) => None,
                (a, b) => Some(
                    format!(
                        "{} {}",
                        a.map(|s| s.as_str()).unwrap_or(&current.auto_text),
                        b.map(|s| s.as_str()).unwrap_or(&next.auto_text)
                    )
                    .trim()
                    .to_string(),
                ),
            };
            store.update_segment(&mut merged)?;
            store.delete_segment(&next.id)?;
            store.reindex_segments(transcript_id)?;
            store.insert_audit(&izses_core::audit::AuditEntry::new(
                &current.transcript_id,
                izses_core::audit::ActionKind::SegmentMerged,
                "Metin bölümleri birleştirildi",
            ))?;
            Ok(())
        })()
    };
    match result {
        Ok(()) => {
            app.refresh_session();
            app.notify(NotificationKind::Info, "Bölümler birleştirildi");
        }
        Err(e) => app.notify(NotificationKind::Error, format!("Birleştirilemedi: {e}")),
    }
}

fn apply_delete(app: &mut IzsesApp, id: &izses_core::model::Id) {
    let result = {
        let store_arc = app.store_handle();
        let store = store_arc.lock().unwrap();
        let seg = store.get_segment(id);
        match seg {
            Ok(seg) => store.delete_segment(id).and_then(|_| {
                store.reindex_segments(&seg.transcript_id)?;
                store.insert_audit(&izses_core::audit::AuditEntry::new(
                    &seg.transcript_id,
                    izses_core::audit::ActionKind::SegmentDeleted,
                    "Metin bölümü silindi",
                ))
            }),
            Err(e) => Err(e),
        }
    };
    match result {
        Ok(()) => {
            app.refresh_session();
            app.review_selected = None;
            app.notify(NotificationKind::Info, "Bölüm silindi");
        }
        Err(e) => app.notify(NotificationKind::Error, format!("Silinemedi: {e}")),
    }
}

fn flags_changed(a: &SegmentFlags, b: &SegmentFlags) -> bool {
    a.unclear != b.unclear
        || a.relisten != b.relisten
        || a.noise != b.noise
        || a.overlap != b.overlap
}

/// Dalga formunu oynatma başlığı ve bölüm işaretleriyle çizer.
fn draw_waveform_with_playhead(
    ui: &mut egui::Ui,
    samples: &[f32],
    playhead_ms: u64,
    duration_ms: u64,
    segments: &[TranscriptSegment],
) {
    let bins = izses_audio::dsp::waveform(samples, 700);
    let height = 90.0;
    let width = ui.available_width().min(1000.0);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 3.0, egui::Color32::from_rgb(250, 251, 252));

    let mid = rect.center().y;
    let half = rect.height() / 2.0 - 4.0;
    let bar_width = rect.width() / bins.len().max(1) as f32;

    for (i, bin) in bins.iter().enumerate() {
        let x = rect.left() + i as f32 * bar_width;
        let h = bin.peak * half;
        painter.line_segment(
            [egui::pos2(x, mid - h), egui::pos2(x, mid + h)],
            egui::Stroke::new(bar_width.max(0.5), theme::ACCENT.gamma_multiply(0.75)),
        );
    }

    // Bölüm sınırları.
    if duration_ms > 0 {
        for seg in segments {
            let x = rect.left() + (seg.start_ms as f32 / duration_ms as f32) * rect.width();
            let color = if seg.has_attention_flag() {
                theme::WARNING.gamma_multiply(0.7)
            } else {
                egui::Color32::from_gray(160)
            };
            painter.line_segment(
                [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
                egui::Stroke::new(1.0_f32, color),
            );
        }

        // Oynatma başlığı.
        let x = rect.left() + (playhead_ms as f32 / duration_ms as f32) * rect.width();
        painter.line_segment(
            [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
            egui::Stroke::new(2.0_f32, theme::ERROR),
        );
    }
}

/// Windows'ta ses oynatmayı başlatır.
#[cfg(windows)]
fn start_playback(app: &mut IzsesApp, file: &EvidenceFile) {
    use rodio::{Decoder, OutputStream, Sink};
    let Some(path) = app.session.as_ref().map(|s| s.local_path(&app.root, file)) else {
        return;
    };
    let result = (|| -> Result<(), String> {
        let (_stream, handle) =
            OutputStream::try_default().map_err(|e| format!("ses aygıtı açılamadı: {e}"))?;
        let sink = Sink::try_new(&handle).map_err(|e| format!("oynatıcı kurulamadı: {e}"))?;
        let source = std::fs::File::open(&path).map_err(|e| format!("dosya açılamadı: {e}"))?;
        let decoder = Decoder::new(std::io::BufReader::new(source))
            .map_err(|e| format!("ses çözülemedi: {e}"))?;
        sink.append(decoder);
        sink.detach();
        Ok(())
    })();
    if let Err(e) = result {
        app.notify(NotificationKind::Error, e);
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flag_change_detection() {
        let a = SegmentFlags::default();
        let mut b = SegmentFlags::default();
        assert!(!flags_changed(&a, &b));
        b.unclear = true;
        assert!(flags_changed(&a, &b));
    }
}
