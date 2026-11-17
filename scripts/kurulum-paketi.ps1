# IZSES Windows x64 kurulum paketi hazirlama betigi.
#
# Ne yapar:
#   1. Windows x64 icin surum derlemesini alir.
#   2. Uygulama ikilisini, yazi tipini ve model dosyalarini bir klasore toplar.
#   3. Inno Setup varsa imzalanabilir bir kurulum paketi uretir.
#   4. Paketin SHA-256 ozetini yazar (butunluk dogrulamasi icin).
#
# Kullanim (Windows PowerShell, yonetici gerekmez):
#   .\scripts\kurulum-paketi.ps1
#   .\scripts\kurulum-paketi.ps1 -ModelKlasoru "C:\modeller\whisper-large-v3-turbo"

param(
    [string]$ModelKlasoru = "",
    [string]$Cikis = "dist",
    [switch]$AtlaDerleme
)

$ErrorActionPreference = "Stop"
$kok = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
Set-Location $kok

Write-Host "IZSES kurulum paketi hazirlaniyor" -ForegroundColor Cyan

# ---- 1. Derleme ----
if (-not $AtlaDerleme) {
    Write-Host "Windows x64 surum derlemesi aliniyor..."
    cargo build --release --target x86_64-pc-windows-msvc -p izses
    if ($LASTEXITCODE -ne 0) { throw "Derleme basarisiz" }
}

$ikili = Join-Path $kok "target\x86_64-pc-windows-msvc\release\izses.exe"
if (-not (Test-Path $ikili)) { throw "Uygulama ikilisi bulunamadi: $ikili" }

# ---- 2. Paket klasoru ----
$paket = Join-Path $kok "$Cikis\izses"
if (Test-Path $paket) { Remove-Item -Recurse -Force $paket }
New-Item -ItemType Directory -Force -Path $paket | Out-Null
New-Item -ItemType Directory -Force -Path (Join-Path $paket "fonts") | Out-Null

Copy-Item $ikili (Join-Path $paket "izses.exe")
Copy-Item (Join-Path $kok "README.md") $paket -ErrorAction SilentlyContinue
Copy-Item (Join-Path $kok "NOTICE.md") $paket -ErrorAction SilentlyContinue

# PDF ciktisi icin gomulecek yazi tipi.
$yazitipleri = @(
    "$env:WINDIR\Fonts\segoeui.ttf",
    "$env:WINDIR\Fonts\arial.ttf"
)
foreach ($y in $yazitipleri) {
    if (Test-Path $y) {
        Copy-Item $y (Join-Path $paket "fonts\DejaVuSans.ttf") -Force
        Write-Host "Yazi tipi eklendi: $y"
        break
    }
}

# ---- 3. Model dosyalari ----
if ($ModelKlasoru -and (Test-Path $ModelKlasoru)) {
    $modelHedef = Join-Path $paket "modeller\whisper-large-v3-turbo"
    New-Item -ItemType Directory -Force -Path $modelHedef | Out-Null
    Copy-Item "$ModelKlasoru\*" $modelHedef -Recurse -Force
    Write-Host "Model dosyalari eklendi: $modelHedef"
} else {
    Write-Host "UYARI: Model klasoru verilmedi. Paket model olmadan hazirlaniyor." -ForegroundColor Yellow
    Write-Host "       Kurulum sonrasi: .\scripts\model-indir.ps1" -ForegroundColor Yellow
}

# ---- 4. SHA-256 ----
$exe = Join-Path $paket "izses.exe"
$hash = (Get-FileHash $exe -Algorithm SHA256).Hash.ToLower()
$hashDosyasi = Join-Path $paket "izses.exe.sha256"
"$hash  izses.exe" | Out-File -Encoding ascii $hashDosyasi
Write-Host "izses.exe SHA-256: $hash"

# ---- 5. Inno Setup paketi (varsa) ----
$iscc = "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe"
if (Test-Path $iscc) {
    $iss = Join-Path $kok "installer\izses.iss"
    if (Test-Path $iss) {
        Write-Host "Kurulum paketi uretiliyor (Inno Setup)..."
        & $iscc "/DMySourceDir=$paket" "/DMyOutputDir=$(Join-Path $kok $Cikis)" $iss
        if ($LASTEXITCODE -eq 0) {
            $setup = Get-ChildItem "$Cikis\*.exe" | Where-Object { $_.Name -like "*kurulum*" } | Select-Object -First 1
            if ($setup) {
                $setupHash = (Get-FileHash $setup.FullName -Algorithm SHA256).Hash.ToLower()
                "$setupHash  $($setup.Name)" | Out-File -Encoding ascii "$($setup.FullName).sha256"
                Write-Host "Kurulum paketi: $($setup.FullName)" -ForegroundColor Green
                Write-Host "SHA-256: $setupHash"
            }
        }
    }
} else {
    Write-Host "Inno Setup bulunamadi; yalnizca tasinabilir klasor hazirlandi." -ForegroundColor Yellow
    Write-Host "Kurulum paketi icin: https://jrsoftware.org/isdl.php"
}

Write-Host ""
Write-Host "Hazir: $paket" -ForegroundColor Green
