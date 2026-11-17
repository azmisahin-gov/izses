# İZSES Kullanım Kılavuzu

Bu kılavuz, 56.12 uzmanlık alanında görev yapan bilirkişi içindir.
Teknik bilgi gerektirmez.

---

## 1. Kurulum

1. `izses-kurulum-<sürüm>-x64.exe` dosyasına çift tıklayın.
2. Ekrandaki adımları izleyin. Kurulum; uygulamayı, gerekli bileşenleri ve
   konuşma çözümleme modelini bilgisayarınıza yerleştirir.
3. Kurulum bitince Başlat menüsünden **İZSES**'i açın.

Kurulumdan sonra terminal, Python veya ek bir program gerekmez. İnceleme
işlemleri internet bağlantısı olmadan çalışır.

---

## 2. Yeni inceleme oluşturma

Uygulama açıldığında karşınıza inceleme yönetimi gelir.

1. Sol taraftaki **Yeni İnceleme Oluştur** formunu doldurun:
   - Görevlendiren mahkeme veya merci
   - Dosya / soruşturma numarası
   - Görevlendirme tarihi ve son teslim tarihi
   - Taraf bilgileri (her satıra bir taraf: `Şüpheli: Ad Soyad`)
   - İnceleme konusu
   - Bilirkişi adı, sicil numarası, uzmanlık alanı
   - Görevlendirme yazısındaki sorular (her satıra bir soru)
2. **İnceleme Oluştur** düğmesine basın.

Bu bilgiler bir kez girilir ve incelemenin tamamında kullanılır. Rapor
hazırlarken tekrar istenmez.

**Devam eden bir incelemeyi açmak için:** Sağdaki listeden incelemeyi bulup
**Aç** düğmesine basın. Son açtığınız incelemeler üstteki kısayollarda görünür.

---

## 3. Delil materyalini alma

1. Ses kayıtlarının bulunduğu flash belleği, SD kartı veya diski takın.
2. **Delil Materyalleri** sekmesine geçin.
3. **Aygıtları Yenile** düğmesine basın.
4. İncelemek istediğiniz aygıtın yanındaki **İncelemeye Kaynak Olarak Ekle**
   düğmesine basın.
5. Kaynağın yanındaki **Tara** düğmesine basın.

Uygulama, kaynaktaki dosyaları listeler ve ses dosyalarını otomatik tanır.

6. Ses dosyalarını işaretleyin (**Tüm Ses Dosyalarını Seç** düğmesi hepsini
   seçer).
7. **Seçili Dosyaları İncelemeye Aktar** düğmesine basın.

Uygulama her dosyayı kopyalar, kaynak ve kopya için SHA-256 değerini
karşılaştırır ve sonucu gösterir.

> **Önemli:** Kaynak dosyalarınız hiçbir zaman değiştirilmez. Uygulama
> kaynağa yazmaz. Ancak bu, aygıtın donanımsal olarak yazmaya karşı korumalı
> olduğu anlamına gelmez; fiziksel koruma ayrı bir konudur.

Aktarım sonucu **Doğrulandı** değilse, o dosya güvenilir sayılmaz ve rapora
alınmaz.

---

## 4. Otomatik deşifre

1. **Otomatik Deşifre** sekmesine geçin.
2. Model durumunun **Hazır** olduğunu doğrulayın.
3. **Deşifreyi Başlat** düğmesine basın.

Uygulama kayıtları sırayla metne dönüştürür. İlerleme çubuğu işlemin durumunu
gösterir.

- İşlem tamamen bilgisayarınızda çalışır. Ses verisi hiçbir yere gönderilmez.
- Uzun kayıtlar parçalara ayrılır; her parça bittiğinde kaydedilir.
- İşlem kesilirse **Kaldığı Yerden Sürdür** düğmesiyle devam edebilirsiniz.
  Tamamlanan bölümler tekrar çözülmez.
- **Durdur** düğmesine bastığınızda tamamlanan parçalar korunur.

---

## 5. Metin kontrolü

**Metin Kontrolü** sekmesi ana çalışma alanınızdır.

1. Üstteki listeden bir kayıt seçin.
2. Dalga formunda kaydı görün. Oynatma düğmeleriyle dinleyin.
   - `Yukarı` / `Aşağı` tuşları bölümler arasında geçiş yapar.
   - Oynatma hızını değiştirebilirsiniz.
3. Her bölüm için:
   - **Otomatik** satırı modelin ürettiği metni gösterir (değiştirilemez).
   - **Düzeltme** alanına doğru metni yazıp **Kaydet** düğmesine basın.
   - **Konuşmacı** alanına etiket yazın (ör. `Konuşmacı 1`).
   - **Gözlem** alanına teknik notunuzu yazın.
   - Gerekli kutuları işaretleyin: *Anlaşılamadı*, *Yeniden dinle*,
     *Gürültü*, *Üst üste konuşma*.
