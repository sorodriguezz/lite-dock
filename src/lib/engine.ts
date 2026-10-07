// Engine start / stop / restart, shared by the sidebar engine card and the
// "engine stopped" empty states. Progress lives in the `engineBusy` store so
// every place that shows the engine state agrees.
import { get } from "svelte/store";
import { api } from "./api";
import { engine, engineBusy, guard, askConfirm } from "./stores";

export async function refreshEngine() {
  try {
    engine.set(await api.engineStatus());
  } catch {
    engine.set({ running: false });
  }
}

async function run(kind: "start" | "stop" | "restart", fn: () => Promise<unknown>, ok: string) {
  if (get(engineBusy)) return;
  engineBusy.set(kind);
  try {
    await guard(fn, ok);
    await refreshEngine();
  } finally {
    engineBusy.set("");
  }
}

export function startEngine() {
  return run("start", () => api.engineStart(), "Motor iniciado");
}

export async function stopEngine() {
  const ok = await askConfirm({
    title: "Detener el motor",
    message: "Se detendrán el motor de LiteDock y todos los contenedores en ejecución.",
    confirmText: "Detener motor",
  });
  if (ok) await run("stop", () => api.engineStop(), "Motor detenido");
}

export async function restartEngine() {
  const ok = await askConfirm({
    title: "Reiniciar el motor",
    message: "Los contenedores en ejecución se detendrán un momento mientras el motor se reinicia.",
    confirmText: "Reiniciar motor",
    danger: false,
  });
  if (ok) await run("restart", () => api.engineRestart(), "Motor reiniciado");
}

export const busyLabel: Record<string, string> = {
  start: "Iniciando…",
  stop: "Deteniendo…",
  restart: "Reiniciando…",
};
