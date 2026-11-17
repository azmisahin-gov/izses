use std::path::PathBuf;

/// Çekirdek katman hataları.
#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    #[error("veritabanı hatası: {0}")]
    Database(#[from] rusqlite::Error),

    #[error("giriş/çıkış hatası: {0}")]
    Io(#[from] std::io::Error),

    #[error("serileştirme hatası: {0}")]
    Serde(#[from] serde_json::Error),

    #[error("kayıt bulunamadı: {0}")]
    NotFound(String),

    #[error("geçersiz işlem: {0}")]
    Invalid(String),

    #[error("bütünlük doğrulaması başarısız: {0}")]
    Integrity(String),

    #[error("depolama alanı yetersiz: {0}")]
    Storage(String),

    #[error("erişim reddedildi: {0}")]
    Permission(String),
}

impl CoreError {
    pub fn not_found(what: impl Into<String>) -> Self {
        Self::NotFound(what.into())
    }

    pub fn invalid(what: impl Into<String>) -> Self {
        Self::Invalid(what.into())
    }

    /// Hatayı kullanıcıya gösterilebilecek kısa bir etikete indirger.
    ///
    /// Hassas dosya yolları loglara ve arayüze sızdırılmamalıdır; bu yüzden
    /// yalnızca dosya adı (tam yol değil) döndürülür.
    pub fn safe_label(&self, path: &std::path::Path) -> String {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "<bilinmeyen>".to_string());
        format!("{name}: {self}")
    }
}

pub type CoreResult<T> = Result<T, CoreError>;

/// Bir dosya yolunun yalnızca son bileşenini loglamak için yardımcı.
pub fn redact_path(path: &std::path::Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "<gizli-yol>".to_string())
}

/// Tam yolu gizleyerek dosya adını ve üst klasör adını verir.
pub fn redact_path_shallow(path: &std::path::Path) -> String {
    let file = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    let parent = path
        .parent()
        .and_then(|p| p.file_name())
        .map(|n| n.to_string_lossy().to_string());
    match parent {
        Some(p) => format!("{p}/{file}"),
        None => file,
    }
}

/// Yardımcı: PathBuf'u kısaltılmış biçimde döndürür (yalnızca tanılama için).
pub fn short_path(path: &std::path::Path) -> PathBuf {
    path.file_name().map(PathBuf::from).unwrap_or_default()
}
