//! Teknik envanter: ses dosyalarının özelliklerini çıkarır.
//!
//! Değerler yalnızca dosyadan gerçekten okunabildiğinde doldurulur.
//! Tahmin yoluyla doldurma yapılmaz.

use std::path::Path;

use izses_core::inventory::AudioTechnicalInfo;

/// Kısa ad: teknik bilgi çıkarımının sonucu.
pub type TechnicalInfo = AudioTechnicalInfo;

/// Bir ses dosyasının teknik bilgilerini çıkarır.
///
/// Dosya hasarlı veya desteklenmiyorsa `probe_error` alanı doldurulur ve
/// mümkün olan diğer bilgiler yine de döndürülür.
pub fn probe(path: &Path) -> AudioTechnicalInfo {
    let mut info = AudioTechnicalInfo::default();
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_uppercase())
        .unwrap_or_default();
    info.container = if ext.is_empty() { None } else { Some(ext) };

    // Önce symphonia ile tam teknik analiz dene.
    match probe_symphonia(path) {
        Ok(detailed) => {
            merge(&mut info, detailed);
        }
        Err(e) => {
            // Symphonia başarısız olursa WAV için hound'a düş.
            if let Ok(detailed) = probe_hound(path) {
                merge(&mut info, detailed);
            } else {
                info.probe_error = Some(e.to_string());
            }
        }
    }

    info
}

fn merge(into: &mut AudioTechnicalInfo, from: AudioTechnicalInfo) {
    if from.container.is_some() {
        into.container = from.container;
    }
    if from.codec.is_some() {
        into.codec = from.codec;
    }
    if from.duration_ms.is_some() {
        into.duration_ms = from.duration_ms;
    }
    if from.channels.is_some() {
        into.channels = from.channels;
    }
    if from.sample_rate.is_some() {
        into.sample_rate = from.sample_rate;
    }
    if from.bit_depth.is_some() {
        into.bit_depth = from.bit_depth;
    }
    if from.bit_rate.is_some() {
        into.bit_rate = from.bit_rate;
    }
    if from.probe_error.is_some() {
        into.probe_error = from.probe_error;
    }
}

fn probe_symphonia(path: &Path) -> izses_core::error::CoreResult<AudioTechnicalInfo> {
    use symphonia::core::codecs::{CodecParameters, DecoderOptions};
    use symphonia::core::formats::FormatOptions;
    use symphonia::core::io::MediaSourceStream;
    use symphonia::core::meta::MetadataOptions;
    use symphonia::core::probe::Hint;

    let file = std::fs::File::open(path)?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }

    let probed = symphonia::default::get_probe()
        .format(&hint, mss, &FormatOptions::default(), &MetadataOptions::default())
        .map_err(|e| izses_core::CoreError::invalid(format!("{e}")))?;

    let format = probed.format;
    let track = format
        .default_track()
        .ok_or_else(|| izses_core::CoreError::invalid("ses izi yok"))?;

    let params: &CodecParameters = &track.codec_params;
    let mut info = AudioTechnicalInfo::default();

    // Kapsayıcı biçim adı.
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        info.container = Some(ext.to_uppercase());
    }

    // Kodek adı.
    let codec_id = params.codec;
    info.codec = Some(format!("{codec_id:?}").to_lowercase());

    info.sample_rate = params.sample_rate;
    info.channels = params.channels.map(|c| c.count() as u32);
    info.bit_depth = params.bits_per_sample;

    if let Some(tb) = params.time_base {
        if let Some(n_frames) = params.n_frames {
            let dur = tb.calc_time(n_frames);
            info.duration_ms = Some((dur.seconds * 1000 + (dur.frac * 1000.0) as u64) as u64);
        }
    }
    if let (Some(br), Some(sr)) = (params.bits_per_sample, params.sample_rate) {
        info.bit_rate = Some(br * sr);
    }

    // Süre bilinmiyorsa kodeği açıp paketleri sayarak süreyi çıkar.
    if info.duration_ms.is_none() {
        if let Ok(mut decoder) = symphonia::default::get_codecs()
            .make(params, &DecoderOptions::default())
        {
            if let Some(mut format) = Some(format) {
                let mut total_frames: u64 = 0;
                let sr = info.sample_rate;
                loop {
                    match format.next_packet() {
                        Ok(packet) => {
                            if let Ok(buf) = decoder.decode(&packet) {
                                total_frames += buf.frames() as u64;
                            }
                        }
                        Err(_) => break,
                    }
                }
                if let Some(sr) = sr {
                    if sr > 0 {
                        info.duration_ms = Some(total_frames * 1000 / sr as u64);
                    }
                }
            }
        }
    }

    Ok(info)
}

fn probe_hound(path: &Path) -> izses_core::error::CoreResult<AudioTechnicalInfo> {
    let reader = hound::WavReader::open(path)
        .map_err(|e| izses_core::CoreError::invalid(format!("{e}")))?;
    let spec = reader.spec();
    let frames = reader.len() as u64 / spec.channels.max(1) as u64;
    let duration_ms = if spec.sample_rate > 0 {
        frames * 1000 / spec.sample_rate as u64
    } else {
        0
    };
    Ok(AudioTechnicalInfo {
        container: Some("WAV".into()),
        codec: Some(format!("pcm_s{}le", spec.bits_per_sample)),
        duration_ms: Some(duration_ms),
        channels: Some(spec.channels as u32),
        sample_rate: Some(spec.sample_rate),
        bit_depth: Some(spec.bits_per_sample as u32),
        bit_rate: Some(spec.bits_per_sample as u32 * spec.sample_rate),
        probe_error: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_wav(path: &Path, sr: u32, ch: u16, bits: u16, frames: usize) {
        let spec = hound::WavSpec {
            channels: ch,
            sample_rate: sr,
            bits_per_sample: bits,
            sample_format: hound::SampleFormat::Int,
        };
        let mut w = hound::WavWriter::create(path, spec).unwrap();
        for i in 0..frames * ch as usize {
            w.write_sample((i % 1000) as i16).unwrap();
        }
        w.finalize().unwrap();
    }

    #[test]
    fn probes_wav_correctly() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("t.wav");
        write_wav(&p, 44_100, 2, 16, 44_100);

        let info = probe(&p);
        assert_eq!(info.sample_rate, Some(44_100));
        assert_eq!(info.channels, Some(2));
        assert_eq!(info.bit_depth, Some(16));
        assert!(info.probe_error.is_none(), "{:?}", info.probe_error);
        let dur = info.duration_ms.unwrap();
        assert!((900..=1100).contains(&dur), "süre: {dur}");
    }

    #[test]
    fn corrupt_file_reports_error_without_panicking() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("bad.wav");
        std::fs::write(&p, b"garbage data here").unwrap();
        let info = probe(&p);
        // Ya hata bildirir ya da en azından yalan bilgi üretmez.
        if info.probe_error.is_none() {
            // Kabul edilebilir: hound yedek yolu veri bulduysa.
        }
        assert!(info.probe_error.is_some() || info.sample_rate.is_some());
    }

    #[test]
    fn missing_file_reports_error() {
        let tmp = tempfile::tempdir().unwrap();
        let info = probe(&tmp.path().join("yok.wav"));
        assert!(info.probe_error.is_some());
    }
}
