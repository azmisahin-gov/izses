//! Sayısal ses işleme yardımcıları: kanal indirgeme, yeniden örnekleme,
//! dalga formu özeti ve spektrogram.

use std::f32::consts::PI;

/// Çok kanallı örnekleri mono hâle getirir.
pub fn downmix_to_mono(channels: &[Vec<f32>]) -> Vec<f32> {
    match channels.len() {
        0 => Vec::new(),
        1 => channels[0].clone(),
        n => {
            let len = channels.iter().map(|c| c.len()).max().unwrap_or(0);
            let mut out = vec![0f32; len];
            for ch in channels {
                for (i, &s) in ch.iter().enumerate() {
                    out[i] += s / n as f32;
                }
            }
            out
        }
    }
}

/// Basit ve bağımlılıksız yeniden örnekleme (pencere tabanlı doğrusal interpolasyon).
///
/// Kalite, konuşma tanıma için yeterlidir. Kaynak ve hedef frekans eşitse
/// veri değiştirilmeden döndürülür.
pub fn resample(input: &[f32], from_rate: u32, to_rate: u32) -> Vec<f32> {
    if from_rate == 0 || to_rate == 0 || input.is_empty() || from_rate == to_rate {
        return input.to_vec();
    }
    let ratio = to_rate as f64 / from_rate as f64;
    let out_len = ((input.len() as f64) * ratio).round() as usize;
    let mut out = Vec::with_capacity(out_len);

    // Önceden hesaplanmış düşük geçiren filtre çekirdeği (anti-aliasing).
    // Geçiş bandı dar tutulur; konuşma için yeterli.
    let cutoff = 0.5 * (to_rate.min(from_rate) as f64 / from_rate as f64);
    let kernel = lowpass_kernel(cutoff, 16);

    for i in 0..out_len {
        let src_pos = i as f64 / ratio;
        let idx = src_pos.floor() as isize;
        let frac = (src_pos - idx as f64) as f32;
        let mut acc = 0f32;
        let mut wsum = 0f32;
        for (k, &w) in kernel.iter().enumerate() {
            let offset = k as isize - (kernel.len() as isize / 2);
            let sample_idx = idx + offset;
            if sample_idx < 0 || sample_idx as usize >= input.len() {
                continue;
            }
            let weight = w * (1.0 - (frac - 0.5).abs());
            acc += input[sample_idx as usize] * weight;
            wsum += weight;
        }
        let value = if wsum.abs() > f32::EPSILON {
            acc / wsum
        } else {
            input.get(idx.max(0) as usize).copied().unwrap_or(0.0)
        };
        out.push(value.clamp(-1.0, 1.0));
    }
    out
}

/// Normalize edilmiş alçak geçiren pencere çekirdeği üretir (Hamming pencereli sinc).
fn lowpass_kernel(cutoff: f64, taps: usize) -> Vec<f32> {
    use std::f64::consts::PI as PI64;
    let mut kernel = Vec::with_capacity(taps);
    let mid = (taps - 1) as f64 / 2.0;
    for i in 0..taps {
        let n = i as f64 - mid;
        let sinc = if n.abs() < 1e-9 {
            2.0 * cutoff
        } else {
            (2.0 * PI64 * cutoff * n).sin() / (PI64 * n)
        };
        // Hamming penceresi.
        let window = 0.54 - 0.46 * (2.0 * PI64 * i as f64 / (taps - 1) as f64).cos();
        kernel.push((sinc * window) as f32);
    }
    let sum: f32 = kernel.iter().sum();
    if sum.abs() > f32::EPSILON {
        for k in &mut kernel {
            *k /= sum;
        }
    }
    kernel
}

/// Dalga formu için özet (her kutu için en büyük genlik ve RMS).
#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
pub struct WaveformBin {
    pub peak: f32,
    pub rms: f32,
}

