import { writable } from "svelte/store";
import type { EngineStatus } from "./types";

export type Route =
  | "dashboard"
  | "containers"
  | "images"
  | "volumes"
  | "networks"
  | "build"
  | "compose"
  | "config";

export const route = writable<Route>("dashboard");
export const engine = writable<EngineStatus>({ running: false });
/** When set, the Images view pre-fills its filter with this (image hyperlink). */
export const imagesFilter = writable<string>("");

export type ToastKind = "info" | "success" | "error";
export interface ToastMsg {
  id: number;
  kind: ToastKind;
  text: string;
}
export const toasts = writable<ToastMsg[]>([]);

let nextId = 0;
export function notify(kind: ToastKind, text: string) {
  const id = ++nextId;
  toasts.update((t) => [...t, { id, kind, text }]);
  setTimeout(() => toasts.update((t) => t.filter((x) => x.id !== id)), 4200);
}

// ── confirmation dialog ────────────────────────────────────────────────
export interface ConfirmReq {
  id: number;
  title: string;
  message: string;
  confirmText: string;
  danger: boolean;
  resolve: (ok: boolean) => void;
}
/** Current pending confirmation, rendered once by <ConfirmDialog/> at the app root. */
export const confirmReq = writable<ConfirmReq | null>(null);

/**
 * Ask the user to confirm a destructive/disruptive action. Resolves `true` if
 * they choose Sí, `false` on No / Escape / backdrop. Usage:
 *   if (!(await askConfirm({ message: "¿Eliminar…?" }))) return;
 */
export function askConfirm(opts: {
  title?: string;
  message: string;
  confirmText?: string;
  danger?: boolean;
}): Promise<boolean> {
  return new Promise((resolve) => {
    confirmReq.set({
      id: ++nextId,
      title: opts.title ?? "¿Estás seguro?",
      message: opts.message,
      confirmText: opts.confirmText ?? "Sí, continuar",
      danger: opts.danger ?? true,
      resolve,
    });
  });
}

/** Copy text to the clipboard and show a confirmation toast. */
export async function copyText(text: string, label = "Copiado al portapapeles") {
  try {
    await navigator.clipboard.writeText(text);
    notify("success", label);
  } catch {
    notify("error", "No se pudo copiar al portapapeles");
  }
}

/** Wrap an async action with error toasting. Returns true on success. */
export async function guard(fn: () => Promise<unknown>, ok?: string): Promise<boolean> {
  try {
    await fn();
    if (ok) notify("success", ok);
    return true;
  } catch (e) {
    notify("error", String(e));
    return false;
  }
}
