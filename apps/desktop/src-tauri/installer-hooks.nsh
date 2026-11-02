; NSIS hooks for Tauri's installer template, wired up by bundle.windows.nsis.installerHooks.

; Uninstalling removes everything the app created: its settings, the WebView2 cache and the
; installer's registry key. Tauri's template only does this when the user ticks "delete app data",
; which a silent uninstall never does. An update (/UPDATE) reinstalls over the top, so it keeps them.
!macro NSIS_HOOK_POSTUNINSTALL
  ${If} $UpdateMode <> 1
    SetShellVarContext current
    RmDir /r "$APPDATA\${BUNDLEID}"
    RmDir /r "$LOCALAPPDATA\${BUNDLEID}"
    DeleteRegKey HKCU "${MANUPRODUCTKEY}"
    DeleteRegKey /ifempty HKCU "${MANUKEY}"
  ${EndIf}
!macroend
