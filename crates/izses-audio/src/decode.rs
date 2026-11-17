//! Ses çözümleme: desteklenen biçimleri 16 kHz mono f32 örnek dizisine çevirir.
//!
//! Çözümleme tamamen yereldir ve salt okunurdur. Kaynak dosya değiştirilmez.

use std::path::Path;

use izses_core::error::{CoreError, CoreResult};

/// Çözümlenmiş ses verisi.
#[derive(Debug, Clone)]
pub struct DecodedAudio {
    /// 16 kHz, mono, -1.0..1.0 aralığında örnekler.
    pub samples: Vec<f32>,
    /// Kaynak dosyanın örnekleme frekansı.
    pub source_sample_rate: u32,
    /// Kaynak dosyanın kanal sayısı.
    pub source_channels: u32,
    /// Kaynak dosyanın süresi (milisaniye).
    pub duration_ms: u64,
    /// Ayrı kanallar (stereo dinleme için); kaynak mono ise boş.
    pub channels: Vec<Vec<f32>>,
}

impl DecodedAudio {
    /// Örnek sayısına göre süre.
    pub fn duration_from_samples(&self) -> u64 {
        (self.samples.len() as u64 * 1000) / 16_000
    }

    /// 16 kHz'e göre milisaniye aralığını örnek indekslerine çevirir.
    pub fn ms_to_sample(&self, ms: u64) -> usize {
        ((ms as u128 * 16_000) / 1000) as usize
    }
}

/// Bir ses dosyasını 16 kHz mono biçime çözer.
pub fn decode_to_mono_16k(path: &Path) -> CoreResult<DecodedAudio> {
    let decoded = symphonia_decode(path)?;
    let mono = crate::dsp::downmix_to_mono(&decoded.channels);
    let resampled = crate::dsp::resample(&mono, decoded.sample_rate, 16_000);

    let duration_ms = if decoded.sample_rate > 0 {
        (mono.len() as u64 * 1000) / decoded.sample_rate as u64
    } else {
        0
    };

    // Kanalları da 16 kHz'e indirgeyerek sakla (stereo dinleme yardımcı araçları için).
    let channels = decoded
        .channels
        .iter()
        .map(|ch| crate::dsp::resample(ch, decoded.sample_rate, 16_000))
        .collect();

    Ok(DecodedAudio {
        samples: resampled,
        source_sample_rate: decoded.sample_rate,
        source_channels: decoded.channels.len() as u32,
        duration_ms,
        channels,
    })
}

/// Ham çözümlenmiş ses (kaynak örnekleme frekansında).
pub struct RawDecoded {
    pub channels: Vec<Vec<f32>>,
    pub sample_rate: u32,
}

