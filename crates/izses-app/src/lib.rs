//! IZSES – 56.12 Ses Kayıtlarının Metin Hâline Dönüştürülmesi
//!
//! Yerel, otomatik ve güvenli bilirkişi çalışma sistemi.
//!
//! Uygulama tamamen yerel çalışır: ses dosyaları, deşifre metinleri, notlar
//! ve raporlar dış bir sunucuya gönderilmez. Konuşma çözümleme saf Rust
//! (Candle + Whisper) ile yapılır.

pub mod app;
pub mod services;
pub mod settings;
pub mod ui;

pub use app::IzsesApp;
