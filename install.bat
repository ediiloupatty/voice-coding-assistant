@echo off
REM ===========================================================================
REM  Pemasang Voca - AI Coding Assistant (perintah: voca) untuk Windows (CMD).
REM  Mengunduh binary core (Rust) jadi — sama seperti install.ps1, tanpa Python.
REM
REM  Cara pakai (CMD), satu baris:
REM    curl -fsSL -o "%TEMP%\voca-install.bat" https://raw.githubusercontent.com/ediiloupatty/voice-coding-assistant/main/install.bat ^&^& "%TEMP%\voca-install.bat"
REM
REM  Override: VOCA_BASE_URL (sumber binary), VOCA_INSTALL_DIR (folder bin).
REM  Mode suara (STT/TTS) butuh sidecar Python — siapkan manual (lihat rust/README.md)
REM  lalu set VOCA_VOICE_PYTHON & VOCA_VOICE_HOME.
REM ===========================================================================
setlocal enabledelayedexpansion

set "REPO=ediiloupatty/voice-coding-assistant"
if not defined VOCA_BASE_URL set "VOCA_BASE_URL=https://github.com/%REPO%/releases/latest/download"
if not defined VOCA_INSTALL_DIR set "VOCA_INSTALL_DIR=%LOCALAPPDATA%\Voca"
set "ASSET=voca-windows-x64.exe"
set "DEST=%VOCA_INSTALL_DIR%\voca.exe"

echo ===========================================
echo   Memasang Voca core (perintah: voca)
echo ===========================================

REM --- 1) Prasyarat ---
where curl >nul 2>nul || (echo [ERROR] curl tidak ditemukan ^(butuh Windows 10+^). & goto :fail)

REM --- 2) Unduh binary Rust jadi dari GitHub Releases ---
if not exist "%VOCA_INSTALL_DIR%" mkdir "%VOCA_INSTALL_DIR%"
echo Mengunduh Voca core ^(%ASSET%^)...
curl -fsSL "%VOCA_BASE_URL%/%ASSET%" -o "%DEST%" || (echo [ERROR] Gagal mengunduh binary dari %VOCA_BASE_URL%/%ASSET% & goto :fail)
echo Core terpasang: %DEST%

REM --- 3) Tambahkan ke PATH (user, aman lewat PowerShell) ---
powershell -NoProfile -Command "$d='%VOCA_INSTALL_DIR%'; $p=[Environment]::GetEnvironmentVariable('PATH','User'); if ($p -notlike '*'+$d+'*') { [Environment]::SetEnvironmentVariable('PATH', $p+';'+$d, 'User') }"

echo.
echo ===========================================
echo  Selesai terpasang di %VOCA_INSTALL_DIR%
echo ===========================================
echo  Jalankan:  voca
echo    ^(API key diminta otomatis saat pertama dijalankan^)
echo  Mode suara: siapkan sidecar Python manual ^(lihat rust\README.md^),
echo    lalu set VOCA_VOICE_PYTHON ^& VOCA_VOICE_HOME.
echo.

REM --- 4) Tawarkan buka terminal baru (PATH ter-refresh) biar 'voca' langsung jalan ---
choice /c RK /n /m "Tekan [R] buka terminal baru ^& pakai voca sekarang, atau [K] keluar: "
if errorlevel 2 goto :selesai
start "Voca" cmd /k "set PATH=%VOCA_INSTALL_DIR%;%PATH% & cls & echo Voca siap dipakai. Ketik:  voca & echo."

:selesai
endlocal
exit /b 0

:fail
echo.
echo Instalasi GAGAL. Perbaiki error di atas lalu jalankan ulang.
endlocal
exit /b 1