/// Symphonia ile dosyayı çözer.
pub fn symphonia_decode(path: &Path) -> CoreResult<RawDecoded> {
    use symphonia::core::codecs::DecoderOptions;
    use symphonia::core::errors::Error as SymErr;
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
        .format(
            &hint,
            mss,
            &FormatOptions {
                enable_gapless: true,
                ..Default::default()
            },
            &MetadataOptions::default(),
        )
        .map_err(|e| CoreError::invalid(format!("biçim tanınamadı: {e}")))?;

    let mut format = probed.format;

    let track = format
        .default_track()
        .ok_or_else(|| CoreError::invalid("çalınabilir ses izi bulunamadı"))?;
    let track_id = track.id;
    let mut sample_rate = track.codec_params.sample_rate.unwrap_or(0);
    let mut channels: Vec<Vec<f32>> = Vec::new();

    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .map_err(|e| CoreError::invalid(format!("kodek açılamadı: {e}")))?;

    loop {
        let packet = match format.next_packet() {
            Ok(p) => p,
            Err(SymErr::IoError(ref e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(SymErr::ResetRequired) => break,
            Err(e) => {
                // Son paketlerde oluşan hatalar tüm çözümlemeyi iptal etmez;
                // elde edilen örnekler korunur.
                if !channels.is_empty() {
                    break;
                }
                return Err(CoreError::invalid(format!("okuma hatası: {e}")));
            }
        };
        if packet.track_id() != track_id {
            continue;
        }
        match decoder.decode(&packet) {
            Ok(audio_buf) => {
                if sample_rate == 0 {
                    sample_rate = audio_buf.spec().rate;
                }
                let spec_channels = audio_buf.spec().channels.count();
                if channels.is_empty() {
                    channels = vec![Vec::new(); spec_channels];
                }
                // Örnek formatını f32'ye çevir.
                let mut sample_buf = symphonia::core::audio::SampleBuffer::<f32>::new(
                    audio_buf.capacity() as u64,
                    *audio_buf.spec(),
                );
                sample_buf.copy_interleaved_ref(audio_buf);
                let interleaved = sample_buf.samples();
                for (i, &s) in interleaved.iter().enumerate() {
                    let ch = i % spec_channels;
                    if ch < channels.len() {
                        channels[ch].push(s);
                    }
                }
            }
            Err(SymErr::DecodeError(_)) => {
                // Tek bir paket çözülemedi; sonraki pakete geç.
                continue;
            }
            Err(SymErr::IoError(_)) => break,
            Err(e) => {
                if channels.is_empty() {
                    return Err(CoreError::invalid(format!("çözme hatası: {e}")));
                }
                break;
            }
        }
    }

    if channels.is_empty() || channels.iter().all(|c| c.is_empty()) {
        return Err(CoreError::invalid(
            "dosyadan ses örneği elde edilemedi (hasarlı veya desteklenmeyen)",
        ));
    }

    Ok(RawDecoded {
        channels,
        sample_rate,
    })
}

/// WAV dosyaları için hound tabanlı hızlı çözümleme (yedek yol).
pub fn decode_wav_with_hound(path: &Path) -> CoreResult<RawDecoded> {
    let mut reader = hound::WavReader::open(path)
        .map_err(|e| CoreError::invalid(format!("WAV açılamadı: {e}")))?;
    let spec = reader.spec();
    let channels_count = spec.channels as usize;
    let mut channels: Vec<Vec<f32>> = vec![Vec::new(); channels_count];

    match spec.sample_format {
        hound::SampleFormat::Float => {
            for (i, sample) in reader.samples::<f32>().enumerate() {
                if let Ok(s) = sample {
                    channels[i % channels_count].push(s);
                }
            }
        }
        hound::SampleFormat::Int => {
            let max = (1i64 << (spec.bits_per_sample - 1)) as f32;
            for (i, sample) in reader.samples::<i32>().enumerate() {
                if let Ok(s) = sample {
                    channels[i % channels_count].push(s as f32 / max);
                }
            }
        }
    }

    Ok(RawDecoded {
        channels,
        sample_rate: spec.sample_rate,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write_test_wav(path: &Path, sample_rate: u32, channels: u16, samples: &[i16]) {
        let spec = hound::WavSpec {
            channels,
            sample_rate,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut writer = hound::WavWriter::create(path, spec).unwrap();
        for s in samples {
            writer.write_sample(*s).unwrap();
        }
        writer.finalize().unwrap();
    }

    #[test]
    fn decodes_wav_and_downmixes_to_mono() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("test.wav");
        // 1 saniye, 44.1 kHz, stereo, 440 Hz sinüs.
        let mut samples = Vec::new();
        for i in 0..44_100 {
            let v = ((i as f32 * 440.0 * 2.0 * std::f32::consts::PI / 44_100.0).sin() * 16_000.0)
                as i16;
            samples.push(v);
            samples.push(v);
        }
        write_test_wav(&p, 44_100, 2, &samples);

        let decoded = decode_to_mono_16k(&p).unwrap();
        assert_eq!(decoded.source_sample_rate, 44_100);
        assert_eq!(decoded.source_channels, 2);
        // Yaklaşık 1 saniye olmalı.
        let dur = decoded.duration_ms;
        assert!((950..=1050).contains(&dur), "süre: {dur}");
        // 16 kHz'e indirgenmiş olmalı.
        assert!((15_500..=16_500).contains(&decoded.samples.len()));
    }

    #[test]
    fn hound_fallback_matches() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("mono.wav");
        let samples: Vec<i16> = (0..16_000).map(|i| (i % 100) as i16).collect();
        write_test_wav(&p, 16_000, 1, &samples);

        let raw = decode_wav_with_hound(&p).unwrap();
        assert_eq!(raw.sample_rate, 16_000);
        assert_eq!(raw.channels.len(), 1);
        assert_eq!(raw.channels[0].len(), 16_000);
    }

    #[test]
    fn rejects_corrupt_file() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("broken.wav");
        let mut f = std::fs::File::create(&p).unwrap();
        f.write_all(b"not really audio at all").unwrap();
        assert!(decode_to_mono_16k(&p).is_err());
    }

    #[test]
    fn rejects_missing_file() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("nope.wav");
        assert!(decode_to_mono_16k(&p).is_err());
    }
}
