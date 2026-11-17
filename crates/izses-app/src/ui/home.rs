//! Ana ekran: inceleme yönetimi.
//!
//! Yeni inceleme oluşturma, devam eden ve tamamlanan incelemeleri listeleme
//! ve son açılanlara hızlı erişim.

use egui::RichText;

use izses_core::model::{Investigation, InvestigationStatus};

use crate::app::{IzsesApp, NotificationKind, Screen};
use crate::ui::{theme, widgets};

/// Ana ekranı çizer.
pub fn show(app: &mut IzsesApp, ctx: &egui::Context) {
    egui::TopBottomPanel::top("ust_bar").show(ctx, |ui| {
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.label(
                RichText::new("İZSES")
                    .size(20.0)
                    .strong()
                    .color(theme::ACCENT),
            );
            ui.label(
                RichText::new("Ses Kayıtlarının Metin Hâline Dönüştürülmesi (56.12)")
                    .size(13.0)
                    .color(theme::INFO),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Ayarlar").clicked() {
                    app.screen = Screen::Settings;
                }
                if ui.button("Yenile").clicked() {
                    app.reload_investigations();
                }
            });
        });
        ui.add_space(6.0);
    });

    egui::CentralPanel::default().show(ctx, |ui| {
        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.add_space(8.0);

            // Son açılanlar.
            if !app.settings.recent_investigations.is_empty() {
                theme::subheading(ui, "Son Açılan İncelemeler");
                let recents = app.settings.recent_investigations.clone();
                ui.horizontal_wrapped(|ui| {
                    for key in recents {
                        if ui.button(format!("📂 {key}")).clicked() {
                            app.open_investigation_by_key(&key);
                        }
                    }
                });
                ui.add_space(12.0);
            }

            ui.columns(2, |cols| {
                // Sol: yeni inceleme.
                new_investigation_form(app, &mut cols[0]);
                // Sağ: liste.
                investigation_list(app, &mut cols[1]);
            });
        });
    });
}

fn new_investigation_form(app: &mut IzsesApp, ui: &mut egui::Ui) {
    egui::Frame::new()
        .fill(egui::Color32::WHITE)
        .stroke(egui::Stroke::new(
            1.0_f32,
            egui::Color32::from_rgb(214, 218, 224),
        ))
        .inner_margin(egui::Margin::same(12))
        .corner_radius(6.0)
        .show(ui, |ui| {
            theme::subheading(ui, "Yeni İnceleme Oluştur");
            ui.add_space(6.0);

            let inv = &mut app.new_investigation;

            field(ui, "Görevlendiren mahkeme / merci", &mut inv.court);
            field(ui, "Dosya / soruşturma numarası", &mut inv.case_number);
            widgets::date_field(ui, "Görevlendirme tarihi", &mut inv.assignment_date);
            widgets::date_field(ui, "Son teslim tarihi", &mut inv.due_date);

            ui.horizontal(|ui| {
                ui.label("Verilen süre (gün):");
                let mut days = inv.allowed_days.unwrap_or(0);
                if ui
                    .add(egui::DragValue::new(&mut days).range(0..=3650))
                    .changed()
                {
                    inv.allowed_days = if days == 0 { None } else { Some(days) };
                }
            });

            ui.add_space(4.0);
            theme::muted(
                ui,
                "Taraf bilgileri (satır başına bir taraf, 'Rol: Ad' biçiminde)",
            );
            let mut parties_text = inv
                .parties
                .iter()
                .map(|p| format!("{}: {}", p.role, p.name))
                .collect::<Vec<_>>()
                .join("\n");
            if ui
                .add(
                    egui::TextEdit::multiline(&mut parties_text)
                        .desired_rows(2)
                        .desired_width(f32::INFINITY)
                        .hint_text("Şüpheli: Ad Soyad"),
                )
                .changed()
            {
                inv.parties = parse_parties(&parties_text);
            }

            ui.add_space(4.0);
            theme::muted(ui, "İnceleme konusu");
            ui.add(
                egui::TextEdit::multiline(&mut inv.subject)
                    .desired_rows(2)
                    .desired_width(f32::INFINITY),
            );

            ui.add_space(6.0);
            field(ui, "Bilirkişi adı ve soyadı", &mut inv.expert_name);
            field(ui, "Bilirkişi sicil numarası", &mut inv.expert_registry_no);
            field(ui, "Uzmanlık alanı", &mut inv.expertise_field);

            ui.add_space(4.0);
            theme::muted(
                ui,
                "Görevlendirme yazısındaki sorular (satır başına bir soru)",
            );
            ui.add(
                egui::TextEdit::multiline(&mut app.new_questions_text)
                    .desired_rows(3)
                    .desired_width(f32::INFINITY),
            );

            ui.add_space(4.0);
            theme::muted(ui, "İnceleme kapsamı notları");
            ui.add(
                egui::TextEdit::multiline(&mut inv.scope_notes)
                    .desired_rows(2)
                    .desired_width(f32::INFINITY),
            );

            ui.add_space(8.0);

            // Eksik alanlar.
            let missing = inv.missing_required_fields();
            if !missing.is_empty() {
                theme::callout(
                    ui,
                    &format!("Eksik bilgiler: {}", missing.join(", ")),
                    theme::WARNING,
                );
                ui.add_space(6.0);
            }

            ui.horizontal(|ui| {
                let can_create = !app.new_investigation.case_number.trim().is_empty();
                if ui
                    .add_enabled(can_create, egui::Button::new("İnceleme Oluştur"))
                    .clicked()
                {
                    create(app);
                }
                if ui.button("Formu Temizle").clicked() {
                    app.new_investigation = Investigation::new();
                    app.new_questions_text.clear();
                }
            });
        });
}

