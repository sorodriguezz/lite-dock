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

/** Strip ANSI/terminal escape codes (we render plain text, not a full terminal). */
export function stripAnsi(s: string): string {
  return (
    s
      // CSI sequences (colors, cursor movement): ESC [ … final-byte.
      // eslint-disable-next-line no-control-regex
      .replace(/\x1b\[[0-9;?]*[ -/]*[@-~]/g, "")
      // OSC sequences (e.g. window title "ESC ] 0 ; … BEL/ST"), what shows up as
      // `]0;user@host: ~` in shell prompts under a TTY.
      // eslint-disable-next-line no-control-regex
      .replace(/\x1b\][^\x07\x1b]*(?:\x07|\x1b\\)/g, "")
      // Charset-selection escapes like ESC ( B.
      // eslint-disable-next-line no-control-regex
      .replace(/\x1b[()][0-9A-Za-z]/g, "")
      // Any leftover control chars (stray ESC, BEL…), keeping tab/newline/CR.
      // eslint-disable-next-line no-control-regex
      .replace(/[\x00-\x08\x0b\x0c\x0e-\x1f\x7f]/g, "")
  );
}

// ── Terminal (ANSI) → styled segments, so the console renders like a real one ──

export interface AnsiSeg {
  text: string;
  /** Inline CSS for this run ("" when default). */
  style: string;
}

// 16-colour palette tuned for the dark console background.
const ANSI16 = [
  "#3b4252", "#f87171", "#34d399", "#fbbf24", "#60a5fa", "#c084fc", "#2dd4bf", "#cdd6e6",
  "#5b6478", "#fca5a5", "#6ee7b7", "#fde047", "#93c5fd", "#d8b4fe", "#5eead4", "#ffffff",
];

function xterm256(n: number): string {
  if (n < 16) return ANSI16[n] ?? "";
  if (n < 232) {
    n -= 16;
    const cube = [0, 95, 135, 175, 215, 255];
    return `rgb(${cube[Math.floor(n / 36)]},${cube[Math.floor((n % 36) / 6)]},${cube[n % 6]})`;
  }
  const v = 8 + (n - 232) * 10;
  return `rgb(${v},${v},${v})`;
}

interface SgrState {
  fg: string;
  bg: string;
  bold: boolean;
  dim: boolean;
  italic: boolean;
  underline: boolean;
}
const freshSgr = (): SgrState => ({ fg: "", bg: "", bold: false, dim: false, italic: false, underline: false });

function applySgr(st: SgrState, params: string) {
  const codes = params === "" ? [0] : params.split(";").map((p) => parseInt(p, 10) || 0);
  for (let i = 0; i < codes.length; i++) {
    const c = codes[i];
    if (c === 0) Object.assign(st, freshSgr());
    else if (c === 1) st.bold = true;
    else if (c === 2) st.dim = true;
    else if (c === 22) (st.bold = false), (st.dim = false);
    else if (c === 3) st.italic = true;
    else if (c === 23) st.italic = false;
    else if (c === 4) st.underline = true;
    else if (c === 24) st.underline = false;
    else if (c >= 30 && c <= 37) st.fg = ANSI16[c - 30];
    else if (c >= 90 && c <= 97) st.fg = ANSI16[c - 90 + 8];
    else if (c === 39) st.fg = "";
    else if (c >= 40 && c <= 47) st.bg = ANSI16[c - 40];
    else if (c >= 100 && c <= 107) st.bg = ANSI16[c - 100 + 8];
    else if (c === 49) st.bg = "";
    else if (c === 38 || c === 48) {
      const key = c === 38 ? "fg" : "bg";
      if (codes[i + 1] === 5) ((st[key] = xterm256(codes[i + 2] ?? 0)), (i += 2));
      else if (codes[i + 1] === 2) ((st[key] = `rgb(${codes[i + 2] ?? 0},${codes[i + 3] ?? 0},${codes[i + 4] ?? 0})`), (i += 4));
    }
  }
}

function sgrStyle(st: SgrState): string {
  let s = "";
  if (st.fg) s += `color:${st.fg};`;
  if (st.bg) s += `background:${st.bg};`;
  if (st.bold) s += "font-weight:600;";
  if (st.dim) s += "opacity:.65;";
  if (st.italic) s += "font-style:italic;";
  if (st.underline) s += "text-decoration:underline;";
  return s;
}

// Re-parsing every visible line on each new log line is wasteful; cache by raw text.
const _segCache = new Map<string, AnsiSeg[]>();

/** Parse one line of terminal output into colour-styled segments (ANSI SGR). */
export function parseAnsiSegments(raw: string): AnsiSeg[] {
  const cached = _segCache.get(raw);
  if (cached) return cached;

  // Normalize CRLF → LF and drop bare CR. A single emitted chunk (e.g. from an
  // interactive shell) can carry MANY lines, so every newline must be kept —
  // collapsing on \r used to swallow command output in the terminal.
  const line = raw.replace(/\r\n/g, "\n").replace(/\r/g, "");

  const segs: AnsiSeg[] = [];
  const st = freshSgr();
  let buf = "";
  const flush = () => {
    if (buf) {
      segs.push({ text: buf, style: sgrStyle(st) });
      buf = "";
    }
  };

  let i = 0;
  while (i < line.length) {
    const ch = line[i];
    if (ch === "\x1b") {
      const next = line[i + 1];
      if (next === "[") {
        let j = i + 2;
        while (j < line.length && /[0-9;?]/.test(line[j])) j++;
        while (j < line.length && line[j] >= " " && line[j] <= "/") j++;
        if (line[j] === "m") {
          flush();
          applySgr(st, line.slice(i + 2, j));
        }
        i = j + 1;
        continue;
      }
      if (next === "]") {
        let j = i + 2;
        while (j < line.length && line[j] !== "\x07" && !(line[j] === "\x1b" && line[j + 1] === "\\")) j++;
        i = line[j] === "\x07" ? j + 1 : j + 2;
        continue;
      }
      if (next === "(" || next === ")") {
        i += 3;
        continue;
      }
      i += 1;
      continue;
    }
    // Drop control chars, but keep tab and newline (chunks can be multi-line).
    if (ch === "\x7f" || (ch < " " && ch !== "\t" && ch !== "\n")) {
      i++;
      continue;
    }
    buf += ch;
    i++;
  }
  flush();

  const res = segs.length ? segs : [{ text: "", style: "" }];
  if (_segCache.size > 8000) _segCache.clear();
  _segCache.set(raw, res);
  return res;
}
