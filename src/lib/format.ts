// Small formatting helpers.

export function bytes(n: number | undefined | null): string {
  if (!n || n <= 0) return "0 B";
  const u = ["B", "KB", "MB", "GB", "TB"];
  let i = 0;
  let v = n;
  while (v >= 1024 && i < u.length - 1) {
    v /= 1024;
    i++;
  }
  return `${v.toFixed(v >= 10 || i === 0 ? 0 : 1)} ${u[i]}`;
}

export function ago(unixSecs: number | undefined): string {
  if (!unixSecs) return "—";
  const diff = Date.now() / 1000 - unixSecs;
  if (diff < 60) return "hace un momento";
  const mins = Math.floor(diff / 60);
  if (mins < 60) return `hace ${mins} min`;
  const hrs = Math.floor(mins / 60);
  if (hrs < 24) return `hace ${hrs} h`;
  const days = Math.floor(hrs / 24);
  if (days < 30) return `hace ${days} d`;
  const months = Math.floor(days / 30);
  if (months < 12) return `hace ${months} mes${months > 1 ? "es" : ""}`;
  return `hace ${Math.floor(months / 12)} a`;
}

export function shortId(id: string): string {
  return id.replace(/^sha256:/, "").slice(0, 12);
}

export function primaryTag(tags: string[]): string {
  if (!tags || tags.length === 0) return "<none>";
  const real = tags.find((t) => t !== "<none>:<none>");
  return real ?? tags[0];
}

/** Convert a path inside the engine distro to its Windows \\wsl$ UNC path. */
export function wslPath(linux: string): string {
  return "\\\\wsl$\\litedock-engine\\" + linux.replace(/^\/+/, "").replace(/\//g, "\\");
}

/** Strip ANSI escape codes (we render plain text, not a full terminal). */
export function stripAnsi(s: string): string {
  // eslint-disable-next-line no-control-regex
  return s.replace(/\x1b\[[0-9;?]*[ -/]*[@-~]/g, "");
}
