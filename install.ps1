# ============================================================================
#  install.ps1 — Pemasang LENGKAP Voca di Windows (1 perintah, pengalaman 1:1).
#
#    irm https://raw.githubusercontent.com/ediiloupatty/voice-coding-assistant/main/install.ps1 | iex
#
#  Yang dipasang (dengan bar progres 1-100%):
#    • Binary inti (Rust)  → perintah `voca`
#    • Sidecar SUARA (Python: Whisper + Piper + Silero) bila Python & Git ada
#    • Model suara id + en, lalu minta API key, lalu jalankan `voca` DI SINI
#      (tanpa buka window baru).
#
#  Override: $env:VOCA_BASE_URL (sumber binary), $env:VOCA_INSTALL_DIR (folder
#  binary), $env:VOCA_HOME (folder suara), $env:VOCA_NO_VOICE=1 (lewati suara).
# ============================================================================
$ErrorActionPreference = "Stop"
$ProgressPreference    = "Continue"   # biar bar progres kita tampil

$repo  = "ediiloupatty/voice-coding-assistant"
$base  = if ($env:VOCA_BASE_URL)    { $env:VOCA_BASE_URL }    else { "https://github.com/$repo/releases/latest/download" }
$dir   = if ($env:VOCA_INSTALL_DIR) { $env:VOCA_INSTALL_DIR } else { Join-Path $env:LOCALAPPDATA "Voca" }
$home_ = if ($env:VOCA_HOME)        { $env:VOCA_HOME }        else { Join-Path $env:USERPROFILE ".voca" }
$dest  = Join-Path $dir "voca.exe"

$ACT = "Memasang Voca"
function Step($pct, $msg) { Write-Progress -Activity $ACT -Status $msg -PercentComplete $pct }
function Note($m) { Write-Host "  $m" -ForegroundColor DarkGray }

# ── 1) Unduh binary inti (Rust) ─────────────────────────────────────────────
Step 5 "Menyiapkan folder..."
New-Item -ItemType Directory -Force -Path $dir | Out-Null
Step 12 "Mengunduh binary inti (voca.exe)..."
curl.exe -fsSL "$base/voca-windows-x64.exe" -o $dest
if (-not (Test-Path $dest)) { Write-Progress -Activity $ACT -Completed; throw "Gagal mengunduh binary dari $base" }

Step 18 "Menambahkan ke PATH..."
$userPath = [Environment]::GetEnvironmentVariable("Path", "User")
if ($userPath -notlike "*$dir*") {
  [Environment]::SetEnvironmentVariable("Path", "$userPath;$dir", "User")
}
if (";$env:Path;" -notlike "*;$dir;*") { $env:Path = "$env:Path;$dir" }  # sesi ini juga, biar tak perlu window baru

