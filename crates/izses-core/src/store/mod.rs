//! Kalıcılık katmanı: SQLite veritabanı + dosya sistemi.
//!
//! Veritabanı ve dosya sistemi birlikte tutarlı çalışır. Her inceleme
//! kendi kayıtlarına sahiptir ve farklı incelemelerin verileri karışmaz.
//! Yarım kalan aktarım ve deşifre işlemleri durum alanları üzerinden
//! tanınabilir ve sürdürülebilir.

mod audit_repo;
mod evidence_repo;
mod investigation_repo;
mod note_repo;
mod schema;
mod transcript_repo;

use std::path::Path;

use rusqlite::Connection;

use crate::error::CoreResult;

/// Tüm kalıcılık işlemlerinin giriş noktası.
pub struct Store {
    conn: Connection,
}

impl Store {
    /// Veritabanını açar; şema yoksa oluşturur.
    pub fn open(path: &Path) -> CoreResult<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path)?;
        Self::from_connection(conn)
    }

    /// Bellek içi veritabanı (testler için).
    pub fn open_in_memory() -> CoreResult<Self> {
        let conn = Connection::open_in_memory()?;
        Self::from_connection(conn)
    }

    fn from_connection(conn: Connection) -> CoreResult<Self> {
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "FULL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        let store = Self { conn };
        schema::migrate(&store.conn)?;
        Ok(store)
    }

    pub(crate) fn conn(&self) -> &Connection {
        &self.conn
    }

    /// Veritabanı bütünlüğünü denetler.
    pub fn integrity_check(&self) -> CoreResult<()> {
        let result: String = self
            .conn
            .query_row("PRAGMA integrity_check", [], |row| row.get(0))?;
        if result != "ok" {
            return Err(crate::error::CoreError::Integrity(format!(
                "veritabanı bütünlük denetimi: {result}"
            )));
        }
        Ok(())
    }
}

/// Birden çok işlemi tek bir atomik işlemde toplar.
pub fn with_transaction<T>(
    store: &mut Store,
    f: impl FnOnce(&rusqlite::Transaction) -> CoreResult<T>,
) -> CoreResult<T> {
    let tx = store.conn.transaction()?;
    let value = f(&tx)?;
    tx.commit()?;
    Ok(value)
}
