// Long-running streamed jobs (image build, compose up/down/logs) live here, at
// module scope, instead of inside their views. App.svelte swaps views with
// `{#if $route === …}`, so a view is unmounted when you navigate away; the
// job (busy flag, heading, output and the backend event subscription) must
// outlive it, so coming back shows the live log and a disabled button until
// the process really ends.
import { listen } from "./api";
import { notify } from "./stores";
import type { OutputLine } from "./types";

const MAX_LINES = 4000;

export class StreamJob {
  /** Id of the running action ("" = idle); views use it to disable buttons / show spinners. */
  busy = $state("");
  /** Heading of the current (or last) run. */
  label = $state("");
  /** Output of the current (or last) run, capped to the last MAX_LINES lines. */
  lines = $state.raw<string[]>([]);
  /** Bumped every time a run ends, so a (re)mounted view can refresh its data. */
  finished = $state(0);

  #event: string;
  #listening: Promise<unknown> | undefined;

  constructor(event: string) {
    this.#event = event;
  }

  /** Subscribe to the backend's output event once, for the whole app lifetime. */
  #listen() {
    this.#listening ??= listen<OutputLine>(this.#event, (e) => {
      const line = e.payload.line;
      if (!line.startsWith("__EXIT__")) this.append(line);
    }).catch((err) => {
      this.#listening = undefined; // retry on the next run
      throw err;
    });
    return this.#listening;
  }

  append(...more: string[]) {
    this.lines = [...this.lines, ...more].slice(-MAX_LINES);
  }

  /**
   * Run `fn` as this job's single active action: marks it busy, resets the log
   * (optionally seeded) under `heading`, and clears the busy flag when it ends.
   * Ignored if another action of this job is still running.
   */
  async run(id: string, heading: string, fn: () => Promise<void>, seed: string[] = []) {
    if (this.busy) return;
    this.busy = id;
    try {
      await this.#listen();
      this.label = heading;
      this.lines = seed;
      await fn();
    } catch (e) {
      notify("error", String(e));
    } finally {
      this.busy = "";
      this.finished++;
    }
  }
}

export const buildJob = new StreamJob("build-output");
export const composeJob = new StreamJob("compose-output");
