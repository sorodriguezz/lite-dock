; ─────────────────────────────────────────────────────────────────────────
; LiteDock — NSIS installer hooks (Tauri 2 `installerHooks`).
;
; - Install: optionally enable the bundled `docker` CLI in the terminal (only
;   offered if you ran scripts/fetch-docker-cli.ps1 before building).
; - Uninstall: force-close the tray app + stop the engine, and undo the docker
;   CLI integration. The engine distro and its data are KEPT (remove manually
;   with `wsl --unregister litedock-engine` if you want a full wipe).
;
; The heavy WSL2 enablement + engine import happens in the in-app First-Run
; Wizard, not here.
; ─────────────────────────────────────────────────────────────────────────

!ifndef WM_WININICHANGE
  !define WM_WININICHANGE 0x001A
!endif
!ifndef HWND_BROADCAST
  !define HWND_BROADCAST 0xFFFF
!endif

!macro NSIS_HOOK_PREINSTALL
!macroend

!macro NSIS_HOOK_POSTINSTALL
  ; Optional convenience, only offered when the docker CLI was bundled
  ; (fetch-docker-cli.ps1 ran before building). The in-app "Habilitar comandos
  ; docker" button does the same on demand, so this is just a shortcut.
  IfFileExists "$INSTDIR\resources\docker-cli\docker.exe" 0 litedock_no_cli
    MessageBox MB_YESNO|MB_ICONQUESTION \
      "¿Habilitar los comandos 'docker' y 'docker compose' en la terminal? Se agregan al PATH y se conectan a LiteDock." \
      IDNO litedock_no_cli
      ; Point the standard docker CLI at LiteDock's path-translation proxy.
      ; Port MUST match config::ENGINE_PROXY_PORT.
      WriteRegStr HKCU "Environment" "DOCKER_HOST" "tcp://127.0.0.1:23752"
      ; Add the bundled CLI folder to the user PATH.
      ReadRegStr $0 HKCU "Environment" "Path"
      WriteRegExpandStr HKCU "Environment" "Path" "$0;$INSTDIR\resources\docker-cli"
      ; Install the compose plugin where the docker CLI looks for it.
      CreateDirectory "$PROFILE\.docker\cli-plugins"
      CopyFiles /SILENT "$INSTDIR\resources\docker-cli\docker-compose.exe" "$PROFILE\.docker\cli-plugins\docker-compose.exe"
      ; Notify running processes that the environment changed.
      SendMessage ${HWND_BROADCAST} ${WM_WININICHANGE} 0 "STR:Environment" /TIMEOUT=2000
  litedock_no_cli:
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  ; LiteDock runs in the background (system tray), so closing its window does NOT
  ; quit it. Force-close the app and stop the engine VM so nothing keeps running
  ; or holds files locked while uninstalling. (/T also closes the WebView2 child
  ; processes.) This is why no "close the app first" step is needed from you.
  nsExec::Exec 'taskkill /F /T /IM LiteDock.exe'
  nsExec::Exec 'taskkill /F /T /IM litedock.exe'
  nsExec::Exec 'wsl.exe --terminate litedock-engine'

  ; Undo the optional docker-CLI integration we may have set up. The engine
  ; distro and its data (your containers/images/volumes) are intentionally KEPT,
  ; so reinstalling is instant and nothing is destroyed without your say-so.
  ; To wipe the engine completely:  wsl --unregister litedock-engine
  DeleteRegValue HKCU "Environment" "DOCKER_HOST"
  Delete "$PROFILE\.docker\cli-plugins\docker-compose.exe"
  SendMessage ${HWND_BROADCAST} ${WM_WININICHANGE} 0 "STR:Environment" /TIMEOUT=2000
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
!macroend
