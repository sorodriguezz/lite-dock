# Usar LiteDock desde la línea de comandos

El motor de LiteDock es el **Docker Engine estándar**, así que cualquier cliente `docker`
es compatible: `docker`, `docker build`, `docker compose`, `docker buildx`, etc.

LiteDock publica **dos endpoints** locales (solo `127.0.0.1`, nunca a la red):

- **`tcp://127.0.0.1:23752` — proxy (recomendado).** Reenvía al motor y, además, **traduce
  las rutas de Windows** de los bind mounts (`C:\…` → `/mnt/c/…`). Así un `docker compose`
  con volúmenes hacia carpetas de Windows funciona igual que con Docker Desktop, **sin tocar
  el compose**. Es lo que LiteDock pone en `DOCKER_HOST` al pulsar **"Habilitar comandos
  docker"** en Configuración. El proxy vive dentro de la app, así que requiere que LiteDock
  esté abierto (la bandeja cuenta).
- **`tcp://127.0.0.1:23750` — motor directo (sin traducción).** Habla con `dockerd` tal cual.
  Funciona aunque la app esté cerrada (p. ej. arrancando el motor con el script de abajo),
  pero los bind mounts a rutas `C:\…` **no** se traducen aquí: usa rutas `/mnt/c/…`.

El motor se apaga cuando nada lo usa (RAM ~0).

---

## Opción A — CLI de Docker nativo en Windows (recomendado)

1. **Instala el cliente de Docker** (solo el cliente, sin daemon):
   - scoop: `scoop install docker docker-compose`
   - choco: `choco install docker-cli docker-compose`
   - o los binarios estáticos de
     <https://download.docker.com/win/static/stable/x86_64/> (extrae `docker.exe` a una
     carpeta que esté en tu `PATH`).
   - Si instalaste LiteDock con el instalador "todo-en-uno", ya trae `docker` y
     `docker compose` empaquetados.

2. **Apúntalo al motor de LiteDock.** Lo más fácil: en LiteDock →
   **Configuración → "Habilitar comandos docker"**. Eso fija `DOCKER_HOST` al proxy
   automáticamente (traducción de rutas incluida).
   - Manual, solo esta sesión: `$env:DOCKER_HOST = "tcp://127.0.0.1:23752"`
   - Manual, permanente: `setx DOCKER_HOST tcp://127.0.0.1:23752` (reabre la terminal)

   > El cambio de `DOCKER_HOST` solo aplica a terminales **abiertas después**. Si ya tenías
   > una terminal o tu IDE abiertos, **ábrelos de nuevo** (o reinicia el IDE); si no, seguirán
   > apuntando a donde sea que estuvieran antes.

3. **Verifica la conexión** — debe responder con el motor, no con un error de *pipe*:
   ```powershell
   docker info --format '{{.Name}} {{.ServerVersion}}'
   ```
   Si ves algo como `open //./pipe/dockerDesktopLinuxEngine: ...`, es que `DOCKER_HOST`
   **no** está puesto en esta sesión (terminal vieja) → abre una nueva o fija la variable.

4. **Asegúrate de que el motor está corriendo** — abre LiteDock, o:
   `.\scripts\litedock-cli.ps1 -Start`

5. **Usa Docker normalmente:**
   ```powershell
   docker ps
   docker build -t miapp C:\ruta\al\proyecto
   docker run --rm miapp
   docker compose -f C:\ruta\docker-compose.yml up -d
   docker compose -f C:\ruta\docker-compose.yml down
   ```
   Con el proxy, **las rutas de Windows en los bind mounts funcionan** (LiteDock las traduce
   a `/mnt/c/…`). El contexto de `docker build` también, porque el cliente lo empaqueta y lo
   envía por la API.

**Atajo:** `scripts\litedock-cli.ps1` arranca el motor si hace falta y deja `DOCKER_HOST`
configurado. Para que la variable quede en TU shell, ejecútalo con **punto** (dot-sourcing):
```powershell
. .\scripts\litedock-cli.ps1        # arranca motor + setea DOCKER_HOST en esta sesión
.\scripts\litedock-cli.ps1 -Status  # ¿está activo?
.\scripts\litedock-cli.ps1 -Stop    # apaga el motor (libera RAM)
```
> Nota: este script apunta al motor **directo** (`:23750`), porque puede arrancar el motor
> sin la app y el proxy vive dentro de LiteDock. Si necesitas la traducción de rutas de
> Windows, abre LiteDock y usa el proxy (`:23752`), o pon las rutas en formato `/mnt/c/…`.

---

## Opción B — Sin instalar nada (usa el CLI de la propia distro)

`docker`, `docker compose` y `buildx` ya están **dentro** de la distro `litedock-engine`.
Úsalos con `wsl` (las rutas son de WSL — tu `C:\` está en `/mnt/c/`):

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

## Solución de problemas

- **`open //./pipe/dockerDesktopLinuxEngine: ...`** → `DOCKER_HOST` no apunta a LiteDock en
  esa terminal (típico con terminales o IDEs abiertos antes de habilitar el CLI). Abre una
  terminal nueva, o fija la variable: `$env:DOCKER_HOST = "tcp://127.0.0.1:23752"`.
- **Un bind mount falla con `no such file or directory` y una ruta `C:\…`** → estás usando el
  endpoint **directo** (`:23750`), que no traduce rutas, o LiteDock no está abierto (el proxy
  no corre). Abre LiteDock y usa el proxy (`:23752`), o pon la ruta en formato `/mnt/c/…`.
- **Diagnóstico directo al motor (sin proxy):**
  ```powershell
  $env:DOCKER_HOST = "tcp://127.0.0.1:23750"
  docker info --format '{{.Name}} {{.ServerVersion}}'
  ```

---

## Seguridad

Ambos endpoints están atados a `127.0.0.1` (mismo modelo de confianza que el endpoint local
de Docker Desktop). No expongas los puertos `23750` / `23752` a la red.
