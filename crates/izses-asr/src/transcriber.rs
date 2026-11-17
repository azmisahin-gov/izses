//! Yerel Whisper çıkarımı (Candle tabanlı).
//!
//! Bu modül, OpenAI Whisper modelini saf Rust ile çalıştırır. Python, C++
//! tabanlı bir çalışma motoru veya harici bir servis kullanılmaz.
//!
//! Tasarım ilkeleri:
//! - CPU çalışma yolu her zaman mevcuttur; GPU yalnızca gerçekten
//!   kullanılabiliyorsa devreye girer.
//! - Uzun sesler 30 saniyelik parçalara ayrılır ve her parça tamamlandığında
//!   sonuç kalıcı olarak kaydedilir. Kesinti hâlinde tamamlanan parçalar korunur.
//! - Türkçe konuşma çözümleme varsayılandır. Çeviri yapılmaz.
//! - Model güven skoru sağlamadığında uydurma değer üretilmez; alanlar boş kalır.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use candle_core::{Device, IndexOp, Tensor};
use candle_nn::ops::softmax;
use candle_nn::VarBuilder;
use candle_transformers::models::whisper::{self as m, Config};
use izses_core::error::{CoreError, CoreResult};
use rand::distr::weighted::WeightedIndex;
use rand::distr::Distribution;
use rand::SeedableRng;
use tokenizers::Tokenizer;

use crate::mel;
use crate::model_store::{ModelSpec, ModelStore};

/// Çıkarım ayarları.
#[derive(Debug, Clone)]
pub struct TranscriberConfig {
    /// Dil kodu (ör. "tr"). Varsayılan Türkçe.
    pub language: String,
    /// Zaman damgalı çıktı üret.
    pub timestamps: bool,
    /// Rastgelelik tohumu (tekrarlanabilirlik için).
    pub seed: u64,
    /// İlk zaman damgası için üst sınır.
    pub max_initial_timestamp_index: Option<u32>,
}

impl Default for TranscriberConfig {
    fn default() -> Self {
        Self {
            language: "tr".to_string(),
            timestamps: true,
            seed: 299_792_458,
            max_initial_timestamp_index: None,
        }
    }
}

/// Modelin ürettiği tek bir metin aralığı.
#[derive(Debug, Clone)]
pub struct SegmentData {
    /// Dosya başından itibaren başlangıç (ms).
    pub start_ms: u64,
    /// Bitiş (ms).
    pub end_ms: u64,
    pub text: String,
    /// Ortalama log-olasılık; model sağlıyorsa doldurulur.
    pub avg_logprob: Option<f64>,
    /// Konuşma yok olasılığı.
    pub no_speech_prob: Option<f64>,
    /// Metin boşsa (yalnızca zaman damgası) doğrudur.
    pub is_empty: bool,
}

/// İlerleme bildirimi.
#[derive(Debug, Clone)]
pub struct Progress {
    pub chunk_index: u32,
    pub total_chunks: u32,
    pub segments_done: u32,
    pub message: String,
}

impl Progress {
    pub fn fraction(&self) -> f32 {
        if self.total_chunks == 0 {
            0.0
        } else {
            (self.chunk_index as f32 / self.total_chunks as f32).clamp(0.0, 1.0)
        }
    }
}

/// Çıkarımın durdurulması için işaret.
#[derive(Debug, Clone, Default)]
pub struct CancelToken(Arc<AtomicBool>);

impl CancelToken {
    pub fn new() -> Self {
        Self(Arc::new(AtomicBool::new(false)))
    }

