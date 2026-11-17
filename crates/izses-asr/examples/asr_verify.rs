//! Gerçek çıkarım doğrulaması: yerel Whisper modelini yükleyip bir WAV
//! dosyasını metne dönüştürür.
//!
//! Kullanım:
//!   cargo run -p izses-asr --example asr_verify -- <model-klasoru> <wav-dosyasi> [dil]
//!
//! Bu örnek, geliştirme sırasında Candle + Whisper çıkarımının gerçekten
//! çalıştığını doğrulamak içindir. Son kullanıcı uygulaması bu örneği kullanmaz.

use std::path::PathBuf;

use izses_asr::model_store::{ModelSpec, ModelStore};
use izses_asr::transcriber::{CancelToken, TranscriberConfig, TranscriptEngine};

/// Doğrulama için genel model tanımı (klasör adı verilen köke göre belirlenir).
const VERIFY_SPEC: ModelSpec = ModelSpec {
    display_name: "Doğrulama modeli",
    repo: "verify/model",
    weights_file: "model.safetensors",
    tokenizer_file: "tokenizer.json",
    config_file: "config.json",
    num_mel_bins: 80,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!("kullanım: asr_verify <model-klasoru> <wav-dosyasi> [dil]");
        std::process::exit(2);
    }
    let model_dir = PathBuf::from(&args[1]);
    let wav_path = PathBuf::from(&args[2]);
    let language = args.get(3).cloned().unwrap_or_else(|| "en".to_string());

    // ModelStore kökün altında <repo son parçası>/ arar. Verilen klasörün
    // adı beklenenle aynı değilse dosyaları beklenen konuma kopyala.
    let root = model_dir
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| model_dir.clone());
    let store = ModelStore::new(&root);
    let expected = store.model_dir(&VERIFY_SPEC);
    if expected != model_dir {
        std::fs::create_dir_all(&expected)?;
        for f in ["config.json", "tokenizer.json", "model.safetensors"] {
            std::fs::copy(model_dir.join(f), expected.join(f))?;
        }
    }

    let state = store.check_present(&VERIFY_SPEC);
    println!("model durumu: {}", state.label_tr());
    if !state.is_ready() {
        return Err("model hazır değil".into());
    }

    let cfg = TranscriberConfig {
        language: language.clone(),
        timestamps: true,
        ..Default::default()
    };

    println!("model yükleniyor…");
    let mut engine = TranscriptEngine::load(&store, &VERIFY_SPEC, &cfg, false)?;
    println!(
        "model yüklendi (cihaz: {}, ağırlık SHA-256: {})",
        engine.compute_device().label_tr(),
        engine
            .model_sha256()
            .map(|s| s[..16].to_string())
            .unwrap_or_else(|| "<yok>".to_string())
    );

    println!("ses çözülüyor: {}", wav_path.display());
    let audio = izses_audio::decode_to_mono_16k(&wav_path)?;
    println!(
        "ses: {} örnek, {:.1} sn, {} kanal, {} Hz",
        audio.samples.len(),
        audio.duration_ms as f64 / 1000.0,
        audio.source_channels,
        audio.source_sample_rate
    );

    let cancel = CancelToken::new();
    let mut all_segments = Vec::new();
    let completed = izses_asr::transcriber::transcribe(
        &mut engine,
        &audio.samples,
        0,
        &cancel,
        |p| {
            println!("  ilerleme: {:.0}% {}", p.fraction() * 100.0, p.message);
        },
        |_chunk, segs| {
            for s in segs {
                all_segments.push(s.clone());
            }
            Ok(())
        },
    )?;

    println!("\n--- DEŞİFRE ---");
    for seg in &all_segments {
        println!(
            "{}–{}  {}",
            izses_core::timefmt::format_ms(seg.start_ms),
            izses_core::timefmt::format_ms(seg.end_ms),
            seg.text
        );
    }
    println!("--- SON ---");
    println!(
        "tamamlanan parça: {completed}, bölüm sayısı: {}",
        all_segments.len()
    );

    if all_segments.is_empty() {
        return Err("çıkarım hiç metin üretmedi".into());
    }
    Ok(())
}
