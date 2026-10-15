@echo off
rem Double-click to build the development app on Windows. Requires Git for Windows.
cd /d "%~dp0"
if not exist target mkdir target
set "KODEM_BASH=%ProgramFiles%\Git\bin\bash.exe"
if not exist "%KODEM_BASH%" (
    echo Install Git for Windows first: winget install Git.Git
    pause
    exit /b 1
)
rem Git Bash stays in this folder instead of opening the home folder.
set CHERE_INVOKING=1
"%KODEM_BASH%" -lc "scripts/build-windows.sh debug 2>&1 | tee target/build-debug.log; exit ${PIPESTATUS[0]}"
set result=%ERRORLEVEL%
if not "%result%"=="0" echo. & echo Debug command stopped. See target\build-debug.log for the failed step.
if not "%KODEM_NONINTERACTIVE%"=="1" pause
exit /b %result%
