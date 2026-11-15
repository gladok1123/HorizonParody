@echo off
rem Starts Horizon from this folder; PLAY.txt says what the first run needs.
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0play.ps1" %*
set code=%errorlevel%
if not "%code%"=="0" pause
exit /b %code%
