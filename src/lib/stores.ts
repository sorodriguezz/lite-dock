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
/** Ask the bottom terminal panel to open on a given shell ("engine" = docker CLI in WSL). */
export const terminalReq = writable<{ kind: "host" | "engine"; at: number } | null>(null);
/** Engine start/stop/restart in progress (shared by the sidebar card and empty states). */
export const engineBusy = writable<"" | "start" | "stop" | "restart">("");
/** Open a container's detail page from elsewhere (e.g. a row on the Panel). */
export const openContainerReq = writable<string | null>(null);

export type ToastKind = "info" | "success" | "error";
export interface ToastMsg {
  id: number;
  kind: ToastKind;
  text: string;
}
export const toasts = writable<ToastMsg[]>([]);

let nextId = 0;
const MAX_TOASTS = 5;
export function dismissToast(id: number) {
  toasts.update((t) => t.filter((x) => x.id !== id));
}
/** Show a toast. Errors stay until dismissed (they often carry details worth reading). */
export function notify(kind: ToastKind, text: string) {
  const id = ++nextId;
  toasts.update((t) => [...t, { id, kind, text }].slice(-MAX_TOASTS));
  if (kind !== "error") setTimeout(() => dismissToast(id), 4200);
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
 * they confirm, `false` on Cancelar / Escape. Usage:
 *   if (!(await askConfirm({ message: "¿Eliminar…?" }))) return;
 */
export function askConfirm(opts: {
  title?: string;
  message: string;
  confirmText?: string;
  danger?: boolean;
}): Promise<boolean> {
  return new Promise((resolve) => {
    // Only one confirmation at a time: a newer request cancels the pending one
    // instead of leaving its promise unresolved forever.
    confirmReq.update((prev) => {
      prev?.resolve(false);
      return null;
    });
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
