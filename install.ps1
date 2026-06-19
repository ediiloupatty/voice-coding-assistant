# install.ps1 — pemasang Voca core (Rust) di Windows:
#   irm https://raw.githubusercontent.com/ediiloupatty/voice-coding-assistant/main/install.ps1 | iex
#
# Override: $env:VOCA_BASE_URL (sumber binary), $env:VOCA_INSTALL_DIR (folder).
# Catatan: mode suara (--voice/--listen) butuh sidecar Python; di Windows
# siapkan manual (lihat rust/README.md) lalu set VOCA_VOICE_PYTHON/HOME.
$ErrorActionPreference = "Stop"

$repo = "ediiloupatty/voice-coding-assistant"
$base = if ($env:VOCA_BASE_URL) { $env:VOCA_BASE_URL }
        else { "https://github.com/$repo/releases/latest/download" }
$dir  = if ($env:VOCA_INSTALL_DIR) { $env:VOCA_INSTALL_DIR }
        else { Join-Path $env:LOCALAPPDATA "Voca" }

$asset = "voca-windows-x64.exe"
$dest  = Join-Path $dir "voca.exe"

New-Item -ItemType Directory -Force -Path $dir | Out-Null
Write-Host "Mengunduh Voca core ($asset)..."
Invoke-WebRequest "$base/$asset" -OutFile $dest

$userPath = [Environment]::GetEnvironmentVariable("Path", "User")
if ($userPath -notlike "*$dir*") {
    [Environment]::SetEnvironmentVariable("Path", "$userPath;$dir", "User")
    Write-Host "PATH user diperbarui. Buka terminal BARU agar 'voca' aktif."
}

Write-Host "Selesai. Jalankan: voca"
Write-Host "  (API key diminta otomatis saat pertama dijalankan)"
