@echo off
rem Double-click to install this Kodem package for the current user.
powershell.exe -NoLogo -NoProfile -ExecutionPolicy Bypass -File "%~dp0install.ps1" %*
set result=%ERRORLEVEL%
pause
exit /b %result%
