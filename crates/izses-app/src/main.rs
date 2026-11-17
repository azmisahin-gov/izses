//! IZSES uygulama girişi.
//!
//! Uygulama, yerel bir masaüstü penceresi açar. Tüm veri yerel bilgisayarda
//! kalır; hiçbir işlem dış sunucuya gönderilmez.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use izses_app::IzsesApp;

fn main() -> eframe::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_target(false)
        .init();

    let app = match IzsesApp::new() {
        Ok(app) => app,
        Err(e) => {
            eprintln!("İZSES başlatılamadı: {e}");
            rfd::MessageDialog::new()
                .set_level(rfd::MessageLevel::Error)
                .set_title("İZSES başlatılamadı")
                .set_description(format!(
                    "Uygulama başlatılırken bir sorun oluştu:\n\n{e}\n\n\
                     Veri klasörünüzün erişilebilir olduğundan emin olun."
                ))
                .show();
            std::process::exit(1);
        }
    };

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 860.0])
            .with_min_inner_size([1000.0, 700.0])
            .with_title("İZSES – 56.12 Ses Kayıtlarının Metin Hâline Dönüştürülmesi"),
        ..Default::default()
    };

    eframe::run_native(
        "izses",
        options,
        Box::new(|cc| {
            load_fonts(&cc.egui_ctx);
            Ok(Box::new(app))
        }),
    )
}

/// Türkçe karakter desteği için yazı tipi yükler.
fn load_fonts(ctx: &egui::Context) {
    let candidates = [
        "C:/Windows/Fonts/segoeui.ttf",
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
        "/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf",
    ];
    for path in candidates {
        if let Ok(bytes) = std::fs::read(path) {
            let mut fonts = egui::FontDefinitions::default();
            fonts.font_data.insert(
                "izses_ui".to_string(),
                std::sync::Arc::new(egui::FontData::from_owned(bytes)),
            );
            for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
                fonts
                    .families
                    .entry(family)
                    .or_default()
                    .insert(0, "izses_ui".to_string());
            }
            ctx.set_fonts(fonts);
            return;
        }
    }
}
