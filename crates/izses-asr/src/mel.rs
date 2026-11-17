//! Mel spektrogram hesaplama (Whisper ön işlemesi).
//!
//! Whisper, 16 kHz mono sesi 30 saniyelik pencereler hâlinde 80 veya 128
//! kanallı mel spektrograma çevirir. Bu modül, Candle örneğindeki ile aynı
//! filtre katsayılarını kullanır; böylece çıktı referans uygulamayla
//! uyumludur.

use candle_transformers::models::whisper as m;

/// 80 kanallı mel filtre katsayıları (Whisper tarafından sağlanır).
const MEL_FILTERS_80: &[u8] = include_bytes!("../data/melfilters.bytes");
/// 128 kanallı mel filtre katsayıları.
const MEL_FILTERS_128: &[u8] = include_bytes!("../data/melfilters128.bytes");

/// Mel filtre katsayılarını float dizisine çevirir.
pub fn mel_filters(num_mel_bins: usize) -> Vec<f32> {
    let bytes = match num_mel_bins {
        80 => MEL_FILTERS_80,
        128 => MEL_FILTERS_128,
        _ => MEL_FILTERS_80,
    };
    let mut filters = vec![0f32; bytes.len() / 4];
    byteorder::LittleEndian::read_f32_into(bytes, &mut filters);
    filters
}

use byteorder::ByteOrder;

/// Ham örnek dizisinden mel spektrogram üretir.
///
/// Dönen vektör `num_mel_bins * frames` uzunluğundadır (satır öncelikli).
pub fn compute_mel(samples: &[f32], num_mel_bins: usize) -> Vec<f32> {
    let filters = mel_filters(num_mel_bins);
    m::audio::pcm_to_mel(
        &m::Config {
            num_mel_bins,
            max_source_positions: 1500,
            d_model: 0,
            encoder_attention_heads: 0,
            encoder_layers: 0,
            vocab_size: 0,
            max_target_positions: 448,
            decoder_attention_heads: 0,
            decoder_layers: 0,
            suppress_tokens: vec![],
        },
        samples,
        &filters,
    )
}

/// Örnek dizisini Candle tensörü için hazır biçimde döndürür: (bins, frames).
pub fn mel_dimensions(num_mel_bins: usize, mel_len: usize) -> (usize, usize) {
    let frames = mel_len.checked_div(num_mel_bins).unwrap_or(0);
    (num_mel_bins, frames)
}

/// 30 saniyelik pencereye karşılık gelen örnek sayısı.
pub const SAMPLES_PER_CHUNK: usize = m::N_SAMPLES;
/// Mel çerçeve cinsinden pencere uzunluğu.
pub const FRAMES_PER_CHUNK: usize = m::N_FRAMES;
/// Örnekleme frekansı (Whisper sabiti).
pub const SAMPLE_RATE: usize = m::SAMPLE_RATE;

/// Uzun sesi 30 saniyelik parçalara böler.
///
/// Son parça kısaysa sıfırla doldurulur (Whisper böyle bekler).
pub fn chunk_samples(samples: &[f32]) -> Vec<Vec<f32>> {
    if samples.is_empty() {
        return vec![vec![0f32; SAMPLES_PER_CHUNK]];
    }
    let mut out = Vec::new();
    let mut i = 0;
    while i < samples.len() {
        let end = (i + SAMPLES_PER_CHUNK).min(samples.len());
        let mut chunk = samples[i..end].to_vec();
        if chunk.len() < SAMPLES_PER_CHUNK {
            chunk.resize(SAMPLES_PER_CHUNK, 0.0);
        }
        out.push(chunk);
        i += SAMPLES_PER_CHUNK;
    }
    out
}

