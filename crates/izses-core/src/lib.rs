//! IZSES çekirdek altyapısı.
//!
//! Bu crate, inceleme merkezli veri modelini, yerel depolama düzenini,
//! kalıcılığı (SQLite + dosya sistemi), bütünlük doğrulamasını ve
//! işlem geçmişini (audit log) sağlar. Arayüz ve çıkarım motoru bu
//! crate'in üzerine kurulur.

pub mod audit;
pub mod device;
pub mod error;
pub mod hash;
pub mod inventory;
pub mod investigation;
pub mod model;
pub mod store;
pub mod timefmt;
pub mod transcript;

pub use error::{CoreError, CoreResult};