# ── 2) Sidecar suara (best-effort: butuh Python + Git) ──────────────────────
$voice = $false
if ($env:VOCA_NO_VOICE -eq "1") {
  Step 20 "Melewati suara (VOCA_NO_VOICE=1)."
} else {
  $py = $null
  foreach ($c in @("python", "py")) { if (Get-Command $c -ErrorAction SilentlyContinue) { $py = $c; break } }
  $hasGit = [bool](Get-Command git -ErrorAction SilentlyContinue)

  if ($py -and $hasGit) {
    try {
      Step 25 "Menyiapkan suara: mengambil kode..."
      if (Test-Path (Join-Path $home_ ".git")) {
        git -C $home_ fetch --depth 1 origin main *>$null
        git -C $home_ reset --hard FETCH_HEAD     *>$null
      } else {
        if (Test-Path $home_) { Remove-Item -Recurse -Force $home_ }
        git clone --depth 1 "https://github.com/$repo.git" $home_ *>$null
      }
      $venv  = Join-Path $home_ ".venv"
      $pyexe = Join-Path $venv "Scripts\python.exe"

      Step 35 "Membuat virtualenv..."
      & $py -m venv $venv
      & $pyexe -m pip install --upgrade pip --quiet

      Step 50 "Memasang Whisper (dengar) + Piper (suara)..."
      & $pyexe -m pip install --quiet faster-whisper piper-tts sounddevice numpy python-dotenv

      Step 70 "Memasang VAD Silero (torch CPU, ~200MB)..."
      & $pyexe -m pip install --quiet torch torchaudio --index-url https://download.pytorch.org/whl/cpu
      & $pyexe -m pip install --quiet silero-vad

      Step 85 "Mengunduh model suara (id + en, ~120MB)..."
      $models = Join-Path $home_ "models"
      New-Item -ItemType Directory -Force -Path $models | Out-Null
      $PB = "https://huggingface.co/rhasspy/piper-voices/resolve/main"
      $files = @{
        "$PB/id/id_ID/news_tts/medium/id_ID-news_tts-medium.onnx"      = "id_ID-news_tts-medium.onnx"
        "$PB/id/id_ID/news_tts/medium/id_ID-news_tts-medium.onnx.json" = "id_ID-news_tts-medium.onnx.json"
        "$PB/en/en_US/amy/medium/en_US-amy-medium.onnx"                = "en_US-amy-medium.onnx"
        "$PB/en/en_US/amy/medium/en_US-amy-medium.onnx.json"           = "en_US-amy-medium.onnx.json"
      }
      foreach ($url in $files.Keys) { curl.exe -fsSL $url -o (Join-Path $models $files[$url]) }

      [Environment]::SetEnvironmentVariable("VOCA_VOICE_PYTHON", $pyexe, "User")
      [Environment]::SetEnvironmentVariable("VOCA_VOICE_HOME",   $home_, "User")
      $env:VOCA_VOICE_PYTHON = $pyexe; $env:VOCA_VOICE_HOME = $home_
      $voice = $true
    } catch {
      Write-Host "  ! Setup suara gagal: $($_.Exception.Message)" -ForegroundColor Yellow
      Write-Host "  ! Lanjut mode teks. Ulangi nanti: irm https://raw.githubusercontent.com/$repo/main/install-voice.ps1 | iex" -ForegroundColor Yellow
    }
  } else {
    Write-Host "  ! Python/Git belum ada -> suara dilewati (mode teks)." -ForegroundColor Yellow
    Write-Host "    Pasang Python 3.10+ (python.org, centang 'Add to PATH') & Git (git-scm.com)," -ForegroundColor Yellow
    Write-Host "    lalu: irm https://raw.githubusercontent.com/$repo/main/install-voice.ps1 | iex" -ForegroundColor Yellow
  }
}

# ── 3) API key (sekali; voca tak akan nanya lagi kalau diisi) ───────────────
Step 92 "Hampir selesai — API key."
Write-Progress -Activity $ACT -Completed
Write-Host ""
Write-Host "===========================================" -ForegroundColor Green
Write-Host (" Voca terpasang" + $(if ($voice) { " + suara siap (hands-free)" } else { " (mode teks)" })) -ForegroundColor Green
Write-Host "===========================================" -ForegroundColor Green
Write-Host "Tempel API key Qwen / DashScope (daftar gratis: https://dashscope.aliyun.com)"
$key = Read-Host "API Key (sk-...)"
if ($key -and $key.Trim()) {
  [Environment]::SetEnvironmentVariable("DASHSCOPE_API_KEY", $key.Trim(), "User")
  $env:DASHSCOPE_API_KEY = $key.Trim()
  Note "API key tersimpan."
} else {
  Note "Dilewati — voca akan meminta API key saat pertama dijalankan."
}

# ── 4) Reload di tempat (tanpa window baru) ─────────────────────────────────
Write-Host ""
Write-Host ("Ganti bahasa kapan saja: " + "/lan id" + "  atau  " + "/lan en") -ForegroundColor DarkGray
$ans = Read-Host "Tekan R lalu Enter untuk jalankan 'voca' sekarang DI SINI, atau Enter untuk keluar"
if ($ans -match '^(r|R)') {
  & $dest    # jalan di terminal yang SAMA — bukan window baru
} else {
  Write-Host "Selesai. Ketik 'voca' kapan saja (terminal ini sudah siap)." -ForegroundColor Green
}