/// Sesi sessizlik bölgelerine göre daha küçük parçalara ayırır.
///
/// Bu, 30 saniyeden kısa konuşma bloklarını birleştirerek daha doğal
/// bölümler elde etmeyi amaçlar. Çok kısa parçalar üretilmez.
pub fn chunk_by_silence(samples: &[f32], target_chunk: usize, min_chunk: usize) -> Vec<Vec<f32>> {
    if samples.len() <= target_chunk {
        let mut chunk = samples.to_vec();
        chunk.resize(target_chunk.min(SAMPLES_PER_CHUNK).max(chunk.len()), 0.0);
        if chunk.len() < SAMPLES_PER_CHUNK {
            chunk.resize(SAMPLES_PER_CHUNK, 0.0);
        }
        return vec![chunk];
    }

    // Enerji tabanlı sessizlik tespiti.
    let window = 1600; // 100 ms
    let threshold = 0.005f32;
    let mut boundaries = vec![0usize];
    let mut i = 0;
    while i + window <= samples.len() {
        let energy =
            (samples[i..i + window].iter().map(|&s| s * s).sum::<f32>() / window as f32).sqrt();
        if energy < threshold {
            // Sessizliğin ortasını kesim noktası olarak işaretle.
            let candidate = i + window / 2;
            if candidate - boundaries.last().copied().unwrap_or(0) >= min_chunk {
                boundaries.push(candidate);
            }
        }
        i += window;
    }
    boundaries.push(samples.len());

    let mut out = Vec::new();
    for pair in boundaries.windows(2) {
        let mut chunk = samples[pair[0]..pair[1]].to_vec();
        if chunk.is_empty() {
            continue;
        }
        if chunk.len() < SAMPLES_PER_CHUNK {
            chunk.resize(SAMPLES_PER_CHUNK, 0.0);
        }
        out.push(chunk);
    }
    if out.is_empty() {
        out.push(vec![0f32; SAMPLES_PER_CHUNK]);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mel_filters_have_expected_lengths() {
        assert_eq!(mel_filters(80).len(), 80 * 201);
        assert_eq!(mel_filters(128).len(), 128 * 201);
    }

    #[test]
    fn computes_mel_for_one_second() {
        let samples: Vec<f32> = (0..16_000).map(|i| (i as f32 * 0.01).sin() * 0.3).collect();
        let mel = compute_mel(&samples, 80);
        let (bins, frames) = mel_dimensions(80, mel.len());
        assert_eq!(bins, 80);
        assert!(frames > 0);
        assert!(mel.iter().all(|v| v.is_finite()));
    }

    #[test]
    fn chunks_pad_to_30_seconds() {
        let samples = vec![0.1f32; 16_000]; // 1 saniye
        let chunks = chunk_samples(&samples);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].len(), SAMPLES_PER_CHUNK);
        // Kısa ses sıfırla doldurulmalı.
        assert!(chunks[0][16_000..].iter().all(|&s| s == 0.0));
    }

    #[test]
    fn chunks_split_long_audio() {
        // 65 saniye -> 3 parça.
        let samples = vec![0.1f32; 16_000 * 65];
        let chunks = chunk_samples(&samples);
        assert_eq!(chunks.len(), 3);
        assert!(chunks.iter().all(|c| c.len() == SAMPLES_PER_CHUNK));
    }

    #[test]
    fn empty_audio_yields_one_silent_chunk() {
        let chunks = chunk_samples(&[]);
        assert_eq!(chunks.len(), 1);
        assert!(chunks[0].iter().all(|&s| s == 0.0));
    }

    #[test]
    fn silence_chunking_splits_on_quiet_regions() {
        // 40 sn konuşma - 2 sn sessizlik - 40 sn konuşma (hedef 30 sn'den uzun).
        let mut samples = vec![0.2f32; 16_000 * 40];
        samples.extend(std::iter::repeat_n(0.0, 16_000 * 2));
        samples.extend(std::iter::repeat_n(0.2, 16_000 * 40));
        let chunks = chunk_by_silence(&samples, SAMPLES_PER_CHUNK, 16_000);
        assert!(chunks.len() >= 2, "parça sayısı: {}", chunks.len());
        assert!(chunks.iter().all(|c| c.len() >= SAMPLES_PER_CHUNK));
    }

    #[test]
    fn short_audio_is_not_split_by_silence() {
        // 10 saniyelik ses hedef parçadan kısa; tek parça olmalı.
        let samples = vec![0.2f32; 16_000 * 10];
        let chunks = chunk_by_silence(&samples, SAMPLES_PER_CHUNK, 16_000);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].len(), SAMPLES_PER_CHUNK);
    }
}