    pub fn cancel(&self) {
        self.0.store(true, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

/// Çıkarım cihazı.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComputeDevice {
    Cpu,
    Gpu,
}

impl ComputeDevice {
    pub fn label_tr(self) -> &'static str {
        match self {
            Self::Cpu => "CPU",
            Self::Gpu => "GPU",
        }
    }
}

/// Kullanılabilir cihazı seçer.
///
/// GPU yalnızca derleme sırasında ilgili özellik etkinse ve gerçekten
/// erişilebiliyorsa kullanılır; aksi hâlde CPU'ya düşülür.
pub fn select_device(prefer_gpu: bool) -> (Device, ComputeDevice) {
    #[cfg(feature = "cuda")]
    {
        if prefer_gpu {
            if let Ok(dev) = Device::new_cuda(0) {
                return (dev, ComputeDevice::Gpu);
            }
        }
    }
    #[cfg(not(feature = "cuda"))]
    let _ = prefer_gpu;
    (Device::Cpu, ComputeDevice::Cpu)
}

/// Model ve tokenizer'ı birlikte tutan çıkarım motoru.
pub struct TranscriptEngine {
    model: m::model::Whisper,
    tokenizer: Tokenizer,
    config: Config,
    device: Device,
    compute: ComputeDevice,
    rng: rand::rngs::StdRng,
    suppress_tokens: Tensor,
    sot_token: u32,
    transcribe_token: u32,
    eot_token: u32,
    no_speech_token: u32,
    no_timestamps_token: u32,
    language_token: u32,
    timestamps: bool,
    max_initial_timestamp_index: Option<u32>,
    /// Model ağırlık dosyasının SHA-256 değeri.
    model_sha256: Option<String>,
    /// Kullanılan model adı.
    model_name: String,
    /// Mel kanal sayısı.
    num_mel_bins: usize,
}

fn token_id(tokenizer: &Tokenizer, token: &str) -> CoreResult<u32> {
    tokenizer
        .token_to_id(token)
        .ok_or_else(|| CoreError::invalid(format!("tokenizer'da eksik token: {token}")))
}

impl TranscriptEngine {
    /// Model dosyalarını yükler.
    pub fn load(
        store: &ModelStore,
        spec: &ModelSpec,
        cfg: &TranscriberConfig,
        prefer_gpu: bool,
    ) -> CoreResult<Self> {
        let weights = store.weights_path(spec);
        let tokenizer_path = store.tokenizer_path(spec);
        let config_path = store.config_path(spec);

        if !weights.is_file() {
            return Err(CoreError::not_found(
                "model ağırlık dosyası (kurulum tamamlanmamış olabilir)",
            ));
        }
        if !tokenizer_path.is_file() {
            return Err(CoreError::not_found("tokenizer dosyası"));
        }
        if !config_path.is_file() {
            return Err(CoreError::not_found("model yapılandırma dosyası"));
        }

        let config: Config = serde_json::from_str(
            &std::fs::read_to_string(&config_path).map_err(|e| {
                CoreError::invalid(format!("yapılandırma okunamadı: {e}"))
            })?,
        )
        .map_err(|e| CoreError::invalid(format!("yapılandırma geçersiz: {e}")))?;

        let tokenizer = Tokenizer::from_file(&tokenizer_path)
            .map_err(|e| CoreError::invalid(format!("tokenizer yüklenemedi: {e}")))?;

        let (device, compute) = select_device(prefer_gpu);

        let vb = unsafe {
            VarBuilder::from_mmaped_safetensors(&[weights.clone()], m::DTYPE, &device)
                .map_err(|e| CoreError::invalid(format!("model ağırlıkları yüklenemedi: {e}")))?
        };
        let model = m::model::Whisper::load(&vb, config.clone())
            .map_err(|e| CoreError::invalid(format!("model oluşturulamadı: {e}")))?;

        let sot_token = token_id(&tokenizer, m::SOT_TOKEN)?;
        let transcribe_token = token_id(&tokenizer, m::TRANSCRIBE_TOKEN)?;
        let eot_token = token_id(&tokenizer, m::EOT_TOKEN)?;
        let no_timestamps_token = token_id(&tokenizer, m::NO_TIMESTAMPS_TOKEN)?;
        let no_speech_token = m::NO_SPEECH_TOKENS
            .iter()
            .find_map(|t| token_id(&tokenizer, t).ok())
            .ok_or_else(|| CoreError::invalid("konuşma yok tokeni bulunamadı"))?;

        let language_token = token_id(&tokenizer, &format!("<|{}|>", cfg.language))
            .map_err(|_| {
                CoreError::invalid(format!("dil desteklenmiyor: {}", cfg.language))
            })?;

        let suppress: Vec<f32> = (0..config.vocab_size as u32)
            .map(|i| {
                if config.suppress_tokens.contains(&i)
                    || (cfg.timestamps && i == no_timestamps_token)
                {
                    f32::NEG_INFINITY
                } else {
                    0f32
                }
            })
            .collect();
        let suppress_tokens = Tensor::new(suppress.as_slice(), &device)
            .map_err(|e| CoreError::invalid(format!("bastırma tablosu: {e}")))?;

        let model_sha256 = izses_core::hash::sha256_file(&weights).ok();

        Ok(Self {
            model,
            tokenizer,
            config,
            device,
            compute,
            rng: rand::rngs::StdRng::seed_from_u64(cfg.seed),
            suppress_tokens,
            sot_token,
            transcribe_token,
            eot_token,
            no_speech_token,
            no_timestamps_token,
            language_token,
            timestamps: cfg.timestamps,
            max_initial_timestamp_index: cfg.max_initial_timestamp_index,
            model_sha256,
            model_name: spec.display_name.to_string(),
            num_mel_bins: spec.num_mel_bins,
        })
    }

