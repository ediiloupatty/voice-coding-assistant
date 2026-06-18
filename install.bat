@echo off
REM ===========================================================================
REM  Pemasang AI Coding Companion (perintah: kong) untuk Windows
REM
REM  Cara pakai (CMD): unduh lalu jalankan file ini, atau satu baris:
REM    curl -fsSL -o "%TEMP%\kong-install.bat" https://raw.githubusercontent.com/ediiloupatty/AI-AGENT/main/install.bat ^&^& "%TEMP%\kong-install.bat"
REM ===========================================================================
setlocal enabledelayedexpansion

set "REPO=https://github.com/ediiloupatty/voice-coding-assistant.git"
set "INSTALL_DIR=%USERPROFILE%\.voca"
set "BIN_DIR=%INSTALL_DIR%\bin"
set "MODEL_BASE=https://huggingface.co/rhasspy/piper-voices/resolve/main/id/id_ID/news_tts/medium"
set "MODEL=id_ID-news_tts-medium"
set "MODEL_EN_BASE=https://huggingface.co/rhasspy/piper-voices/resolve/main/en/en_US/amy/medium"
set "MODEL_EN=en_US-amy-medium"

echo ===========================================
echo   Memasang Voca (voca)
echo ===========================================

REM --- 1) Prasyarat wajib ---
where python >nul 2>nul || (echo [ERROR] Python belum terpasang. Pasang dari python.org. & exit /b 1)
where git    >nul 2>nul || (echo [ERROR] Git belum terpasang. Pasang dari git-scm.com. & exit /b 1)
where curl   >nul 2>nul || (echo [ERROR] curl tidak ditemukan (butuh Windows 10+). & exit /b 1)
where ffmpeg >nul 2>nul || echo [WARN] ffmpeg belum ada - pitch-shift suara dimatikan ^(suara tetap jalan^).

REM --- 2) Unduh / perbarui kode ---
if exist "%INSTALL_DIR%\.git" (
  echo Memperbarui kode...
  git -C "%INSTALL_DIR%" pull --ff-only
) else (
  echo Mengunduh kode ke %INSTALL_DIR% ...
  if exist "%INSTALL_DIR%" rmdir /s /q "%INSTALL_DIR%"
  git clone --depth 1 "%REPO%" "%INSTALL_DIR%" || (echo [ERROR] Gagal clone repo. & exit /b 1)
)

REM --- 3) Virtualenv + dependensi ---
echo Menyiapkan virtualenv dan dependensi ^(bisa beberapa menit^)...
python -m venv "%INSTALL_DIR%\.venv"
"%INSTALL_DIR%\.venv\Scripts\python.exe" -m pip install -q --upgrade pip
"%INSTALL_DIR%\.venv\Scripts\python.exe" -m pip install -q -r "%INSTALL_DIR%\requirements.txt"

REM --- 4) Model suara Piper (Indonesia + English) ---
echo Mengunduh model suara Piper Indonesia ^(~60MB^)...
if not exist "%INSTALL_DIR%\models" mkdir "%INSTALL_DIR%\models"
curl -fsSL "%MODEL_BASE%/%MODEL%.onnx"      -o "%INSTALL_DIR%\models\%MODEL%.onnx"
curl -fsSL "%MODEL_BASE%/%MODEL%.onnx.json" -o "%INSTALL_DIR%\models\%MODEL%.onnx.json"
echo Mengunduh model suara Piper English ^(~60MB^)...
curl -fsSL "%MODEL_EN_BASE%/%MODEL_EN%.onnx"      -o "%INSTALL_DIR%\models\%MODEL_EN%.onnx"
curl -fsSL "%MODEL_EN_BASE%/%MODEL_EN%.onnx.json" -o "%INSTALL_DIR%\models\%MODEL_EN%.onnx.json"

REM --- 5) Siapkan .env ---
if not exist "%INSTALL_DIR%\.env" copy "%INSTALL_DIR%\.env.example" "%INSTALL_DIR%\.env" >nul

REM --- 6) Buat perintah 'voca' ---
echo Membuat perintah 'voca'...
if not exist "%BIN_DIR%" mkdir "%BIN_DIR%"
> "%BIN_DIR%\voca.cmd" echo @echo off
>> "%BIN_DIR%\voca.cmd" echo set "PYTHONPATH=%INSTALL_DIR%"
>> "%BIN_DIR%\voca.cmd" echo "%INSTALL_DIR%\.venv\Scripts\python.exe" -m voca %%*

REM --- 7) Tambahkan ke PATH (user, aman lewat PowerShell) ---
powershell -NoProfile -Command "$b='%BIN_DIR%'; $p=[Environment]::GetEnvironmentVariable('PATH','User'); if ($p -notlike '*'+$b+'*') { [Environment]::SetEnvironmentVariable('PATH', $p+';'+$b, 'User') }"

echo.
echo ===========================================
echo  Selesai terpasang di %INSTALL_DIR%
echo ===========================================
echo Langkah terakhir:
echo   1^) Isi API key Qwen di:  %INSTALL_DIR%\.env   ^(DASHSCOPE_API_KEY=sk-xxxx^)
echo   2^) BUKA CMD/Terminal BARU ^(biar PATH ter-refresh^)
echo   3^) Jalankan:  voca        ^(mode hands-free^)
echo              atau voca --text  ^(mode teks murni^)
endlocal
