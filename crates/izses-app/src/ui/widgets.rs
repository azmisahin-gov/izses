//! Ortak arayüz bileşenleri.

use egui::RichText;

use izses_core::inventory::{FileCategory, VerificationState};

use crate::ui::theme;

/// Doğrulama durumu için renk.
pub fn verification_color(state: VerificationState) -> egui::Color32 {
    match state {
        VerificationState::Verified => theme::SUCCESS,
        VerificationState::Mismatch | VerificationState::Failed => theme::ERROR,
        VerificationState::InProgress => theme::WARNING,
        VerificationState::NotAcquired => theme::INFO,
    }
}

/// Doğrulama durumu rozeti.
pub fn verification_badge(ui: &mut egui::Ui, state: VerificationState) {
    theme::badge(ui, state.label_tr(), verification_color(state));
}

/// Dosya sınıfı rozeti.
pub fn category_badge(ui: &mut egui::Ui, category: FileCategory) {
    let color = match category {
        FileCategory::Audio => theme::ACCENT,
        FileCategory::Document => theme::INFO,
        FileCategory::Other => egui::Color32::GRAY,
    };
    theme::badge(ui, category.label_tr(), color);
}

/// Boş liste durumu için açıklama.
pub fn empty_state(ui: &mut egui::Ui, text: &str) {
    ui.add_space(12.0);
    ui.vertical_centered(|ui| {
        ui.label(
            RichText::new(text)
                .size(13.0)
                .color(theme::INFO)
                .italics(),
        );
    });
    ui.add_space(12.0);
}

/// Ayrılabilir çizgi.
pub fn separator(ui: &mut egui::Ui) {
    ui.add_space(4.0);
    ui.separator();
    ui.add_space(4.0);
}

/// Onay gerektiren işlemler için iki aşamalı düğme.
pub struct ConfirmButton {
    armed: bool,
    armed_until: Option<std::time::Instant>,
}

impl Default for ConfirmButton {
    fn default() -> Self {
        Self {
            armed: false,
            armed_until: None,
        }
    }
}

impl ConfirmButton {
    /// Düğmeyi çizer. İlk tıklama silahı kurar, ikinci tıklama onaylar.
    pub fn ui(
        &mut self,
        ui: &mut egui::Ui,
        label: &str,
        confirm_label: &str,
        enabled: bool,
    ) -> bool {
        // Silahın süresi dolduysa geri al.
        if let Some(until) = self.armed_until {
            if std::time::Instant::now() > until {
                self.armed = false;
                self.armed_until = None;
            }
        }

        let text = if self.armed { confirm_label } else { label };
        let button = egui::Button::new(RichText::new(text).size(13.0));
        let button = if self.armed {
            button.fill(theme::ERROR.gamma_multiply(0.20))
        } else {
            button
        };

        let clicked = ui.add_enabled(enabled, button).clicked();
        if !clicked {
            return false;
        }
        if self.armed {
            self.armed = false;
            self.armed_until = None;
            true
        } else {
            self.armed = true;
            self.armed_until = Some(std::time::Instant::now() + std::time::Duration::from_secs(5));
            false
        }
    }

    pub fn reset(&mut self) {
        self.armed = false;
        self.armed_until = None;
    }
}

/// Tarih seçici yerine basit metin alanı (gg/aa/yyyy).
pub fn date_field(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut Option<chrono::NaiveDate>,
) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label(RichText::new(format!("{label}:")).size(13.0));
        let mut text = value
            .map(|d| d.format("%d.%m.%Y").to_string())
            .unwrap_or_default();
        let response = ui.add(
            egui::TextEdit::singleline(&mut text)
                .hint_text("gg.aa.yyyy")
                .desired_width(120.0),
        );
        if response.changed() {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                *value = None;
                changed = true;
            } else if let Ok(d) = chrono::NaiveDate::parse_from_str(trimmed, "%d.%m.%Y") {
                *value = Some(d);
                changed = true;
            }
        }
    });
    changed
}