    pub fn compute_device(&self) -> ComputeDevice {
        self.compute
    }

    pub fn model_name(&self) -> &str {
        &self.model_name
    }

    pub fn model_sha256(&self) -> Option<&str> {
        self.model_sha256.as_deref()
    }

    pub fn num_mel_bins(&self) -> usize {
        self.num_mel_bins
    }

    /// Toplam parça sayısını hesaplar (30 saniyelik parçalar).
    pub fn plan_chunk_count(sample_count: usize) -> u32 {
        if sample_count == 0 {
            return 1;
        }
        let per = mel::SAMPLES_PER_CHUNK;
        ((sample_count + per - 1) / per).max(1) as u32
    }

    /// Tek bir 30 saniyelik parçayı çözer.
    ///
    /// `chunk_start_ms`, parçanın dosya başından itibaren başlangıcıdır;
    /// üretilen zaman damgaları buna göre kaydırılır.
    pub fn transcribe_chunk(
        &mut self,
        samples: &[f32],
        chunk_start_ms: u64,
    ) -> CoreResult<Vec<SegmentData>> {
        let mel_values = mel::compute_mel(samples, self.num_mel_bins);
        if mel_values.is_empty() {
            return Ok(Vec::new());
        }
        let frames = mel_values.len() / self.num_mel_bins;
        if frames == 0 {
            return Ok(Vec::new());
        }
        let mel_tensor = Tensor::from_vec(
            mel_values,
            (1, self.num_mel_bins, frames),
            &self.device,
        )
        .map_err(|e| CoreError::invalid(format!("mel tensörü: {e}")))?;

        self.run_on_mel(&mel_tensor, chunk_start_ms)
    }

    /// Mel tensörü üzerinde kod çözme döngüsünü çalıştırır.
    fn run_on_mel(&mut self, mel: &Tensor, chunk_start_ms: u64) -> CoreResult<Vec<SegmentData>> {
        let (_, _, content_frames) = mel
            .dims3()
            .map_err(|e| CoreError::invalid(format!("mel boyutu: {e}")))?;
        let mut seek = 0usize;
        let mut out = Vec::new();

        while seek < content_frames {
            let time_offset_s = (seek * m::HOP_LENGTH) as f64 / m::SAMPLE_RATE as f64;
            let segment_size = usize::min(content_frames - seek, m::N_FRAMES);
            let mel_segment = mel
                .narrow(2, seek, segment_size)
                .map_err(|e| CoreError::invalid(format!("mel dilimi: {e}")))?;
            let segment_duration_s =
                (segment_size * m::HOP_LENGTH) as f64 / m::SAMPLE_RATE as f64;
            seek += segment_size;

            let dr = self.decode_with_fallback(&mel_segment)?;

            // Konuşma yok olasılığı yüksekse ve ortalama olasılık düşükse atla.
            if let Some(nsp) = dr.no_speech_prob {
                if nsp > m::NO_SPEECH_THRESHOLD && dr.avg_logprob < m::LOGPROB_THRESHOLD {
                    continue;
                }
            }

            let base_ms = chunk_start_ms + (time_offset_s * 1000.0) as u64;

            if self.timestamps {
                self.emit_timestamped(&dr, base_ms, segment_duration_s, &mut out)?;
            } else {
                let text = dr.text.trim().to_string();
                let end_ms = base_ms + (segment_duration_s * 1000.0) as u64;
                if !text.is_empty() {
                    out.push(SegmentData {
                        start_ms: base_ms,
                        end_ms,
                        text,
                        avg_logprob: Some(dr.avg_logprob),
                        no_speech_prob: dr.no_speech_prob,
                        is_empty: false,
                    });
                }
            }
        }

        Ok(out)
    }