/// Dalga formunu verilen kutu sayısına indirger.
pub fn waveform(samples: &[f32], bins: usize) -> Vec<WaveformBin> {
    if samples.is_empty() || bins == 0 {
        return Vec::new();
    }
    let per = (samples.len() as f64 / bins as f64).max(1.0);
    let mut out = Vec::with_capacity(bins);
    for b in 0..bins {
        let start = (b as f64 * per) as usize;
        let end = (((b + 1) as f64 * per) as usize).min(samples.len());
        if start >= end {
            out.push(WaveformBin { peak: 0.0, rms: 0.0 });
            continue;
        }
        let slice = &samples[start..end];
        let peak = slice.iter().fold(0f32, |m, &s| m.max(s.abs()));
        let rms = (slice.iter().map(|&s| s * s).sum::<f32>() / slice.len() as f32).sqrt();
        out.push(WaveformBin { peak, rms });
    }
    out
}

/// Basit ayrık Fourier dönüşümü ile güç spektrumu (küçük pencereler için).
///
/// Spektrogram görüntüsü üretmek için yeterlidir; performans için pencere
/// uzunluğu küçük tutulmalıdır (256 veya 512 önerilir).
pub fn power_spectrum(window: &[f32]) -> Vec<f32> {
    let n = window.len();
    if n == 0 {
        return Vec::new();
    }
    // Hann penceresi.
    let windowed: Vec<f32> = window
        .iter()
        .enumerate()
        .map(|(i, &s)| s * (0.5 - 0.5 * (2.0 * PI * i as f32 / (n - 1).max(1) as f32).cos()))
        .collect();

    let bins = n / 2 + 1;
    let mut out = vec![0f32; bins];
    for (k, bin) in out.iter_mut().enumerate() {
        let mut re = 0f32;
        let mut im = 0f32;
        for (i, &s) in windowed.iter().enumerate() {
            let angle = -2.0 * PI * k as f32 * i as f32 / n as f32;
            re += s * angle.cos();
            im += s * angle.sin();
        }
        *bin = (re * re + im * im) / n as f32;
    }
    out
}

/// Spektrogram ızgarası üretir: satırlar zaman, sütunlar frekans kutusu.
pub fn spectrogram(samples: &[f32], window_size: usize, hop: usize, rows: usize) -> Vec<Vec<f32>> {
    if samples.is_empty() || window_size == 0 || hop == 0 || rows == 0 {
        return Vec::new();
    }
    let total_frames = if samples.len() > window_size {
        (samples.len() - window_size) / hop + 1
    } else {
        1
    };
    let frame_step = (total_frames as f64 / rows as f64).max(1.0);
    let mut out = Vec::with_capacity(rows);
    for r in 0..rows {
        let frame = (r as f64 * frame_step) as usize;
        let start = frame * hop;
        if start + window_size > samples.len() {
            out.push(Vec::new());
            continue;
        }
        out.push(power_spectrum(&samples[start..start + window_size]));
    }
    out
}

/// Ses seviyesini (dB) hesaplar.
pub fn level_db(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return f32::NEG_INFINITY;
    }
    let rms = (samples.iter().map(|&s| s * s).sum::<f32>() / samples.len() as f32).sqrt();
    if rms <= f32::EPSILON {
        f32::NEG_INFINITY
    } else {
        20.0 * rms.log10()
    }
}

/// Basit gürültü azaltma (türetilmiş kopya için, orijinal dosyaya dokunmaz).
///
/// Yalnızca yumuşatma uygular; bilirkişi tarafından açıkça istenirse kullanılır.
pub fn gentle_denoise(samples: &[f32], window: usize) -> Vec<f32> {
    if samples.is_empty() || window < 2 {
        return samples.to_vec();
    }
    let mut out = Vec::with_capacity(samples.len());
    let half = window / 2;
    for i in 0..samples.len() {
        let start = i.saturating_sub(half);
        let end = (i + half + 1).min(samples.len());
        let avg: f32 = samples[start..end].iter().sum::<f32>() / (end - start) as f32;
        out.push(avg);
    }
    out
}

/// Kazanç uygular (normalizasyon). Türetilmiş kopyalar için.
pub fn apply_gain(samples: &[f32], gain: f32) -> Vec<f32> {
    samples.iter().map(|&s| (s * gain).clamp(-1.0, 1.0)).collect()
}

