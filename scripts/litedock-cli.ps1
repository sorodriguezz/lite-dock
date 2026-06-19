<#
  litedock-cli.ps1 — usa el motor Docker de LiteDock desde la línea de comandos.

  El motor de LiteDock expone la API estándar de Docker en tcp://127.0.0.1:23750.
  Este script lo arranca si hace falta y deja DOCKER_HOST apuntando a él, para que
  uses `docker` / `docker compose` de Windows tal cual.

  Uso:
    . .\scripts\litedock-cli.ps1        # (con punto) arranca el motor + setea DOCKER_HOST en ESTA sesión
    .\scripts\litedock-cli.ps1 -Start   # arranca el motor en segundo plano
    .\scripts\litedock-cli.ps1 -Stop    # detiene el motor (libera RAM)
    .\scripts\litedock-cli.ps1 -Status  # muestra si está activo
#>
[CmdletBinding()]
param(
    [switch]$Start,
    [switch]$Stop,
    [switch]$Status
)

$Distro   = "litedock-engine"
$Endpoint = "tcp://127.0.0.1:23750"
$PingUrl  = "http://127.0.0.1:23750/_ping"

function Test-Engine {
    try {
        return (Invoke-WebRequest -UseBasicParsing -Uri $PingUrl -TimeoutSec 2).StatusCode -eq 200
    } catch {
        return $false
    }
}

if ($Stop) {
    wsl --terminate $Distro 2>$null | Out-Null
    Write-Host "Motor detenido (RAM liberada)." -ForegroundColor Yellow
    return
}

if ($Status) {
    if (Test-Engine) {
        Write-Host "Motor ACTIVO en $Endpoint" -ForegroundColor Green
    } else {
        Write-Host "Motor detenido." -ForegroundColor DarkGray
    }
    return
}

# Por defecto (o -Start): asegurar que el motor está arriba.
if (-not (Test-Engine)) {
    Write-Host "Arrancando el motor de LiteDock..." -ForegroundColor Cyan
    Start-Process -WindowStyle Hidden -FilePath "wsl.exe" `
        -ArgumentList @("-d", $Distro, "-u", "root", "--exec", "/usr/local/bin/litedock-init.sh")
    for ($i = 0; $i -lt 40; $i++) {
        if (Test-Engine) { break }
        Start-Sleep -Milliseconds 700
    }
}

if (-not (Test-Engine)) {
    Write-Host "No se pudo arrancar el motor. Abre LiteDock o revisa WSL (wsl -l -v)." -ForegroundColor Red
    return
}

$env:DOCKER_HOST = $Endpoint
Write-Host "Motor ACTIVO. DOCKER_HOST=$Endpoint (en esta sesión)." -ForegroundColor Green
Write-Host "Ya puedes usar:  docker ps  |  docker build  |  docker compose up" -ForegroundColor Green

if (-not (Get-Command docker -ErrorAction SilentlyContinue)) {
    Write-Host ""
    Write-Host "Aviso: no encuentro 'docker' en el PATH. Instala el cliente con:" -ForegroundColor Yellow
    Write-Host "    scoop install docker docker-compose      (o)" -ForegroundColor Yellow
    Write-Host "    choco install docker-cli docker-compose" -ForegroundColor Yellow
    Write-Host "...o usa el de la distro:  wsl -d litedock-engine -u root -- docker ps" -ForegroundColor Yellow
}
