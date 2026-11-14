; Halo Island installer (NSIS 3; kolijijani's fork of Halo Island). Per-user install, no admin rights needed.
;
; The name shown to people is "Halo Island"; the install folder, the uninstall key, the Run value and halo.exe
; use the name "Halo".
;
; Build (Linux/macOS/Windows with makensis):
;   makensis -DEXE=/abs/path/halo.exe -DOUTDIR=/abs/path/dist installer/halo.nsi
; tools/package.sh does this for you.

; Native 64-bit installer (Halo itself is x64-only).
Target amd64-unicode
SetCompressor /SOLID lzma

!define APPNAME "Halo Island"
!define VERSION "0.2.40"
!define PUBLISHER "kolijijani"
!define UNINST_KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\Halo"
!define RUN_KEY "Software\Microsoft\Windows\CurrentVersion\Run"

!ifndef EXE
  !error "Pass -DEXE=<path to halo.exe>"
!endif
!ifndef OUTDIR
  !define OUTDIR "."
!endif

Name "${APPNAME}"
OutFile "${OUTDIR}/Halo-Island-Setup-${VERSION}.exe"
RequestExecutionLevel user
InstallDir "$LOCALAPPDATA\Programs\Halo"
InstallDirRegKey HKCU "Software\Halo" "InstallDir"
BrandingText "${APPNAME} ${VERSION}"

VIProductVersion "0.2.40.0"
VIAddVersionKey "ProductName" "${APPNAME}"
VIAddVersionKey "CompanyName" "${PUBLISHER}"
VIAddVersionKey "FileDescription" "Halo Island installer"
VIAddVersionKey "FileVersion" "${VERSION}"
VIAddVersionKey "ProductVersion" "${VERSION}"
VIAddVersionKey "LegalCopyright" "Copyright (C) 2026"

!include "MUI2.nsh"
!include "LogicLib.nsh"
!include "WinMessages.nsh"
!include "FileFunc.nsh"
!include "Sections.nsh"

; Ask a running Halo to quit (it saves its data on WM_CLOSE), wait up to 3 s,
; then force-close with the fully qualified system taskkill.
!macro StopHalo
  FindWindow $0 "Halo.Island"
  ${If} $0 <> 0
    SendMessage $0 ${WM_CLOSE} 0 0 /TIMEOUT=2000
    StrCpy $1 0
    ${DoWhile} $0 <> 0
      ${If} $1 >= 30
        ${Break}
      ${EndIf}
      Sleep 100
      FindWindow $0 "Halo.Island"
      IntOp $1 $1 + 1
    ${Loop}
  ${EndIf}
  nsExec::Exec '"$SYSDIR\taskkill.exe" /IM halo.exe /F'
  Sleep 200
!macroend
!define MUI_ICON "../crates/halo/res/halo.ico"
!define MUI_UNICON "../crates/halo/res/halo.ico"
; Halo hanging from the island on the welcome and finish pages, Halo's logo in the header (tools/make_brand.py).
!define MUI_WELCOMEFINISHPAGE_BITMAP "art/nsis-welcome.bmp"
!define MUI_HEADERIMAGE
!define MUI_HEADERIMAGE_RIGHT
!define MUI_HEADERIMAGE_BITMAP "art/nsis-header.bmp"
!define MUI_ABORTWARNING
!define MUI_COMPONENTSPAGE_SMALLDESC
!define MUI_FINISHPAGE_RUN "$INSTDIR\halo.exe"
!define MUI_FINISHPAGE_RUN_TEXT "Launch Halo Island now"
!define MUI_WELCOMEPAGE_TITLE "Welcome to Halo Island"
!define MUI_WELCOMEPAGE_TEXT "Halo Island puts a living notch at the top of your screen: music with synced lyrics, your day, a focus timer, your steps, a file shelf, tasks and more.$\r$\n$\r$\nIt installs just for you and doesn't need administrator rights."

!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_COMPONENTS
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "English"

Section "Halo Island" SecMain
  SectionIn RO
  ; Close a running copy so the exe can be replaced.
  !insertmacro StopHalo
  SetOutPath "$INSTDIR"
  File "/oname=halo.exe" "${EXE}"
  ; The licenses of what halo.exe is built from (mecab-ipadic asks for its notice in every copy).
  File "../THIRD-PARTY-NOTICES.txt"
  WriteUninstaller "$INSTDIR\Uninstall.exe"
  Delete "$SMPROGRAMS\Halo.lnk"
  Delete "$SMPROGRAMS\Halo Island.lnk" ; old upstream shortcut
  CreateShortcut "$SMPROGRAMS\${APPNAME}.lnk" "$INSTDIR\halo.exe" "" "$INSTDIR\halo.exe" 0
  WriteRegStr HKCU "Software\Halo" "InstallDir" "$INSTDIR"
  WriteRegStr HKCU "${UNINST_KEY}" "DisplayName" "${APPNAME}"
  WriteRegStr HKCU "${UNINST_KEY}" "DisplayVersion" "${VERSION}"
  WriteRegStr HKCU "${UNINST_KEY}" "Publisher" "${PUBLISHER}"
  WriteRegStr HKCU "${UNINST_KEY}" "DisplayIcon" "$INSTDIR\halo.exe"
  WriteRegStr HKCU "${UNINST_KEY}" "InstallLocation" "$INSTDIR"
  WriteRegStr HKCU "${UNINST_KEY}" "UninstallString" '"$INSTDIR\Uninstall.exe"'
  WriteRegStr HKCU "${UNINST_KEY}" "QuietUninstallString" '"$INSTDIR\Uninstall.exe" /S'
  WriteRegDWORD HKCU "${UNINST_KEY}" "NoModify" 1
  WriteRegDWORD HKCU "${UNINST_KEY}" "NoRepair" 1
  WriteRegDWORD HKCU "${UNINST_KEY}" "EstimatedSize" 7680
SectionEnd

Section "Start Halo Island with Windows" SecAutostart
  ; Same value the app's own "Start with Windows" toggle writes.
  WriteRegStr HKCU "${RUN_KEY}" "Halo" '"$INSTDIR\halo.exe"'
SectionEnd

; The app's own update runs this Setup silently: `/S /reopen` (crates/halo/src/services/update.rs).
Function .onInit
  ; Silent: Start with Windows stays as the person set it (the Run value already points here).
  ${If} ${Silent}
    !insertmacro UnselectSection ${SecAutostart}
  ${EndIf}
FunctionEnd

Function .onInstSuccess
  ; The finish page's "Launch Halo Island now" never shows in silent mode: /reopen starts it.
  ${If} ${Silent}
    ${GetParameters} $R0
    ClearErrors
    ${GetOptions} $R0 "/reopen" $R1
    ${IfNot} ${Errors}
      Exec '"$INSTDIR\halo.exe"'
    ${EndIf}
  ${EndIf}
FunctionEnd

!insertmacro MUI_FUNCTION_DESCRIPTION_BEGIN
  !insertmacro MUI_DESCRIPTION_TEXT ${SecMain} "The Halo Island app and a Start Menu shortcut."
  !insertmacro MUI_DESCRIPTION_TEXT ${SecAutostart} "Open Halo Island automatically when you sign in. You can change this later from its menu."
!insertmacro MUI_FUNCTION_DESCRIPTION_END

Section "Uninstall"
  !insertmacro StopHalo
  Delete "$INSTDIR\halo.exe"
  Delete "$INSTDIR\THIRD-PARTY-NOTICES.txt"
  Delete "$INSTDIR\Uninstall.exe"
  RMDir "$INSTDIR"
  Delete "$SMPROGRAMS\${APPNAME}.lnk"
  Delete "$SMPROGRAMS\Halo.lnk"
  Delete "$SMPROGRAMS\Halo Island.lnk" ; old upstream shortcut
  DeleteRegValue HKCU "${RUN_KEY}" "Halo"
  DeleteRegKey HKCU "${UNINST_KEY}"
  DeleteRegKey HKCU "Software\Halo"
  ; Cache and log (lyrics lookups name the songs you played) go too.
  RMDir /r "$LOCALAPPDATA\Halo"
  ; Your settings and data (tasks, shelf, focus history) in %APPDATA%\Halo are kept.
SectionEnd
