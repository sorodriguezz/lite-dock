<#
  fetch-docker-cli.ps1 — descarga el cliente docker + compose para empaquetarlos
  en el instalador (modo "todo-en-uno"). Ejecútalo ANTES de `npm run tauri build`.

  Si no lo ejecutas, el instalador queda liviano y simplemente no ofrece habilitar
  el CLI en la terminal.

  Uso:
    .\scripts\fetch-docker-cli.ps1
    .\scripts\fetch-docker-cli.ps1 -DockerVersion 27.3.1 -ComposeVersion 2.29.7
#>
param(
    [string]$DockerVersion = "27.3.1",
    [string]$ComposeVersion = "2.29.7"
)
$ErrorActionPreference = "Stop"

$dest = Join-Path $PSScriptRoot "..\src-tauri\resources\docker-cli"
New-Item -ItemType Directory -Force $dest | Out-Null

Write-Host "Descargando docker CLI $DockerVersion…" -ForegroundColor Cyan
$zip = Join-Path $env:TEMP "litedock-docker-cli.zip"
$tmp = Join-Path $env:TEMP "litedock-docker-cli"
Invoke-WebRequest "https://download.docker.com/win/static/stable/x86_64/docker-$DockerVersion.zip" -OutFile $zip
if (Test-Path $tmp) { Remove-Item -Recurse -Force $tmp }
Expand-Archive $zip -DestinationPath $tmp -Force
Copy-Item (Join-Path $tmp "docker\docker.exe") $dest -Force

Write-Host "Descargando docker compose $ComposeVersion…" -ForegroundColor Cyan
Invoke-WebRequest "https://github.com/docker/compose/releases/download/v$ComposeVersion/docker-compose-windows-x86_64.exe" `
    -OutFile (Join-Path $dest "docker-compose.exe")

Remove-Item -Force $zip -ErrorAction SilentlyContinue
Remove-Item -Recurse -Force $tmp -ErrorAction SilentlyContinue

Write-Host "Listo. docker.exe + docker-compose.exe en:" -ForegroundColor Green
Write-Host "  $dest"
Write-Host "Ahora `npm run tauri build` los empaquetará (instalador todo-en-uno)." -ForegroundColor Green
