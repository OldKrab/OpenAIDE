!macro NSIS_HOOK_PREINSTALL
  InitPluginsDir
  SetOutPath "$PLUGINSDIR"
  File /oname=openaide-stop-installed-runtime.ps1 "${__FILEDIR__}\stop-installed-runtime.ps1"
  SetOutPath "$INSTDIR"

  nsExec::ExecToStack '"$SYSDIR\WindowsPowerShell\v1.0\powershell.exe" -NoLogo -NoProfile -NonInteractive -ExecutionPolicy Bypass -File "$PLUGINSDIR\openaide-stop-installed-runtime.ps1" -InstallRoot "$INSTDIR"'
  Pop $0
  Pop $1
  ${If} $0 != 0
    DetailPrint "OpenAIDE installed runtime shutdown failed."
    Abort "OpenAIDE could not stop its installed App Server. Close OpenAIDE and retry."
  ${EndIf}
!macroend
