//! Ses dosyalarının çözümlenmesi, teknik analizi ve işlenmesi.
//!
//! Bu crate iki iş yapar:
//! 1. Teknik envanter: süre, kanal, örnekleme frekansı, kodek vb. çıkarır.
//! 2. Çıkarım için örnekleme: sesi Whisper'ın beklediği 16 kHz mono
//!    biçime dönüştürür ve dalga formu/spektrogram verisi üretir.
//!
//! Orijinal dosyalar hiçbir zaman değiştirilmez; tüm işlemler okuma
//! yönlüdür. İyileştirme gerektiğinde yeni bir türetilmiş dosya yazılır.

pub mod decode;
pub mod dsp;
pub mod technical;

pub use decode::{decode_to_mono_16k, DecodedAudio};
pub use technical::{probe, TechnicalInfo};
