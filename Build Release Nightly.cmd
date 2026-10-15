@echo off
rem Double-click to release Nightly for every platform through GitHub Actions. Requires Git for Windows.
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
"%KODEM_BASH%" -lc "scripts/release.sh nightly 2>&1 | tee target/build-nightly.log; exit ${PIPESTATUS[0]}"
set result=%ERRORLEVEL%
if not "%result%"=="0" echo. & echo Nightly command stopped. See target\build-nightly.log for the failed step.
if not "%KODEM_NONINTERACTIVE%"=="1" pause
exit /b %result%
