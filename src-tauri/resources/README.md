# Bundled engine resource

This directory must contain **`litedock-engine.tar`** at build time — the minimal
Alpine + Docker Engine rootfs that LiteDock imports into WSL2 as the
`litedock-engine` distro.

It is **not** committed to source control (it is a build artifact, a few hundred MB).
Produce it before running `npm run tauri build`:

```bash
# Run on a machine with Docker available (e.g. inside an existing WSL Ubuntu, or any Linux host)
bash engine/build-rootfs.sh
# → writes src-tauri/resources/litedock-engine.tar
```

`tauri.conf.json` references this file under `bundle.resources`, so the installer
embeds it and the app resolves it at runtime via the Tauri resource directory.

See `engine/README.md` and `docs/BUILD.md` for details.
