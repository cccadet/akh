!include "WinMessages.nsh"
!include "WordFunc.nsh"
!insertmacro WordAdd
!insertmacro un.WordAdd

!macro NSIS_HOOK_POSTINSTALL
  ReadRegStr $0 HKCU "Environment" "Path"
  ${WordAdd} "$0" ";" "+$INSTDIR" $1
  WriteRegExpandStr HKCU "Environment" "Path" "$1"
  SendMessage ${HWND_BROADCAST} ${WM_SETTINGCHANGE} 0 "STR:Environment" /TIMEOUT=5000
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  ReadRegStr $0 HKCU "Environment" "Path"
  ${un.WordAdd} "$0" ";" "-$INSTDIR" $1
  WriteRegExpandStr HKCU "Environment" "Path" "$1"
  SendMessage ${HWND_BROADCAST} ${WM_SETTINGCHANGE} 0 "STR:Environment" /TIMEOUT=5000
!macroend
