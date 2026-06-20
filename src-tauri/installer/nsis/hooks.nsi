; ─────────────────────────────────────────────────────────────────────────
; LiteDock — NSIS installer hooks (Tauri 2 `installerHooks`).
;
; - Install: auto-set DOCKER_HOST to LiteDock's proxy (+ put the bundled docker
;   CLI on PATH if it was fetched before building). Steps shown via DetailPrint.
; - Uninstall: force-close the tray app + stop the engine, and undo the docker
;   CLI integration (DOCKER_HOST, the PATH entry, and the compose plugin). Then
;   it ASKS whether to also wipe EVERYTHING LiteDock created. Default is "No"
;   (keep the engine + data → instant reinstall). "Yes" leaves no residue:
;     · wsl --unregister litedock-engine        (distro + the imported VHD)
;     · %LOCALAPPDATA%\LiteDock\engine           (engine data dir)
;     · %LOCALAPPDATA%\com.litedock.desktop      (WebView2 / app cache)
;     · docker shims in your other WSL distros   (/usr/local/bin/docker, …)
;   The shared %USERPROFILE%\.wslconfig is left untouched (it may hold settings
;   for your other distros); remove its memory= line by hand if you want.
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

  ; ── Always undo the docker-CLI integration we may have set up ──
  DetailPrint "Quitando la conexión de docker (DOCKER_HOST)…"
  DeleteRegValue HKCU "Environment" "DOCKER_HOST"
  Delete "$PROFILE\.docker\cli-plugins\docker-compose.exe"
  ; Remove the docker-cli folder we appended to the user PATH on install (no-op if
  ; the all-in-one CLI was never bundled). Uses .NET inline so there are no extra
  ; PATH leftovers pointing at the now-deleted install dir.
  DetailPrint "Limpiando el PATH (docker-cli)…"
  nsExec::Exec `powershell -NoProfile -ExecutionPolicy Bypass -Command "[Environment]::SetEnvironmentVariable('Path', ([Environment]::GetEnvironmentVariable('Path','User') -replace [regex]::Escape(';$INSTDIR\resources\docker-cli'),''), 'User')"`
  SendMessage ${HWND_BROADCAST} ${WM_WININICHANGE} 0 "STR:Environment" /TIMEOUT=2000

  ; ── Ask whether to also wipe the engine distro + ALL its data ──
  ; Default (and silent-mode default via /SD) is NO, so data is never destroyed
  ; unless the user explicitly opts in. "Yes" removes the WSL distro and the
  ; imported VHD with every container/image/volume LiteDock created.
  MessageBox MB_YESNO|MB_ICONEXCLAMATION|MB_DEFBUTTON2 "¿Borrar también el motor de LiteDock y TODOS sus datos? Se eliminará la distro WSL 'litedock-engine' con todos tus contenedores, imágenes y volúmenes de LiteDock. Esto NO se puede deshacer.$\n$\nElige No para conservarlos (una reinstalación los reutiliza)." /SD IDNO IDYES litedock_wipe
    DetailPrint "Se conservan el motor y tus datos (contenedores, imagenes, volumenes)."
    DetailPrint "Para borrarlos luego: wsl --unregister litedock-engine"
    Goto litedock_done
  litedock_wipe:
    DetailPrint "Borrando el motor (wsl --unregister litedock-engine)…"
    nsExec::Exec 'wsl.exe --unregister litedock-engine'
    ; Remove the docker shims LiteDock may have installed inside your OTHER WSL
    ; distros (the Docker Desktop-style integration). Best-effort: it skips our
    ; own engine + Docker Desktop's distros and only deletes LiteDock's own paths,
    ; mirroring wsl::integration_disable(). `$$` is NSIS-escaping for a literal `$`
    ; so PowerShell — not NSIS — sees the $d / $_ variables.
    DetailPrint "Quitando los shims de docker de tus otras distros WSL…"
    nsExec::Exec `powershell -NoProfile -ExecutionPolicy Bypass -Command "[Console]::OutputEncoding=[Text.Encoding]::Unicode; (wsl.exe --list --quiet) | ForEach-Object { $$d = $$_.Trim(); if ($$d -and $$d -ne 'litedock-engine' -and $$d -ne 'docker-desktop' -and $$d -ne 'docker-desktop-data') { wsl.exe -d $$d -u root -- rm -rf /usr/local/bin/docker /usr/local/bin/docker-compose /usr/local/lib/litedock /etc/profile.d/zz-litedock.sh } }"`
    DetailPrint "Borrando los datos del motor (%LOCALAPPDATA%\LiteDock\engine)…"
    RMDir /r "$LOCALAPPDATA\LiteDock\engine"
    ; Remove the now-empty parent only if nothing else lives there (RMDir without
    ; /r is a no-op if the folder still has files, e.g. when it is the install dir).
    RMDir "$LOCALAPPDATA\LiteDock"
    ; WebView2 / app cache Tauri creates under the bundle identifier.
    DetailPrint "Borrando la caché de la app (WebView2)…"
    RMDir /r "$LOCALAPPDATA\com.litedock.desktop"
    DetailPrint "Motor, integraciones y datos de LiteDock eliminados por completo."
  litedock_done:
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
!macroend
