//! Bildirim penceresi ve ortak küçük bileşenler.

use egui::{Context, RichText};

use crate::app::{IzsesApp, NotificationKind};
use crate::ui::theme;

/// Sağ üstte bildirimleri gösterir.
pub fn show(app: &mut IzsesApp, ctx: &Context) {
    if app.notifications.is_empty() {
        return;
    }

    egui::Area::new(egui::Id::new("bildirimler"))
        .anchor(egui::Align2::RIGHT_TOP, egui::vec2(-16.0, 16.0))
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            egui::Frame::new()
                .fill(egui::Color32::from_rgb(255, 255, 255))
                .stroke(egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(210, 214, 220)))
                .inner_margin(egui::Margin::same(8))
                .corner_radius(6.0)
                .show(ui, |ui| {
                    ui.set_max_width(420.0);
                    let mut to_remove = None;
                    for (i, n) in app.notifications.iter().enumerate() {
                        let color = match n.kind {
                            NotificationKind::Info => theme::INFO,
                            NotificationKind::Success => theme::SUCCESS,
                            NotificationKind::Warning => theme::WARNING,
                            NotificationKind::Error => theme::ERROR,
                        };
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("●").color(color).size(14.0));
                            ui.label(RichText::new(&n.text).size(12.5));
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if ui.small_button("✕").clicked() {
                                        to_remove = Some(i);
                                    }
                                },
                            );
                        });
                    }
                    if let Some(i) = to_remove {
                        app.notifications.remove(i);
                    }
                });
        });
}
