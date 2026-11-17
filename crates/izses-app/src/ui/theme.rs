//! Arayüz teması ve ortak görsel ayarlar.
//!
//! Sade, profesyonel ve okunaklı bir görünüm hedeflenir. Renkler durum
//! bildirimleri için anlam taşır (yeşil: doğrulandı, kırmızı: hata,
//! sarı: dikkat).

use egui::{Color32, Context, Visuals};

/// Vurgu rengi.
pub const ACCENT: Color32 = Color32::from_rgb(38, 92, 148);
/// Başarı (doğrulandı, tamamlandı).
pub const SUCCESS: Color32 = Color32::from_rgb(28, 122, 66);
/// Uyarı (dikkat gerekli).
pub const WARNING: Color32 = Color32::from_rgb(176, 118, 0);
/// Hata.
pub const ERROR: Color32 = Color32::from_rgb(168, 42, 42);
/// Nötr bilgi.
pub const INFO: Color32 = Color32::from_rgb(70, 80, 92);
/// Vurgulanmış satır arka planı.
pub const HIGHLIGHT_BG: Color32 = Color32::from_rgb(255, 247, 229);

/// Temayı uygular.
pub fn apply(ctx: &Context) {
    let mut visuals = Visuals::light();
    visuals.panel_fill = Color32::from_rgb(247, 248, 250);
    visuals.window_fill = Color32::from_rgb(252, 252, 253);
    visuals.selection.bg_fill = ACCENT.gamma_multiply(0.35);
    visuals.hyperlink_color = ACCENT;
    ctx.set_visuals(visuals);

    let mut style = (*ctx.style()).clone();
    style.spacing.item_spacing = egui::vec2(8.0, 6.0);
    style.spacing.button_padding = egui::vec2(10.0, 5.0);
    ctx.set_style(style);
}

/// Başlık metni çizer.
pub fn heading(ui: &mut egui::Ui, text: &str) {
    ui.add_space(2.0);
    ui.label(egui::RichText::new(text).size(18.0).strong());
    ui.add_space(4.0);
}

/// Alt başlık çizer.
pub fn subheading(ui: &mut egui::Ui, text: &str) {
    ui.label(egui::RichText::new(text).size(14.0).strong());
}

/// Açıklama metni çizer.
pub fn muted(ui: &mut egui::Ui, text: &str) {
    ui.label(egui::RichText::new(text).size(12.0).color(INFO));
}

/// Durum etiketi (renkli rozet).
pub fn badge(ui: &mut egui::Ui, text: &str, color: Color32) {
    ui.label(
        egui::RichText::new(text)
            .size(12.0)
            .color(Color32::WHITE)
            .background_color(color),
    );
}

/// Anahtar-değer satırı.
pub fn key_value(ui: &mut egui::Ui, key: &str, value: &str) {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(format!("{key}:")).strong().size(13.0));
        ui.label(egui::RichText::new(value).size(13.0));
    });
}

/// Uyarı kutusu.
pub fn callout(ui: &mut egui::Ui, text: &str, color: Color32) {
    egui::Frame::new()
        .fill(color.gamma_multiply(0.10))
        .stroke(egui::Stroke::new(1.0_f32, color))
        .inner_margin(egui::Margin::same(8))
        .corner_radius(4.0)
        .show(ui, |ui| {
            ui.label(egui::RichText::new(text).size(12.5).color(color));
        });
}

/// İlerleme çubuğu (etiketli).
pub fn progress(ui: &mut egui::Ui, fraction: f32, label: &str) {
    ui.add(
        egui::ProgressBar::new(fraction.clamp(0.0, 1.0))
            .text(label)
            .desired_width(ui.available_width().min(520.0)),
    );
}
