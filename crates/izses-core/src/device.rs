//! Bağlı depolama aygıtlarının keşfi.
//!
//! Uygulama, bilirkişinin bilgisayarına bağladığı flash bellek, SD kart,
//! taşınabilir disk veya erişilebilir klasörleri listeler. Aygıtlar
//! yalnızca okunur biçimde taranır; hiçbir aygıta yazılmaz.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Aygıt türü.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeviceKind {
    /// Çıkarılabilir medya (USB bellek, SD kart).
    Removable,
    /// Sabit disk veya sistem birimi.
    Fixed,
    /// Ağ konumu veya eşlenmiş sürücü.
    Network,
    /// CD/DVD sürücü.
    Optical,
    /// Bilinmeyen tür.
    Unknown,
}

impl DeviceKind {
    pub fn label_tr(self) -> &'static str {
        match self {
            Self::Removable => "Çıkarılabilir medya",
            Self::Fixed => "Sabit disk",
            Self::Network => "Ağ konumu",
            Self::Optical => "Optik sürücü",
            Self::Unknown => "Bilinmeyen",
        }
    }
}

/// Keşfedilen bir depolama kaynağı.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceInfo {
    /// Kök yol (ör. Windows'ta `E:\`, Linux'ta `/media/user/USB`).
    pub root: PathBuf,
    /// Kullanıcıya gösterilecek etiket (birim adı veya yol).
    pub label: String,
    pub kind: DeviceKind,
    /// Toplam kapasite (bayt), belirlenebiliyorsa.
    pub total_bytes: Option<u64>,
    /// Boş alan (bayt), belirlenebiliyorsa.
    pub free_bytes: Option<u64>,
    /// Dosya sistemi türü (ör. NTFS, FAT32, exFAT).
    pub filesystem: Option<String>,
    /// Kaynağın okunabilir olup olmadığı.
    pub readable: bool,
}

impl DeviceInfo {
    /// Arayüzde gösterilecek kısa açıklama.
    pub fn summary(&self) -> String {
        let mut parts = vec![self.kind.label_tr().to_string()];
        if let Some(fs) = &self.filesystem {
            parts.push(fs.clone());
        }
        if let Some(total) = self.total_bytes {
            parts.push(format_size(total));
        }
        parts.join(" · ")
    }

    /// Yol kökü Windows sürücü harfi mi?
    pub fn drive_letter(&self) -> Option<char> {
        let s = self.root.to_string_lossy();
        let mut chars = s.chars();
        match (chars.next(), chars.next(), chars.next()) {
            (Some(c), Some(':'), Some('\\')) if c.is_ascii_alphabetic() => Some(c),
            (Some(c), Some(':'), Some('/')) if c.is_ascii_alphabetic() => Some(c),
            _ => None,
        }
    }
}

/// Bilinen tüm depolama kaynaklarını listeler.
///
/// Hata durumunda erişilemeyen aygıtlar listeye alınmaz; tüm işlem
/// başarısız sayılmaz.
pub fn list_devices() -> Vec<DeviceInfo> {
    let mut devices = platform::list_devices();
    devices.sort_by(|a, b| a.root.cmp(&b.root));
    devices
}

/// Belirli bir klasörü elle kaynak olarak tanımlar (ör. ağ paylaşımı).
pub fn device_from_path(path: &std::path::Path) -> Option<DeviceInfo> {
    let meta = std::fs::metadata(path).ok()?;
    if !meta.is_dir() {
        return None;
    }
    let label = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string_lossy().to_string());
    Some(DeviceInfo {
        root: path.to_path_buf(),
        label,
        kind: DeviceKind::Unknown,
        total_bytes: None,
        free_bytes: None,
        filesystem: None,
        readable: std::fs::read_dir(path).is_ok(),
    })
}

/// Bayt sayısını okunabilir biçime çevirir.
pub fn format_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut size = bytes as f64;
    let mut unit = 0;
    while size >= 1024.0 && unit < UNITS.len() - 1 {
        size /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{size:.2} {}", UNITS[unit])
    }
}

#[cfg(windows)]
mod platform {
    use super::*;
    use std::ffi::OsString;
    use std::os::windows::ffi::OsStringExt;

    use windows_sys::Win32::Storage::FileSystem::{
        GetDiskFreeSpaceExW, GetDriveTypeW, GetLogicalDrives, GetVolumeInformationW,
    };

    const DRIVE_REMOVABLE: u32 = 2;
    const DRIVE_FIXED: u32 = 3;
    const DRIVE_REMOTE: u32 = 4;
    const DRIVE_CDROM: u32 = 5;

