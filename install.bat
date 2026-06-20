@echo off
REM ===========================================================================
REM  install.bat — pemasang Voca (Windows / CMD).
REM
REM  Hanya bootstrap: menjalankan installer PENUH (install.ps1) di console yang
REM  SAMA — binary + suara + bahasa + model, bar progres 1-100%, lalu minta API
REM  key dan jalankan 'voca' DI SINI (tanpa membuka window baru).
REM
REM  Pakai (satu baris di CMD):
REM    curl -fsSL -o "%TEMP%\voca-install.bat" https://raw.githubusercontent.com/ediiloupatty/voice-coding-assistant/main/install.bat ^&^& "%TEMP%\voca-install.bat"
REM
REM  Override diteruskan otomatis lewat env: VOCA_BASE_URL, VOCA_INSTALL_DIR,
REM  VOCA_HOME, VOCA_NO_VOICE (=1 untuk lewati suara).
REM ===========================================================================
setlocal
set "REPO=ediiloupatty/voice-coding-assistant"

where powershell >nul 2>nul || (echo [ERROR] PowerShell tak ditemukan ^(butuh Windows 10+^). & exit /b 1)

powershell -NoProfile -ExecutionPolicy Bypass -Command "irm https://raw.githubusercontent.com/%REPO%/main/install.ps1 | iex"

endlocal
