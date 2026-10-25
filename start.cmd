@echo off
cd /d "%~dp0"
if not exist "dist\GuitarSmartEffecter.exe" (
  echo Build first with powershell -ExecutionPolicy Bypass -File scripts\build.ps1
  pause
  exit /b 1
)
start "" "dist\GuitarSmartEffecter.exe"
