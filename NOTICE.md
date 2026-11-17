# Üçüncü Taraf Bileşenler ve Lisanslar

İZSES, aşağıdaki açık kaynak bileşenleri kullanır. Tümü saf Rust veya
izin verici lisanslı bileşenlerdir. Python, JavaScript, Node.js, Electron,
Tauri, tarayıcı motoru veya C++ tabanlı Whisper çıkarım motoru kullanılmaz.

## Uygulama çerçevesi

| Bileşen | Sürüm | Lisans | Kullanım |
|---------|-------|--------|----------|
| eframe / egui | 0.32 | MIT OR Apache-2.0 | Masaüstü arayüz |
| rfd | 0.15 | MIT | Dosya/klasör seçme iletişim kutuları |
| tracing / tracing-subscriber | 0.1 / 0.3 | MIT | Tanılama günlüğü |

## Konuşma çözümleme

| Bileşen | Sürüm | Lisans | Kullanım |
|---------|-------|--------|----------|
| candle-core / candle-nn / candle-transformers | 0.11 | MIT OR Apache-2.0 | Saf Rust tensör ve model çıkarımı |
| tokenizers | 0.23 | Apache-2.0 | Whisper tokenizer (saf Rust regex arka ucu) |
| rand | 0.9 | MIT OR Apache-2.0 | Örnekleme |
| byteorder | 1 | Unlicense OR MIT | Mel filtre katsayılarının okunması |

**Model:** OpenAI Whisper large-v3-turbo (MIT lisansı). Model ağırlıkları
kurulum sırasında yerel bilgisayara yerleştirilir; uygulama çalışırken
internetten indirme yapılmaz. Model ağırlık dosyasının SHA-256 değeri
hesaplanır, işlem geçmişine ve rapora yazılır.

## Ses işleme

| Bileşen | Sürüm | Lisans | Kullanım |
|---------|-------|--------|----------|
| symphonia | 0.5 | MPL-2.0 | Ses çözümleme (WAV, MP3, M4A/AAC, FLAC, OGG) |
| hound | 3.5 | Apache-2.0 | WAV okuma/yazma (yedek yol) |
| rodio | 0.20 | MIT OR Apache-2.0 | Ses oynatma (yalnızca Windows hedefi) |

**Lisans notu (MPL-2.0):** Symphonia MPL-2.0 lisanslıdır. Bu, dosya düzeyinde
bir copyleft lisansıdır: symphonia kaynak dosyalarında yapılan değişiklikler
paylaşılmalıdır. İZSES, symphonia'yı değiştirmeden kullanır ve bağımlılık
olarak çeker; bu nedenle İZSES'in kendi kaynak kodu MPL-2.0 koşullarına
tabi olmaz.

## Veri ve bütünlük

| Bileşen | Sürüm | Lisans | Kullanım |
|---------|-------|--------|----------|
| rusqlite (bundled) | 0.32 | MIT | Gömülü SQLite veritabanı |
| sha2 | 0.10 | MIT OR Apache-2.0 | SHA-256 bütünlük doğrulaması |
| hex | 0.4 | MIT OR Apache-2.0 | Özet biçimlendirme |
| uuid | 1 | Apache-2.0 OR MIT | Varlık kimlikleri |
| chrono | 0.4 | MIT OR Apache-2.0 | Tarih ve saat |
| walkdir | 2 | Unlicense OR MIT | Klasör taraması |

**SQLite:** `rusqlite` bağımlılığı C kaynak kodlu SQLite'ı içerir. SQLite
kamu malıdır (public domain). Bu bileşen, saf Rust ilkesinin bilinçli bir
istisnasıdır: gömülü veritabanı için olgun ve yaygın çözümdür.

## Rapor üretimi

| Bileşen | Sürüm | Lisans | Kullanım |
|---------|-------|--------|----------|
| zip | 2.2 | MIT | DOCX (OOXML) kapsayıcısı |
| ttf-parser | 0.25 | MIT OR Apache-2.0 | PDF için TrueType yazı tipi okuma |
| flate2 | 1 | MIT OR Apache-2.0 | PDF akış sıkıştırma |

PDF üretimi kendi yazıcımızla yapılır; harici PDF motoru kullanılmaz.
Türkçe karakterler için gömülü TrueType yazı tipi kullanılır.

## Serileştirme

| Bileşen | Sürüm | Lisans | Kullanım |
|---------|-------|--------|----------|
| serde / serde_json | 1 | MIT OR Apache-2.0 | Veri serileştirme |
| thiserror | 2 | MIT OR Apache-2.0 | Hata türleri |

## Geliştirme araçları (çalışma zamanında kullanılmaz)

| Bileşen | Lisans | Kullanım |
|---------|--------|----------|
| Inno Setup | Inno Setup License (izin verici) | Windows kurulum paketi |

---

## Lisans denetimi özeti

- Tüm çalışma zamanı bağımlılıkları MIT, Apache-2.0, MPL-2.0, Unlicense veya
  kamu malı lisanslarındadır.
- Hiçbir bağımlılık GPL, AGPL veya ticari kısıtlı lisans altında değildir.
- Model ağırlıkları (Whisper) MIT lisanslıdır ve ticari kullanıma uygundur.
- C kod içeren bağımlılıklar: `rusqlite` (SQLite, kamu malı) ve Windows
  hedefinde `rodio` (cpal/ALSA bağlantıları). Whisper çıkarımı için C++
  motoru kullanılmaz.
