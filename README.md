# İZSES

**56.12 – Ses Kayıtlarının Metin Hâline Dönüştürülmesi**
Yerel, otomatik ve güvenli bilirkişi çalışma sistemi.

İZSES, 56.12 uzmanlık alanında görev yapan bilirkişinin iş akışını uçtan uca
otomatikleştirir: inceleme dosyasını oluşturur, dijital delil materyalini
güvenli biçimde çalışma alanına alır, ses kayıtlarını yerel yapay zekâ ile
metne dönüştürür ve rapor taslağını hazırlar. Bilirkişi yalnızca üretilen
metni dinleyerek kontrol eder, düzeltmeleri yapar ve mesleki değerlendirmesiyle
raporu tamamlar.

Uygulama **tamamen yereldir**. Ses dosyaları, deşifre metinleri, notlar ve
raporlar hiçbir dış sunucuya gönderilmez.

---

## Temel özellikler

| Alan | Yetenek |
|------|---------|
| İnceleme yönetimi | Yeni inceleme, devam eden/tamamlanan inceleme, kaldığı yerden devam |
| Delil edinimi | Aygıt keşfi, klasör taraması, ses dosyası tanıma, dosya düzeyinde güvenli kopyalama |
| Bütünlük | Kaynak ve kopya için SHA-256 karşılaştırması; uyuşmazlık başarı olarak gösterilmez |
| Teknik envanter | Biçim, kodek, süre, kanal, örnekleme frekansı, bit derinliği, SHA-256 |
| Otomatik deşifre | Candle + Whisper large-v3-turbo ile saf Rust çıkarım; CPU zorunlu, GPU isteğe bağlı |
| Dayanıklılık | Uzun sesleri parçalara böler; her parça kaydedilir; kesintiden sonra sürdürülür |
| Metin kontrolü | Oynatma, dalga formu, zaman damgalı metin, düzeltme, konuşmacı etiketi, işaretler, gözlem |
| Ses araçları | Dalga formu, spektrogram, oynatma hızı, kanal bazlı dinleme |
| Rapor | Otomatik derleme, tutarlılık kontrolü, DOCX / PDF / HTML çıktısı |
| Veri bütünlüğü | İnceleme merkezli yerel yapı, işlem geçmişi, yedekleme ve geri yükleme |

---

## Kurulum

Son kullanıcı teknik bilgiye ihtiyaç duymaz.

1. `izses-kurulum-<sürüm>-x64.exe` dosyasını çalıştırın.
2. Kurulum sihirbazını izleyin. Kurulum; uygulamayı, gerekli bileşenleri ve
   Whisper modelini yerel bilgisayara yerleştirir.
3. Başlat menüsündeki **İZSES** kısayolundan uygulamayı açın.

Kurulumdan sonra terminal, Python, Node.js veya elle yapılandırma gerekmez.
Temel inceleme iş akışı **internet bağlantısı olmadan** çalışır.

### Kaynaktan derleme (geliştiriciler için)

```bash
# Gereksinim: Rust stable
cargo build --release
# Windows x64 için:
cargo build --release --target x86_64-pc-windows-msvc
```

Model dosyalarını hazırlamak için:

```powershell
# Windows PowerShell
.\scripts\model-indir.ps1 -Hedef "$env:USERPROFILE\Documents\IZSES\modeller"
```

---

## Bilirkişinin kullanım akışı

1. **İnceleme oluştur** – Görevlendirme bilgilerini girin (mahkeme, dosya
   numarası, tarihler, taraflar, bilirkişi bilgileri, sorular).
2. **Delil materyallerini al** – Flash belleği/SD kartı takın, *Delil
   Materyalleri* sekmesinde aygıtı seçip kaynak olarak ekleyin, tarayın.
3. **Dosyaları aktar** – Ses dosyalarını seçip incelemeye aktarın. Uygulama
   kaynak ve kopya SHA-256 değerlerini karşılaştırır.
4. **Deşifre et** – *Otomatik Deşifre* sekmesinden kuyruğu başlatın. İşlem
   yerel olarak çalışır; kesinti hâlinde kaldığı yerden sürdürülür.
5. **Kontrol et** – *Metin Kontrolü* sekmesinde kaydı dinleyin, metni
   düzeltin, konuşmacı etiketleyin, gözlem notu ekleyin.
6. **Raporla** – *Rapor* sekmesinde kendi değerlendirmenizi ve gerekçeli
   sonucunuzu yazın, raporu derleyin ve DOCX/PDF/HTML olarak dışa aktarın.

---

## Yerel veri düzeni

```text
<Belgeler>/IZSES/
├── ayarlar.json
├── izses.db                  (inceleme kayıtları)
├── modeller/                 (Whisper model dosyaları)
└── Incelemeler/
    └── 2026-125/
        ├── inceleme_verileri/
        ├── delil_kaynaklari/
        ├── orijinal_dosyalar/     (değiştirilmez kopyalar)
        ├── calisma_ciktilari/
        ├── otomatik_desifreler/
        ├── bilirkisi_duzeltmeleri/
        ├── notlar/
        ├── islem_gecmisi/
        └── raporlar/
```

Her inceleme kendi klasöründe saklanır; farklı incelemelerin verileri
karışmaz. Klasör adı, inceleme oluşturulurken atanır ve bir daha değişmez.

---

## Güvenlik ilkeleri

- Orijinal kaynak dosyalar hiçbir durumda değiştirilmez.
- Aktarım, SHA-256 karşılaştırması ile doğrulanır. Uyuşmazlık hâlinde aktarım
  başarılı sayılmaz ve rapora yapılmış gibi girmez.
- Yarım kalan kopyalar (`.kismi`) doğrulanmış sayılmaz ve açılışta temizlenir.
- Model ağırlık dosyasının bütünlüğü doğrulanır ve özeti rapora yazılır.
- Hassas tam dosya yolları ve aygıt kimlikleri loglara yazılmaz.
- İnceleme verileri dışarı gönderilmez.
- Başarısız işlemler başarılı gibi gösterilmez.
- Gerçek delil içeren aygıtlar geliştirme ve test amacıyla kullanılmaz;
  testlerde sentetik veri kullanılır.

---

## Mimari

Saf Rust ilkesi korunur. Python, JavaScript, Node.js, Electron, Tauri,
tarayıcı motoru veya C++ tabanlı Whisper motoru kullanılmaz.

```text
crates/
├── izses-core     Veri modeli, SQLite kalıcılık, SHA-256, aygıt keşfi, işlem geçmişi
├── izses-audio    Ses çözümleme (symphonia), yeniden örnekleme, dalga formu, spektrogram
├── izses-asr      Candle tabanlı Whisper çıkarımı, mel spektrogram, model bütünlüğü
├── izses-report   Rapor derleme, tutarlılık kontrolü, DOCX/PDF/HTML çıktısı
└── izses-app      Servisler ve egui/eframe masaüstü arayüzü
```

### Kapsam dışı

Bu ürün genel amaçlı bir adli bilişim yazılımı değildir. Fiziksel disk imajı
alma, silinmiş dosya kurtarma, konuşmacının gerçek kimliğini belirleme ve
kayıt montajını otomatik tespit etme kapsam dışıdır. Uygulama, bilirkişinin
yerine mesleki kanaat üretmez; konuşmacı kimliği otomatik belirlenmiş gibi
sunulmaz. E-imza üretilmez ve resmi portala otomatik gönderim yapılmaz.

---

## Lisans

Bu yazılım, bilirkişi kullanımı için geliştirilmiştir. Kullanılan üçüncü taraf
bileşenler ve lisansları için `NOTICE.md` dosyasına bakın.