    /// Zaman damgalı token dizisinden metin aralıkları üretir.
    fn emit_timestamped(
        &self,
        dr: &DecodingResult,
        base_ms: u64,
        segment_duration_s: f64,
        out: &mut Vec<SegmentData>,
    ) -> CoreResult<()> {
        let mut pending: Vec<u32> = Vec::new();
        let mut prev_ts_s: f32 = 0.0;

        let flush = |pending: &mut Vec<u32>,
                         start_s: f32,
                         end_s: f32,
                         out: &mut Vec<SegmentData>|
         -> CoreResult<()> {
            if pending.is_empty() {
                return Ok(());
            }
            let text = self
                .tokenizer
                .decode(pending, true)
                .map_err(|e| CoreError::invalid(format!("metin çözülemedi: {e}")))?
                .trim()
                .to_string();
            pending.clear();
            if text.is_empty() {
                return Ok(());
            }
            let start_ms = base_ms + (start_s.max(0.0) * 1000.0) as u64;
            let end_ms = base_ms + (end_s.max(start_s) * 1000.0) as u64;
            out.push(SegmentData {
                start_ms,
                end_ms,
                text,
                avg_logprob: Some(dr.avg_logprob),
                no_speech_prob: dr.no_speech_prob,
                is_empty: false,
            });
            Ok(())
        };

        for &token in dr.tokens.iter() {
            if token == self.sot_token || token == self.eot_token {
                continue;
            }
            if token > self.no_timestamps_token {
                // Zaman damgası tokeni.
                let ts_s = (token - self.no_timestamps_token) as f32 / 50.0;
                flush(&mut pending, prev_ts_s, ts_s, out)?;
                prev_ts_s = ts_s;
            } else {
                pending.push(token);
            }
        }
        // Sona kalan metin için bitiş zamanı segment süresidir.
        flush(
            &mut pending,
            prev_ts_s,
            segment_duration_s as f32,
            out,
        )?;
        Ok(())
    }

    /// Sıcaklık geri düşüşü ile kod çözme.
    fn decode_with_fallback(&mut self, segment: &Tensor) -> CoreResult<DecodingResult> {
        let mut last_err: Option<CoreError> = None;
        for (i, &t) in m::TEMPERATURES.iter().enumerate() {
            match self.decode(segment, t) {
                Ok(dr) => {
                    if i == m::TEMPERATURES.len() - 1 {
                        return Ok(dr);
                    }
                    let needs_fallback = dr.compression_ratio > m::COMPRESSION_RATIO_THRESHOLD
                        || dr.avg_logprob < m::LOGPROB_THRESHOLD;
                    let no_speech = dr.no_speech_prob.unwrap_or(0.0);
                    if !needs_fallback || no_speech > m::NO_SPEECH_THRESHOLD {
                        return Ok(dr);
                    }
                }
                Err(e) => last_err = Some(e),
            }
        }
        Err(last_err.unwrap_or_else(|| CoreError::invalid("kod çözme başarısız")))
    }

