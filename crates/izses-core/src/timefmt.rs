//! Zaman biçimlendirme yardımcıları.
//!
//! Deşifre metinleri ve raporlar için saniye/milisaniye biçiminde
//! zaman damgaları üretir.

use chrono::{DateTime, Utc};

/// Milisaniyeyi `HH:MM:SS.mmm` biçimine çevirir.
pub fn format_ms(ms: u64) -> String {
    let total_secs = ms / 1000;
    let millis = ms % 1000;
    let secs = total_secs % 60;
    let mins = (total_secs / 60) % 60;
    let hours = total_secs / 3600;
    format!("{hours:02}:{mins:02}:{secs:02}.{millis:03}")
}

/// Milisaniyeyi `HH:MM:SS` biçimine çevirir (rapor özetleri için).
pub fn format_ms_short(ms: u64) -> String {
    let total_secs = ms / 1000;
    let secs = total_secs % 60;
    let mins = (total_secs / 60) % 60;
    let hours = total_secs / 3600;
    format!("{hours:02}:{mins:02}:{secs:02}")
}

/// Saniyeyi `HH:MM:SS` biçimine çevirir.
pub fn format_secs(secs: u64) -> String {
    format_ms_short(secs * 1000)
}

/// UTC zamanı okunabilir yerel biçime çevirir.
pub fn format_datetime(dt: &DateTime<Utc>) -> String {
    dt.with_timezone(&chrono::Local)
        .format("%d.%m.%Y %H:%M:%S")
        .to_string()
}

/// UTC zamanı ISO 8601 biçiminde (veritabanı için) döndürür.
pub fn to_iso(dt: &DateTime<Utc>) -> String {
    dt.to_rfc3339()
}

/// ISO 8601 metnini UTC zamanına çevirir.
pub fn parse_iso(s: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(s)
        .ok()
        .map(|d| d.with_timezone(&Utc))
}

/// Milisaniyeyi insan okunur süre biçimine çevirir (ör. `1 sa 2 dk 3 sn`).
pub fn human_duration(ms: u64) -> String {
    let total_secs = ms / 1000;
    let secs = total_secs % 60;
    let mins = (total_secs / 60) % 60;
    let hours = total_secs / 3600;
    let mut parts = Vec::new();
    if hours > 0 {
        parts.push(format!("{hours} sa"));
    }
    if mins > 0 || hours > 0 {
        parts.push(format!("{mins} dk"));
    }
    parts.push(format!("{secs} sn"));
    parts.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_millis() {
        assert_eq!(format_ms(0), "00:00:00.000");
        assert_eq!(format_ms(1234), "00:00:01.234");
        assert_eq!(format_ms(3_661_000), "01:01:01.000");
    }

    #[test]
    fn formats_short() {
        assert_eq!(format_ms_short(3_661_000), "01:01:01");
        assert_eq!(format_secs(59), "00:00:59");
    }

    #[test]
    fn human_duration_reads_naturally() {
        assert_eq!(human_duration(5_000), "5 sn");
        assert_eq!(human_duration(65_000), "1 dk 5 sn");
        assert_eq!(human_duration(3_665_000), "1 sa 1 dk 5 sn");
    }

    #[test]
    fn iso_roundtrip() {
        let now = Utc::now();
        let s = to_iso(&now);
        let back = parse_iso(&s).unwrap();
        assert_eq!(now.timestamp_millis(), back.timestamp_millis());
    }
}
