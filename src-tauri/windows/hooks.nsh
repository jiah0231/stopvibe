!macro NSIS_HOOK_POSTINSTALL
  ExecWait '"$INSTDIR\stopvibe-service.exe" --install' $0
  StrCmp $0 0 stopvibe_postinstall_done stopvibe_postinstall_failed
  stopvibe_postinstall_failed:
    MessageBox MB_ICONSTOP "StopVibe service installation failed. The app cannot block targets until the service is repaired."
    Abort
  stopvibe_postinstall_done:
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  ExecWait '"$INSTDIR\stopvibe-service.exe" --uninstall' $0
  StrCmp $0 0 stopvibe_preuninstall_done stopvibe_preuninstall_failed
  stopvibe_preuninstall_failed:
    MessageBox MB_ICONSTOP "StopVibe is currently blocking. Uninstall is disabled until the active timer ends."
    Abort
  stopvibe_preuninstall_done:
!macroend
