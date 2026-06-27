; NSIS installer hooks for ZNet Sink.
;
; Injected into the NSIS installer template via tauri.conf.json
; (bundle.windows.nsis.installerHooks).
;
; On uninstall, prompt the user whether to also delete the app's data
; directory (configs, subscriptions, logs, kernel exports). The default
; is No — the user must explicitly opt in. This keeps config across a
; reinstall by default while still letting the user wipe everything.
;
; IMPORTANT — update path:
;   The Tauri NSIS installer silently runs the previous version's
;   uninstaller (with /S) during an in-place update. IfSilent short-
;   circuits BOTH the prompt and the deletion in that case, so auto-update
;   never touches the user's config and never pops a dialog.
;
; Why a MessageBox instead of a checkbox on the confirm page:
;   Tauri's installerHooks only inject macro bodies — they cannot add
;   UninstPage UI. A real checkbox would require shipping a full custom
;   installer.nsi template (~800 LOC) out of tree. The Yes/No prompt is
;   the pragmatic equivalent and lives entirely in this hook file.

!include "LogicLib.nsh"

Var DeleteAppData

!macro NSIS_HOOK_PREUNINSTALL
  ; Default: keep data. Silent (update) path skips both prompt and delete.
  StrCpy $DeleteAppData 0
  IfSilent skip_appdata_prompt
  MessageBox MB_YESNO|MB_DEFBUTTON2|MB_ICONQUESTION "是否同时删除配置数据（订阅、配置、日志等）？$\r$\n$\r$\n不删除则下次安装将沿用当前配置。" IDNO skip_appdata_prompt
  StrCpy $DeleteAppData 1
  skip_appdata_prompt:
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  ${If} $DeleteAppData == 1
    RMDir /r "$APPDATA\ZNet Sink"
  ${EndIf}
!macroend