    pub fn list_devices() -> Vec<DeviceInfo> {
        let mut out = Vec::new();
        let mask = unsafe { GetLogicalDrives() };
        if mask == 0 {
            return out;
        }
        for i in 0..26u32 {
            if mask & (1 << i) == 0 {
                continue;
            }
            let letter = (b'A' + i as u8) as char;
            let root = format!("{letter}:\\");
            let wide: Vec<u16> = root.encode_utf16().chain(std::iter::once(0)).collect();
            let drive_type = unsafe { GetDriveTypeW(wide.as_ptr()) };
            let kind = match drive_type {
                DRIVE_REMOVABLE => DeviceKind::Removable,
                DRIVE_FIXED => DeviceKind::Fixed,
                DRIVE_REMOTE => DeviceKind::Network,
                DRIVE_CDROM => DeviceKind::Optical,
                _ => DeviceKind::Unknown,
            };

            let (total, free) = volume_space(&wide);
            let fs = volume_filesystem(&wide);
            let label = volume_label(&wide).unwrap_or_else(|| format!("{letter}:"));
            let readable = std::fs::read_dir(&root).is_ok();

            out.push(DeviceInfo {
                root: PathBuf::from(&root),
                label,
                kind,
                total_bytes: total,
                free_bytes: free,
                filesystem: fs,
                readable,
            });
        }
        out
    }

    fn volume_space(wide: &[u16]) -> (Option<u64>, Option<u64>) {
        let mut free_avail: u64 = 0;
        let mut total: u64 = 0;
        let mut total_free: u64 = 0;
        let ok = unsafe {
            GetDiskFreeSpaceExW(
                wide.as_ptr(),
                &mut free_avail as *mut u64,
                &mut total as *mut u64,
                &mut total_free as *mut u64,
            )
        };
        if ok == 0 {
            (None, None)
        } else {
            (Some(total), Some(free_avail))
        }
    }

    fn volume_filesystem(wide: &[u16]) -> Option<String> {
        let mut fs_buf = [0u16; 32];
        let ok = unsafe {
            GetVolumeInformationW(
                wide.as_ptr(),
                std::ptr::null_mut(),
                0,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                fs_buf.as_mut_ptr(),
                fs_buf.len() as u32,
            )
        };
        if ok == 0 {
            return None;
        }
        Some(wide_to_string(&fs_buf))
    }

    fn volume_label(wide: &[u16]) -> Option<String> {
        let mut name_buf = [0u16; 261];
        let ok = unsafe {
            GetVolumeInformationW(
                wide.as_ptr(),
                name_buf.as_mut_ptr(),
                name_buf.len() as u32,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                0,
            )
        };
        if ok == 0 {
            return None;
        }
        let s = wide_to_string(&name_buf);
        if s.is_empty() {
            None
        } else {
            Some(s)
        }
    }

    fn wide_to_string(buf: &[u16]) -> String {
        let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
        OsString::from_wide(&buf[..end])
            .to_string_lossy()
            .to_string()
    }
}

#[cfg(not(windows))]
mod platform {
    use super::*;

    /// Unix sistemlerde çıkarılabilir medya bağlama noktaları ve kök dizin.
    pub fn list_devices() -> Vec<DeviceInfo> {
        let mut out = Vec::new();

        // Yaygın bağlama noktaları.
        let candidate_dirs = ["/media", "/run/media", "/mnt", "/Volumes"];
        for base in candidate_dirs {
            let base = PathBuf::from(base);
            if !base.is_dir() {
                continue;
            }
            if let Ok(entries) = std::fs::read_dir(&base) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_dir() {
                        out.push(describe(&path, DeviceKind::Removable));
                    } else if base == PathBuf::from("/run/media") {
                        // /run/media/<user>/<device>
                        if let Ok(inner) = std::fs::read_dir(&path) {
                            for e2 in inner.flatten() {
                                let p2 = e2.path();
                                if p2.is_dir() {
                                    out.push(describe(&p2, DeviceKind::Removable));
                                }
                            }
                        }
                    }
                }
            }
        }

        // Kök dizin her zaman erişilebilir bir kaynaktır.
        out.push(describe(std::path::Path::new("/"), DeviceKind::Fixed));
        out
    }

    fn describe(path: &std::path::Path, kind: DeviceKind) -> DeviceInfo {
        let label = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| path.to_string_lossy().to_string());
        DeviceInfo {
            root: path.to_path_buf(),
            label,
            kind,
            total_bytes: None,
            free_bytes: None,
            filesystem: None,
            readable: std::fs::read_dir(path).is_ok(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_sizes() {
        assert_eq!(format_size(512), "512 B");
        assert_eq!(format_size(1024), "1.00 KB");
        assert_eq!(format_size(1024 * 1024), "1.00 MB");
        assert_eq!(format_size(3 * 1024 * 1024 * 1024), "3.00 GB");
    }

    #[test]
    fn lists_at_least_root_on_unix() {
        let devices = list_devices();
        assert!(!devices.is_empty());
    }

    #[test]
    fn detects_directory_path() {
        let tmp = tempfile::tempdir().unwrap();
        let dev = device_from_path(tmp.path()).unwrap();
        assert!(dev.readable);
    }

    #[test]
    fn rejects_file_path() {
        let tmp = tempfile::tempdir().unwrap();
        let f = tmp.path().join("x.txt");
        std::fs::write(&f, b"x").unwrap();
        assert!(device_from_path(&f).is_none());
    }
}
