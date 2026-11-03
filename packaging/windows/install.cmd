@echo off
rem AIQ install for Windows: double-click this file. No admin rights.
rem Runs install.ps1 next to it with the script policy relaxed for this one run only, so an
rem unsigned, downloaded install.ps1 is not refused. Nothing about your system's policy changes.
rem A company Group Policy that requires signed scripts still wins; then tell the alpha owner.
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0install.ps1" %*
set AIQ_RC=%ERRORLEVEL%
echo.
if "%AIQ_RC%"=="0" (echo AIQ is installed. You can close this window.) else (echo AIQ install stopped with code %AIQ_RC%. The lines above say why.)
if not defined AIQ_NO_PAUSE pause
exit /b %AIQ_RC%