fn create(app: &mut IzsesApp) {
    app.new_investigation.questions = app
        .new_questions_text
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect();

    let investigation = app.new_investigation.clone();
    let result = {
        let store_arc = app.store_handle();
        let store = store_arc.lock().unwrap();
        crate::services::session::create_investigation(&store, &app.root, investigation)
    };

    match result {
        Ok(session) => {
            let key = session.storage_key.clone();
            app.settings.push_recent(&key);
            let _ = app.settings.save();
            app.session = Some(session);
            app.screen = Screen::Workspace;
            app.tab = crate::app::Tab::Overview;
            app.new_investigation = Investigation::new();
            app.new_questions_text.clear();
            app.reload_investigations();
            app.notify(NotificationKind::Success, "İnceleme oluşturuldu");
        }
        Err(e) => app.notify(NotificationKind::Error, format!("Oluşturulamadı: {e}")),
    }
}

fn investigation_list(app: &mut IzsesApp, ui: &mut egui::Ui) {
    egui::Frame::new()
        .fill(egui::Color32::WHITE)
        .stroke(egui::Stroke::new(
            1.0_f32,
            egui::Color32::from_rgb(214, 218, 224),
        ))
        .inner_margin(egui::Margin::same(12))
        .corner_radius(6.0)
        .show(ui, |ui| {
            theme::subheading(ui, "İncelemeler");
            ui.add_space(6.0);

            if app.investigations.is_empty() {
                widgets::empty_state(ui, "Henüz inceleme oluşturulmadı.");
                return;
            }

            let mut to_open: Option<izses_core::model::Id> = None;

            for inv in &app.investigations {
                let is_open = app
                    .session
                    .as_ref()
                    .map(|s| s.investigation.id == inv.id)
                    .unwrap_or(false);

                egui::Frame::new()
                    .fill(if is_open {
                        theme::ACCENT.gamma_multiply(0.08)
                    } else {
                        egui::Color32::TRANSPARENT
                    })
                    .inner_margin(egui::Margin::same(8))
                    .corner_radius(4.0)
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(inv.display_title()).size(13.5).strong());
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    let color = status_color(inv.status);
                                    theme::badge(ui, inv.status.label_tr(), color);
                                },
                            );
                        });
                        theme::muted(
                            ui,
                            &format!(
                                "{} · Bilirkişi: {} · Güncelleme: {}",
                                if inv.court.is_empty() {
                                    "—"
                                } else {
                                    &inv.court
                                },
                                if inv.expert_name.is_empty() {
                                    "—"
                                } else {
                                    &inv.expert_name
                                },
                                izses_core::timefmt::format_datetime(&inv.updated_at)
                            ),
                        );
                        ui.horizontal(|ui| {
                            if ui.button("Aç").clicked() {
                                to_open = Some(inv.id.clone());
                            }
                        });
                    });
                ui.add_space(4.0);
            }

            if let Some(id) = to_open {
                app.open_investigation(&id);
            }
        });
}

fn status_color(status: InvestigationStatus) -> egui::Color32 {
    match status {
        InvestigationStatus::Draft => theme::INFO,
        InvestigationStatus::InProgress => theme::ACCENT,
        InvestigationStatus::Completed => theme::SUCCESS,
        InvestigationStatus::Archived => egui::Color32::GRAY,
    }
}

fn field(ui: &mut egui::Ui, label: &str, value: &mut String) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(format!("{label}:")).size(13.0));
        ui.add(egui::TextEdit::singleline(value).desired_width(f32::INFINITY));
    });
}

fn parse_parties(text: &str) -> Vec<izses_core::model::Party> {
    text.lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() {
                return None;
            }
            match line.split_once(':') {
                Some((role, name)) => Some(izses_core::model::Party {
                    role: role.trim().to_string(),
                    name: name.trim().to_string(),
                }),
                None => Some(izses_core::model::Party {
                    role: "Taraf".to_string(),
                    name: line.to_string(),
                }),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_parties_with_and_without_role() {
        let parties = parse_parties("Şüpheli: Ahmet\nMağdur: Ayşe\nSadece İsim");
        assert_eq!(parties.len(), 3);
        assert_eq!(parties[0].role, "Şüpheli");
        assert_eq!(parties[0].name, "Ahmet");
        assert_eq!(parties[2].role, "Taraf");
        assert_eq!(parties[2].name, "Sadece İsim");
    }

    #[test]
    fn empty_lines_ignored() {
        assert!(parse_parties("\n  \n").is_empty());
    }
}