4. Gerekirse **Böl**, **Sonrakiyle Birleştir**, **Sil** düğmelerini kullanın.

> **Önemli:** Modelin ürettiği metin bir taslaktır. Anlaşılmayan bölümlere
> kelime uydurulmaz. Konuşmacı kimliği otomatik belirlenmiş gibi sunulmaz;
> etiketleri siz belirlersiniz. Model güven puanı sağlamıyorsa uydurma puan
> gösterilmez.

---

## 6. Ses inceleme araçları

**Ses Kayıtları** sekmesinde:

- Teknik envanter tablosu (biçim, kodek, süre, kanal, örnekleme, SHA-256)
- Dalga formu
- Spektrogram
- Stereo kayıtlarda sol ve sağ kanalı ayrı dinleme

> **Önemli:** Orijinal dosya üzerinde filtreleme, gürültü azaltma veya
> dönüştürme yapılmaz. Böyle bir işlem gerekirse yeni bir türetilmiş dosya
> oluşturulur ve işlem geçmişe kaydedilir.

---

## 7. Notlar

**Notlar ve Bulgular** sekmesinde inceleme sırasındaki gözlemlerinizi
kaydedebilirsiniz. Not türleri: Genel, Teknik, Gözlem, Bulgu.

---

## 8. Rapor

1. **Rapor** sekmesine geçin.
2. Kendi değerlendirmenizi yazın:
   - İnceleme yöntemi notları (isteğe bağlı)
   - Bilirkişinin değerlendirmesi
   - Gerekçeli sonuç
3. **Rapor Taslağını Oluştur** düğmesine basın.

Uygulama raporu, inceleme boyunca girdiğiniz bilgilerden otomatik derler.
Önizlemeyi inceleyin.

Rapor öncesi **tutarlılık kontrolü** sonuçları gösterilir:
- **Engel** (kırmızı): Rapor üretimini engelleyen eksiklik.
- **Uyarı** (sarı): Dikkat edilmesi gereken durum.
- **Bilgi** (yeşil): Eksik bulunmadı.

4. Raporu **DOCX (Word)**, **PDF** veya **HTML** olarak kaydedin.

Raporlar incelemenin `raporlar` klasörüne kaydedilir.

> **Önemli:** Otomatik oluşturulan bölümler ile sizin yazdığınız bölümler
> raporda ayırt edilebilir. Yapılmayan işlemler rapora yapılmış gibi eklenmez.
> E-imza üretilmez ve hiçbir resmi portala gönderim yapılmaz.

---

## 9. Yedekleme

İnceleme verileriniz şu klasörde tutulur:

```text
Belgeler\IZSES\Incelemeler\<dosya-numarası>\
```

Bu klasörü olduğu gibi kopyalayarak yedek alabilirsiniz. Aynı klasörü geri
kopyaladığınızda inceleme, bütün materyalleri, deşifreleri, düzeltmeleri ve
raporuyla birlikte açılır.

---

## 10. Sık sorulan sorular

**Uygulama internete bağlanıyor mu?**
Hayır. İnceleme sırasında hiçbir veri dışarı gönderilmez. Model kurulumla
birlikte gelir.

**Deşifre çok mu yavaş?**
Konuşma çözümleme CPU üzerinde çalışır. Model ne kadar büyükse o kadar uzun
sürer. Bilgisayarınızda desteklenen bir ekran kartı varsa, Ayarlar'dan GPU
hızlandırmayı açabilirsiniz. GPU kullanılamazsa uygulama otomatik olarak
CPU ile devam eder.

**İşlem yarıda kaldı, kaybolur mu?**
Hayır. Tamamlanan parçalar kaydedilir. **Kaldığı Yerden Sürdür** düğmesiyle
devam edebilirsiniz.

**Bir dosyanın doğrulaması başarısız oldu, ne yapmalıyım?**
Dosyayı yeniden aktarın. Başarısız aktarım güvenilir sayılmaz ve rapora
alınmaz.

**Aygıtım yazma korumalı mı?**
Uygulama kaynağa yazmaz. Ancak bu, aygıtın donanımsal olarak korumalı olduğu
anlamına gelmez. Fiziksel yazma koruması ayrı bir donanım özelliğidir.

**Konuşmacılar otomatik ayrılıyor mu?**
Hayır. Konuşmacı etiketlerini siz belirlersiniz. Uygulama, konuşmacı kimliği
otomatik belirlenmiş gibi sunmaz.
