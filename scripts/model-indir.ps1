# Whisper model dosyalarını yerel bilgisayara indirir.
#
# Bu betik YALNIZCA geliştirici/kurulum hazırlığı içindir. Son kullanıcı
# model dosyalarını kurulum paketiyle hazır alır; uygulama çalışırken hiçbir
# dosya internetten indirilmez.
#
# Kullanım:
#   .\scripts\model-indir.ps1
#   .\scripts\model-indir.ps1 -Hedef "C:\IZSES\modeller"

param(
    [string]$Hedef = "$env:USERPROFILE\Documents\IZSES\modeller",
    [string]$Depo = "openai/whisper-large-v3-turbo",
    [string]$Surum = "main"
)

$ErrorActionPreference = "Stop"

Write-Host "İZSES - Whisper model indirme" -ForegroundColor Cyan
Write-Host "Kaynak : $Depo ($Surum)"
Write-Host "Hedef  : $Hedef"
Write-Host ""

$klasor = Join-Path $Hedef ($Depo -split '/')[-1]
New-Item -ItemType Directory -Force -Path $klasor | Out-Null

$dosyalar = @(
    "config.json",
    "tokenizer.json",
    "model.safetensors"
)

foreach ($d in $dosyalar) {
    $url = "https://huggingface.co/$Depo/resolve/$Surum/$d"
    $hedefDosya = Join-Path $klasor $d
    if (Test-Path $hedefDosya) {
        Write-Host "Atlandi (zaten var): $d" -ForegroundColor Yellow
        continue
    }
    Write-Host "Indiriliyor: $d"
    Invoke-WebRequest -Uri $url -OutFile $hedefDosya -UseBasicParsing
}

Write-Host ""
Write-Host "Butunluk dogrulamasi (SHA-256):" -ForegroundColor Cyan
Get-ChildItem $klasor -File | ForEach-Object {
    $hash = (Get-FileHash $_.FullName -Algorithm SHA256).Hash.ToLower()
    Write-Host ("  {0}  {1}" -f $hash, $_.Name)
}

Write-Host ""
Write-Host "Model hazir: $klasor" -ForegroundColor Green
Write-Host "Bu klasoru IZSES Ayarlar ekranindaki 'Model klasoru' alanina girin."