    /// Tek bir sıcaklık değeri ile kod çözme.
    fn decode(&mut self, mel: &Tensor, temperature: f64) -> CoreResult<DecodingResult> {
        let audio_features = self
            .model
            .encoder
            .forward(mel, true)
            .map_err(|e| CoreError::invalid(format!("kodlayıcı: {e}")))?;

        let sample_len = self.config.max_target_positions / 2;
        let mut sum_logprob = 0f64;
        let mut no_speech_prob: Option<f64> = None;
        let mut tokens = vec![self.sot_token, self.language_token, self.transcribe_token];
        if !self.timestamps {
            tokens.push(self.no_timestamps_token);
        }

        for i in 0..sample_len {
            let tokens_t = Tensor::new(tokens.as_slice(), mel.device())
                .and_then(|t| t.unsqueeze(0))
                .map_err(|e| CoreError::invalid(format!("token tensörü: {e}")))?;

            let ys = self
                .model
                .decoder
                .forward(&tokens_t, &audio_features, i == 0)
                .map_err(|e| CoreError::invalid(format!("kod çözücü: {e}")))?;

            if i == 0 {
                let logits = self
                    .model
                    .decoder
                    .final_linear(&ys.i(..1).map_err(candle_err)?)
                    .map_err(candle_err)?
                    .i(0)
                    .map_err(candle_err)?
                    .i(0)
                    .map_err(candle_err)?;
                let probs = softmax(&logits, 0).map_err(candle_err)?;
                no_speech_prob = probs
                    .i(self.no_speech_token as usize)
                    .ok()
                    .and_then(|t| t.to_scalar::<f32>().ok())
                    .map(|v| v as f64);
            }

            let (_, seq_len, _) = ys
                .dims3()
                .map_err(|e| CoreError::invalid(format!("çıktı boyutu: {e}")))?;
            let logits = self
                .model
                .decoder
                .final_linear(&ys.i((..1, seq_len - 1..)).map_err(candle_err)?)
                .map_err(candle_err)?
                .i(0)
                .map_err(candle_err)?
                .i(0)
                .map_err(candle_err)?;

            let logits = if self.timestamps {
                self.apply_timestamp_rules(&logits, &tokens)?
            } else {
                logits
            };

            let logits = logits
                .broadcast_add(&self.suppress_tokens)
                .map_err(candle_err)?;

            let next_token = if temperature > 0f64 {
                let prs = softmax(&(&logits / temperature).map_err(candle_err)?, 0)
                    .map_err(candle_err)?;
                let probs: Vec<f32> = prs.to_vec1().map_err(candle_err)?;
                // Sıfır olasılıklı tokenler WeightedIndex tarafından reddedilir;
                // küçük bir taban ekleyerek geçerli bir dağılım kur.
                let safe: Vec<f32> = probs.iter().map(|&p| if p.is_finite() && p > 0.0 { p } else { 1e-12 }).collect();
                let distr = WeightedIndex::new(&safe)
                    .map_err(|e| CoreError::invalid(format!("olasılık dağılımı: {e}")))?;
                distr.sample(&mut self.rng) as u32
            } else {
                let values: Vec<f32> = logits.to_vec1().map_err(candle_err)?;
                values
                    .iter()
                    .enumerate()
                    .filter(|(_, v)| v.is_finite())
                    .max_by(|(_, a), (_, b)| a.total_cmp(b))
                    .map(|(idx, _)| idx as u32)
                    .unwrap_or(self.eot_token)
            };

            let prob = softmax(&logits, candle_core::D::Minus1)
                .map_err(candle_err)?
                .i(next_token as usize)
                .ok()
                .and_then(|t| t.to_scalar::<f32>().ok())
                .map(|v| v as f64)
                .unwrap_or(1.0);

            tokens.push(next_token);
            if next_token == self.eot_token || tokens.len() > self.config.max_target_positions {
                break;
            }
            if prob > 0.0 {
                sum_logprob += prob.ln();
            }
        }

        let text = self
            .tokenizer
            .decode(&tokens, true)
            .map_err(|e| CoreError::invalid(format!("metin çözülemedi: {e}")))?;
        let avg_logprob = sum_logprob / tokens.len().max(1) as f64;
        let compression_ratio = compression_ratio(&text);

        Ok(DecodingResult {
            tokens,
            text,
            avg_logprob,
            no_speech_prob,
            compression_ratio,
        })
    }