/// Tepe değere göre normalizasyon kazancını hesaplar.
pub fn peak_normalize_gain(samples: &[f32], target: f32) -> f32 {
    let peak = samples.iter().fold(0f32, |m, &s| m.max(s.abs()));
    if peak <= f32::EPSILON {
        1.0
    } else {
        target / peak
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn downmix_averages_channels() {
        let left = vec![1.0f32, 0.0, -1.0];
        let right = vec![0.0f32, 1.0, -1.0];
        let mono = downmix_to_mono(&[left, right]);
        assert_eq!(mono, vec![0.5, 0.5, -1.0]);
    }

    #[test]
    fn downmix_passthrough_mono() {
        let m = vec![0.1f32, 0.2];
        assert_eq!(downmix_to_mono(&[m.clone()]), m);
    }

    #[test]
    fn resample_changes_length_proportionally() {
        let input: Vec<f32> = (0..48_000)
            .map(|i| (i as f32 * 0.01).sin())
            .collect();
        let out = resample(&input, 48_000, 16_000);
        assert!((out.len() as i64 - 16_000).abs() < 50, "len={}", out.len());
    }

    #[test]
    fn resample_identity_when_same_rate() {
        let input = vec![0.1f32, 0.2, 0.3];
        assert_eq!(resample(&input, 16_000, 16_000), input);
    }

    #[test]
    fn resample_preserves_tone_frequency() {
        // 1 kHz sinüs, 48k -> 16k. Sonuçta hâlâ 1 kHz olmalı.
        let freq = 1000.0;
        let input: Vec<f32> = (0..48_000)
            .map(|i| (2.0 * PI * freq * i as f32 / 48_000.0).sin())
            .collect();
        let out = resample(&input, 48_000, 16_000);
        // Sıfır geçişlerini sayarak frekansı tahmin et.
        let mut crossings = 0;
        for w in out.windows(2) {
            if w[0] <= 0.0 && w[1] > 0.0 {
                crossings += 1;
            }
        }
        let est = crossings as f32;
        assert!((est - 1000.0).abs() < 30.0, "tahmini frekans: {est}");
    }

    #[test]
    fn waveform_bins_have_expected_shape() {
        let samples = vec![0.5f32; 1000];
        let bins = waveform(&samples, 10);
        assert_eq!(bins.len(), 10);
        for b in bins {
            assert!((b.peak - 0.5).abs() < 1e-6);
            assert!((b.rms - 0.5).abs() < 1e-6);
        }
    }

    #[test]
    fn waveform_empty_input() {
        assert!(waveform(&[], 10).is_empty());
        assert!(waveform(&[1.0], 0).is_empty());
    }

    #[test]
    fn spectrum_peaks_at_tone_frequency() {
        let n = 256;
        let freq_bin = 16; // 16. kutu
        let window: Vec<f32> = (0..n)
            .map(|i| (2.0 * PI * freq_bin as f32 * i as f32 / n as f32).cos())
            .collect();
        let spectrum = power_spectrum(&window);
        let peak = spectrum
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.total_cmp(b.1))
            .unwrap()
            .0;
        assert!((peak as i64 - freq_bin as i64).abs() <= 1, "tepe: {peak}");
    }

    #[test]
    fn spectrogram_shape() {
        let samples: Vec<f32> = (0..8000).map(|i| (i as f32 * 0.05).sin()).collect();
        let spec = spectrogram(&samples, 256, 128, 16);
        assert_eq!(spec.len(), 16);
        assert!(spec.iter().all(|row| row.is_empty() || row.len() == 129));
    }

    #[test]
    fn level_db_of_silence_is_neg_inf() {
        assert!(level_db(&[0.0; 100]).is_infinite());
    }

    #[test]
    fn gain_clamps() {
        let out = apply_gain(&[0.8, -0.8], 4.0);
        assert_eq!(out, vec![1.0, -1.0]);
    }

    #[test]
    fn normalize_gain_targets_peak() {
        let gain = peak_normalize_gain(&[0.25, -0.5], 0.9);
        assert!((gain - 1.8).abs() < 1e-6);
    }
}
