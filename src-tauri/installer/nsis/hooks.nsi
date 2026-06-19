; ─────────────────────────────────────────────────────────────────────────
; LiteDock — NSIS installer hooks (Tauri 2 `installerHooks`).
;
; - Install: optionally enable the bundled `docker` CLI in the terminal (only
;   offered if you ran scripts/fetch-docker-cli.ps1 before building).
; - Uninstall: optionally remove the dedicated WSL engine distro + data.
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
  ; Only offered when the docker CLI was bundled (fetch-docker-cli.ps1 was run).
  IfFileExists "$INSTDIR\resources\docker-cli\docker.exe" 0 litedock_no_cli
    MessageBox MB_YESNO|MB_ICONQUESTION \
      "¿Habilitar los comandos 'docker' y 'docker compose' en la terminal? Se agregan al PATH y se conectan a LiteDock." \
      IDNO litedock_no_cli
      ; Point the standard docker CLI at LiteDock's engine.
      WriteRegStr HKCU "Environment" "DOCKER_HOST" "tcp://127.0.0.1:23750"
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
  ; Offer to remove the dedicated engine distro and its data (destroys the
  ; LiteDock containers/images/volumes — but not the user's other distros).
  MessageBox MB_YESNO|MB_ICONQUESTION \
    "¿Eliminar también el motor de LiteDock (WSL: litedock-engine) y todos sus contenedores, imágenes y volúmenes?" \
    IDNO litedock_keep_engine
    DetailPrint "Eliminando el motor litedock-engine…"
    nsExec::Exec 'wsl.exe --terminate litedock-engine'
    nsExec::Exec 'wsl.exe --unregister litedock-engine'
    RMDir /r "$LOCALAPPDATA\LiteDock"
  litedock_keep_engine:
  ; Clean up the DOCKER_HOST we may have set.
  DeleteRegValue HKCU "Environment" "DOCKER_HOST"
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
!macroend