    /// Zaman damgası kurallarını uygular (Whisper referans davranışı).
    fn apply_timestamp_rules(&self, input_logits: &Tensor, tokens: &[u32]) -> CoreResult<Tensor> {
        let device = input_logits.device().clone();
        let timestamp_begin = self.no_timestamps_token + 1;
        let vocab_size = self.config.vocab_size as u32;

        // Örneklenen tokenler: ilk üçü (sot, dil, görev) atlanır.
        let sample_begin = 3usize;
        let sampled: &[u32] = if tokens.len() > sample_begin {
            &tokens[sample_begin..]
        } else {
            &[]
        };

        let mut masks: Vec<Vec<f32>> = Vec::new();
        let mut buf = vec![0f32; vocab_size as usize];

        if !sampled.is_empty() {
            let last_was_ts = sampled.last().map(|&t| t >= timestamp_begin).unwrap_or(false);
            let penult_was_ts = if sampled.len() >= 2 {
                sampled[sampled.len() - 2] >= timestamp_begin
            } else {
                false
            };

            if last_was_ts {
                if penult_was_ts {
                    // Ardışık iki zaman damgası olamaz: zaman damgalarını bastır.
                    for i in 0..vocab_size {
                        buf[i as usize] = if i >= timestamp_begin {
                            f32::NEG_INFINITY
                        } else {
                            0.0
                        };
                    }
                    masks.push(buf.clone());
                } else {
                    // Zaman damgasından sonra metin gelmeli: EOT öncesini bastır.
                    for i in 0..vocab_size {
                        buf[i as usize] = if i < self.eot_token {
                            f32::NEG_INFINITY
                        } else {
                            0.0
                        };
                    }
                    masks.push(buf.clone());
                }
            }

            // Zaman damgaları azalamaz.
            let ts_tokens: Vec<u32> = sampled
                .iter()
                .filter(|&&t| t >= timestamp_begin)
                .cloned()
                .collect();
            if !ts_tokens.is_empty() {
                let ts_last = if last_was_ts && !penult_was_ts {
                    *ts_tokens.last().unwrap()
                } else {
                    ts_tokens.last().unwrap() + 1
                };
                for i in 0..vocab_size {
                    buf[i as usize] = if i >= timestamp_begin && i < ts_last {
                        f32::NEG_INFINITY
                    } else {
                        0.0
                    };
                }
                masks.push(buf.clone());
            }
        }

        // Başlangıçta zaman damgası zorunlu.
        if tokens.len() == sample_begin {
            for i in 0..vocab_size {
                buf[i as usize] = if i < timestamp_begin {
                    f32::NEG_INFINITY
                } else {
                    0.0
                };
            }
            masks.push(buf.clone());

            if let Some(max_idx) = self.max_initial_timestamp_index {
                let last_allowed = timestamp_begin + max_idx;
                if last_allowed < vocab_size {
                    for i in 0..vocab_size {
                        buf[i as usize] = if i > last_allowed {
                            f32::NEG_INFINITY
                        } else {
                            0.0
                        };
                    }
                    masks.push(buf.clone());
                }
            }
        }

        let mut logits = input_logits.clone();
        for mask in masks {
            let t = Tensor::new(mask.as_slice(), &device).map_err(candle_err)?;
            logits = logits.broadcast_add(&t).map_err(candle_err)?;
        }

        // Zaman damgası toplam olasılığı metin tokeninden büyükse zaman damgası seç.
        let log_probs = candle_nn::ops::log_softmax(&logits, 0).map_err(candle_err)?;
        let ts_log_probs = log_probs
            .narrow(
                0,
                timestamp_begin as usize,
                vocab_size as usize - timestamp_begin as usize,
            )
            .map_err(candle_err)?;
        let text_log_probs = log_probs
            .narrow(0, 0, timestamp_begin as usize)
            .map_err(candle_err)?;

        let ts_logprob = {
            let max_val = ts_log_probs.max(0).map_err(candle_err)?;
            let shifted = ts_log_probs
                .broadcast_sub(&max_val)
                .map_err(candle_err)?;
            let sum_exp = shifted.exp().map_err(candle_err)?.sum(0).map_err(candle_err)?;
            let log_sum = sum_exp.log().map_err(candle_err)?;
            max_val
                .broadcast_add(&log_sum)
                .map_err(candle_err)?
                .to_scalar::<f32>()
                .map_err(candle_err)?
        };
        let max_text_logprob = text_log_probs
            .max(0)
            .map_err(candle_err)?
            .to_scalar::<f32>()
            .map_err(candle_err)?;

        if ts_logprob > max_text_logprob {
            for i in 0..vocab_size {
                buf[i as usize] = if i < timestamp_begin {
                    f32::NEG_INFINITY
                } else {
                    0.0
                };
            }
            let t = Tensor::new(buf.as_slice(), &device).map_err(candle_err)?;
            logits = logits.broadcast_add(&t).map_err(candle_err)?;
        }

        Ok(logits)
    }
}

fn candle_err(e: candle_core::Error) -> CoreError {
    CoreError::invalid(format!("çıkarım hatası: {e}"))
}

/// Metin sıkıştırma oranı (tekrarlayan metni yakalamak için).
fn compression_ratio(text: &str) -> f64 {
    if text.is_empty() {
        return 1.0;
    }
    // Gzip benzeri kaba tahmin: benzersiz karakter oranının tersi.
    let bytes = text.as_bytes();
    let mut counts = std::collections::HashMap::new();
    for b in bytes {
        *counts.entry(*b).or_insert(0usize) += 1;
    }
    let entropy: f64 = counts
        .values()
        .map(|&c| {
            let p = c as f64 / bytes.len() as f64;
            -p * p.log2()
        })
        .sum();
    let max_entropy = (counts.len() as f64).log2().max(1.0);
    if entropy <= 0.0 {
        return 100.0;
    }
    (max_entropy / entropy).max(1.0)
}

struct DecodingResult {
    tokens: Vec<u32>,
    text: String,
    avg_logprob: f64,
    no_speech_prob: Option<f64>,
    compression_ratio: f64,
}

/// Sesi parçalara bölüp sırayla çözer.
///
/// Her parça tamamlandığında `on_chunk` çağrılır; böylece uygulama sonucu
/// kalıcı olarak kaydedebilir. Kesinti veya iptal hâlinde tamamlanan parçalar
/// korunur ve kalan parçalar `from_chunk` ile yeniden başlatılabilir.
pub fn transcribe(
    engine: &mut TranscriptEngine,
    samples: &[f32],
    from_chunk: u32,
    cancel: &CancelToken,
    mut on_progress: impl FnMut(Progress),
    mut on_chunk: impl FnMut(u32, &[SegmentData]) -> CoreResult<()>,
) -> CoreResult<u32> {
    let per = mel::SAMPLES_PER_CHUNK;
    let total = TranscriptEngine::plan_chunk_count(samples.len());
    let mut completed = from_chunk;

    for chunk_index in from_chunk..total {
        if cancel.is_cancelled() {
            break;
        }
        let start = chunk_index as usize * per;
        let end = (start + per).min(samples.len());
        let mut chunk = samples[start..end].to_vec();
        if chunk.len() < per {
            chunk.resize(per, 0.0);
        }
        let chunk_start_ms = (chunk_index as u64) * (per as u64) * 1000 / 16_000;

        on_progress(Progress {
            chunk_index,
            total_chunks: total,
            segments_done: chunk_index,
            message: format!("Parça {}/{} işleniyor", chunk_index + 1, total),
        });

        let segments = engine.transcribe_chunk(&chunk, chunk_start_ms)?;
        on_chunk(chunk_index, &segments)?;
        completed = chunk_index + 1;
    }

    Ok(completed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plans_chunks_by_30_seconds() {
        assert_eq!(TranscriptEngine::plan_chunk_count(0), 1);
        assert_eq!(TranscriptEngine::plan_chunk_count(16_000), 1);
        assert_eq!(TranscriptEngine::plan_chunk_count(480_000), 1);
        assert_eq!(TranscriptEngine::plan_chunk_count(480_001), 2);
        assert_eq!(TranscriptEngine::plan_chunk_count(16_000 * 65), 3);
    }

    #[test]
    fn cancel_token_works() {
        let c = CancelToken::new();
        assert!(!c.is_cancelled());
        c.cancel();
        assert!(c.is_cancelled());
    }

    #[test]
    fn compression_ratio_flags_repetition() {
        let repeated = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        let varied = "merhaba dünya, bu bir deneme metnidir";
        assert!(compression_ratio(repeated) > compression_ratio(varied));
    }

    #[test]
    fn select_device_defaults_to_cpu_without_gpu() {
        // Bu test ortamında GPU yok; CPU seçilmeli.
        let (_dev, kind) = select_device(false);
        assert_eq!(kind, ComputeDevice::Cpu);
    }

    #[test]
    fn progress_fraction_is_bounded() {
        let p = Progress {
            chunk_index: 5,
            total_chunks: 10,
            segments_done: 5,
            message: String::new(),
        };
        assert!((p.fraction() - 0.5).abs() < 1e-6);
    }
}
