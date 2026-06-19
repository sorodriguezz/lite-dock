# Usar LiteDock desde la línea de comandos

El motor de LiteDock es el **Docker Engine estándar** y publica la **API de Docker en
`tcp://127.0.0.1:23750`** (solo `127.0.0.1`). Por eso cualquier cliente `docker` es
compatible: `docker`, `docker build`, `docker compose`, `docker buildx`, etc.

El motor está disponible mientras LiteDock (la app) esté abierto, o puedes arrancarlo
por tu cuenta (ver más abajo). Cuando nada lo usa, se apaga (RAM ~0).

---

## Opción A — CLI de Docker nativo en Windows (recomendado)

Te da la experiencia `docker` / `docker compose` idéntica, con **rutas de Windows**.

1. **Instala el cliente de Docker** (solo el cliente, sin daemon):
   - scoop: `scoop install docker docker-compose`
   - choco: `choco install docker-cli docker-compose`
   - o los binarios estáticos de
     <https://download.docker.com/win/static/stable/x86_64/> (extrae `docker.exe` a una
     carpeta que esté en tu `PATH`).

2. **Apúntalo al motor de LiteDock** (PowerShell):
   - Solo esta sesión: `$env:DOCKER_HOST = "tcp://127.0.0.1:23750"`
   - Permanente: `setx DOCKER_HOST tcp://127.0.0.1:23750` (reabre la terminal)

3. **Asegúrate de que el motor está corriendo** — abre LiteDock, o:
   `.\scripts\litedock-cli.ps1 -Start`

4. **Usa Docker normalmente:**
   ```powershell
   docker ps
   docker build -t miapp C:\ruta\al\proyecto
   docker run --rm miapp
   docker compose -f C:\ruta\docker-compose.yml up -d
   docker compose -f C:\ruta\docker-compose.yml down
   ```
   Las rutas de Windows funcionan porque el cliente de Docker empaqueta el contexto de
   build localmente y lo envía por la API.

**Atajo:** `scripts\litedock-cli.ps1` arranca el motor si hace falta y te deja
`DOCKER_HOST` configurado. Para que la variable quede en TU shell, ejecútalo con
**punto** (dot-sourcing):
```powershell
. .\scripts\litedock-cli.ps1        # arranca motor + setea DOCKER_HOST en esta sesión
.\scripts\litedock-cli.ps1 -Status  # ¿está activo?
.\scripts\litedock-cli.ps1 -Stop    # apaga el motor (libera RAM)
```

---

## Opción B — Sin instalar nada (usa el CLI de la propia distro)

`docker`, `docker compose` y `buildx` ya están **dentro** de la distro `litedock-engine`.
Úsalos con `wsl` (ojo: las rutas son de WSL — tu `C:\` está en `/mnt/c/`):

```powershell
wsl -d litedock-engine -u root -- docker ps
wsl -d litedock-engine -u root -- docker build -t miapp /mnt/c/ruta/al/proyecto
wsl -d litedock-engine -u root -- docker compose -f /mnt/c/ruta/docker-compose.yml up -d
```

Más cómodo: entra a la distro y trabaja con rutas relativas:
```bash
wsl -d litedock-engine -u root
cd /mnt/c/ruta/al/proyecto
docker build -t miapp .
docker compose up -d
exit
```

---

## Arrancar / parar el motor sin la GUI

- Arrancar (primer plano, `Ctrl+C` para parar):
  `wsl -d litedock-engine -u root --exec /usr/local/bin/litedock-init.sh`
- Arrancar en segundo plano: `.\scripts\litedock-cli.ps1 -Start`
- Parar y liberar RAM: `wsl --terminate litedock-engine`

---

## Seguridad

La API está atada a `127.0.0.1` únicamente (mismo modelo de confianza que el endpoint
local de Docker Desktop). No expongas el puerto `23750` a la red.
