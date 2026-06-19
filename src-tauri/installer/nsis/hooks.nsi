; ─────────────────────────────────────────────────────────────────────────
; LiteDock — NSIS installer hooks (Tauri 2 `installerHooks`).
;
; - Install: auto-set DOCKER_HOST to LiteDock's proxy (+ put the bundled docker
;   CLI on PATH if it was fetched before building). Steps shown via DetailPrint.
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
  ; Show each step in the installer's details list.
  SetDetailsPrint both
  ; Point the Windows docker CLI at LiteDock's path-translation proxy
  ; automatically (no prompt) so `docker` / `docker compose` work out of the box
  ; right after install. Port MUST match config::ENGINE_PROXY_PORT. The in-app
  ; toggle (Configuración) can remove it; uninstall clears it.
  DetailPrint "Conectando el cliente docker a LiteDock (DOCKER_HOST)…"
  WriteRegStr HKCU "Environment" "DOCKER_HOST" "tcp://127.0.0.1:23752"
  ; If the docker CLI was bundled (fetch-docker-cli.ps1 ran before building),
  ; also add it to PATH and drop in the compose plugin.
  IfFileExists "$INSTDIR\resources\docker-cli\docker.exe" 0 litedock_no_cli
    DetailPrint "Agregando docker.exe al PATH e instalando el plugin compose…"
    ReadRegStr $0 HKCU "Environment" "Path"
    WriteRegExpandStr HKCU "Environment" "Path" "$0;$INSTDIR\resources\docker-cli"
    CreateDirectory "$PROFILE\.docker\cli-plugins"
    CopyFiles /SILENT "$INSTDIR\resources\docker-cli\docker-compose.exe" "$PROFILE\.docker\cli-plugins\docker-compose.exe"
  litedock_no_cli:
  ; Notify running processes that the environment changed.
  SendMessage ${HWND_BROADCAST} ${WM_WININICHANGE} 0 "STR:Environment" /TIMEOUT=2000
  DetailPrint "Listo. Abre LiteDock para arrancar el motor."
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  ; Show each step in the uninstaller's details list (otherwise it looks blank).
  SetDetailsPrint both
  ; LiteDock runs in the background (system tray), so closing its window does NOT
  ; quit it. Force-close the app and stop the engine VM so nothing keeps running
  ; or holds files locked while uninstalling. (/T also closes the WebView2 child
  ; processes.) This is why no "close the app first" step is needed from you.
  DetailPrint "Cerrando LiteDock (incluida la bandeja del sistema)…"
  nsExec::Exec 'taskkill /F /T /IM LiteDock.exe'
  nsExec::Exec 'taskkill /F /T /IM litedock.exe'
  DetailPrint "Deteniendo el motor (WSL: litedock-engine)…"
  nsExec::Exec 'wsl.exe --terminate litedock-engine'

  ; Undo the optional docker-CLI integration we may have set up. The engine
  ; distro and its data (your containers/images/volumes) are intentionally KEPT,
  ; so reinstalling is instant and nothing is destroyed without your say-so.
  ; To wipe the engine completely:  wsl --unregister litedock-engine
  DetailPrint "Quitando la conexión de docker (DOCKER_HOST)…"
  DeleteRegValue HKCU "Environment" "DOCKER_HOST"
  Delete "$PROFILE\.docker\cli-plugins\docker-compose.exe"
  SendMessage ${HWND_BROADCAST} ${WM_WININICHANGE} 0 "STR:Environment" /TIMEOUT=2000
  DetailPrint "Se conservan el motor y tus datos (contenedores, imagenes, volumenes)."
  DetailPrint "Para borrarlos por completo: wsl --unregister litedock-engine"
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
!macroend
