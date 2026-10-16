; Installer for one Kodem package, for the current user and without administrator rights. It
; installs the package in %LOCALAPPDATA%\Programs\<app name>, adds Start menu and desktop
; shortcuts, and registers an uninstaller under Installed apps. Settings and tasks live in the
; data folder and stay when Kodem is uninstalled.
;
; Only a first installation needs it: the app updates itself from the release archive, and its
; kodem-update helper keeps Uninstall.exe and the version shown under Installed apps.
;
; scripts/build-windows.sh compiles it with makensis and these definitions:
;   APP_NAME  Kodem, Kodem Nightly          APP_ID   dev.kodem.ade, dev.kodem.ade.nightly
;   VERSION   X.X.X                         ARCH     x86_64 or aarch64
;   PACKAGE   the package folder            OUTFILE  the installer to write
;   ICON      the app icon (.ico)
Unicode true
ManifestDPIAware true
RequestExecutionLevel user
SetCompressor /SOLID lzma

!include "MUI2.nsh"
!include "LogicLib.nsh"
!include "FileFunc.nsh"
!include "x64.nsh"

!define UNINSTALL_KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\${APP_ID}"

Name "${APP_NAME}"
OutFile "${OUTFILE}"
InstallDir "$LOCALAPPDATA\Programs\${APP_NAME}"
BrandingText "${APP_NAME} ${VERSION}"
VIProductVersion "${VERSION}.0"
VIAddVersionKey "ProductName" "${APP_NAME}"
VIAddVersionKey "ProductVersion" "${VERSION}"
VIAddVersionKey "FileVersion" "${VERSION}"
VIAddVersionKey "FileDescription" "${APP_NAME} Setup"
VIAddVersionKey "CompanyName" "Kodem"
VIAddVersionKey "LegalCopyright" "Copyright (c) 2026 Adam Wawrzynkowski"

!define MUI_ICON "${ICON}"
!define MUI_UNICON "${ICON}"
!define MUI_ABORTWARNING
!define MUI_FINISHPAGE_RUN
!define MUI_FINISHPAGE_RUN_TEXT "Open ${APP_NAME}"
!define MUI_FINISHPAGE_RUN_FUNCTION OpenApp
!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_LICENSE "${PACKAGE}\LICENSE"
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "English"

; Sets $1 when a program cannot be opened for writing, which means that it runs: Windows cannot
; replace a running program.
!macro CHECK_CLOSED FILE
    ${If} ${FileExists} "${FILE}"
        ClearErrors
        FileOpen $0 "${FILE}" a
        ${If} ${Errors}
            StrCpy $1 "running"
        ${Else}
            FileClose $0
        ${EndIf}
    ${EndIf}
!macroend

; Waits until Kodem in the installation folder has quit.
!macro WAIT_UNTIL_CLOSED PREFIX
Function ${PREFIX}WaitUntilClosed
    check:
    StrCpy $1 ""
    !insertmacro CHECK_CLOSED "$INSTDIR\kodem.exe"
    !insertmacro CHECK_CLOSED "$INSTDIR\kodem-host.exe"
    ${If} $1 != ""
        MessageBox MB_RETRYCANCEL|MB_ICONEXCLAMATION "Quit ${APP_NAME} and wait for its tasks to finish, then choose Retry." /SD IDCANCEL IDRETRY check
        Abort
    ${EndIf}
FunctionEnd
!macroend
!insertmacro WAIT_UNTIL_CLOSED ""
!insertmacro WAIT_UNTIL_CLOSED "un."

Function .onInit
    ${If} "${ARCH}" == "aarch64"
    ${AndIfNot} ${IsNativeARM64}
        MessageBox MB_ICONSTOP "This installer is for Windows on Arm. Download the x64 installer of ${APP_NAME}." /SD IDOK
        Abort
    ${EndIf}
FunctionEnd

; The app starts in the user's folder, so its working directory never keeps an update from
; moving the installation folder.
Function OpenApp
    SetOutPath "$PROFILE"
    Exec '"$INSTDIR\kodem.exe"'
FunctionEnd

Section "Install"
    SetShellVarContext current
    Call WaitUntilClosed
    ; release.json marks a Kodem package, which the app replaces as a whole when it updates.
    ${If} ${FileExists} "$INSTDIR\release.json"
        RMDir /r "$INSTDIR"
    ${EndIf}
    SetOutPath "$INSTDIR"
    File /r "${PACKAGE}\*"
    WriteUninstaller "$INSTDIR\Uninstall.exe"
    SetOutPath "$PROFILE"
    CreateShortcut "$SMPROGRAMS\${APP_NAME}.lnk" "$INSTDIR\kodem.exe"
    CreateShortcut "$DESKTOP\${APP_NAME}.lnk" "$INSTDIR\kodem.exe"
    WriteRegStr HKCU "${UNINSTALL_KEY}" "DisplayName" "${APP_NAME}"
    WriteRegStr HKCU "${UNINSTALL_KEY}" "DisplayVersion" "${VERSION}"
    WriteRegStr HKCU "${UNINSTALL_KEY}" "Publisher" "Kodem"
    WriteRegStr HKCU "${UNINSTALL_KEY}" "DisplayIcon" "$INSTDIR\kodem.exe,0"
    WriteRegStr HKCU "${UNINSTALL_KEY}" "InstallLocation" "$INSTDIR"
    WriteRegStr HKCU "${UNINSTALL_KEY}" "UninstallString" '"$INSTDIR\Uninstall.exe"'
    WriteRegStr HKCU "${UNINSTALL_KEY}" "QuietUninstallString" '"$INSTDIR\Uninstall.exe" /S'
    WriteRegDWORD HKCU "${UNINSTALL_KEY}" "NoModify" 1
    WriteRegDWORD HKCU "${UNINSTALL_KEY}" "NoRepair" 1
    ${GetSize} "$INSTDIR" "/S=0K" $0 $1 $2
    IntFmt $0 "0x%08X" $0
    WriteRegDWORD HKCU "${UNINSTALL_KEY}" "EstimatedSize" "$0"
SectionEnd

Section "Uninstall"
    SetShellVarContext current
    Call un.WaitUntilClosed
    Delete "$SMPROGRAMS\${APP_NAME}.lnk"
    Delete "$DESKTOP\${APP_NAME}.lnk"
    DeleteRegKey HKCU "${UNINSTALL_KEY}"
    ; Updates replace the folder as a whole, so everything in it is Kodem's.
    ${If} ${FileExists} "$INSTDIR\release.json"
        RMDir /r "$INSTDIR"
    ${EndIf}
SectionEnd
