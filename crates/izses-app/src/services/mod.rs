//! Uygulama servisleri.
//!
//! Servisler, arayüz ile çekirdek katman arasında köprü kurar ve tekrarlayan
//! teknik işleri otomatikleştirir: delil edinimi, otomatik deşifre kuyruğu,
//! inceleme oturumu ve yedekleme.

pub mod acquisition;
pub mod backup;
pub mod session;
pub mod transcription;

pub use session::{create_investigation, InvestigationProgress, Session};
