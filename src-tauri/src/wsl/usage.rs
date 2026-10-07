//! LiteDock's own footprint for the sidebar meter, measured in-process.
//!
//! One `NtQuerySystemInformation(SystemProcessInformation)` call — the same
//! snapshot Task Manager uses — gives every process's parent, image name,
//! working set and CPU times without opening handles (so it also covers the
//! `vmmem` VM process, owned by another account). No PowerShell child, no
//! sleep per call: CPU% is the delta against the previous sample, and results
//! are cached briefly so concurrent callers share one snapshot.

use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

/// Calls within this window get the previous result (no new snapshot).
const CACHE_TTL: Duration = Duration::from_millis(1500);
/// A baseline older than this is stale: re-measure over a short window.
const BASELINE_MAX_AGE: Duration = Duration::from_secs(30);
/// Measurement window when there is no usable baseline (first call).
const FIRST_WINDOW: Duration = Duration::from_millis(300);

/// One process from a system snapshot.
struct Proc {
    pid: usize,
    parent: usize,
    /// FILETIME (100 ns since 1601) — tells a reused PID apart.
    create_time: i64,
    /// User + kernel CPU time, 100 ns units.
    cpu: u64,
    working_set: u64,
    /// Image name, lowercase, without `.exe`.
    name: String,
}

#[derive(Default)]
struct Sampler {
    /// CPU time per (pid, create_time) at the previous sample.
    prev: HashMap<(usize, i64), u64>,
    prev_at: Option<Instant>,
    prev_filetime: i64,
    last: (f64, u64),
    last_at: Option<Instant>,
    /// Size of the last snapshot buffer, to get it right on the first try.
    buf_hint: usize,
}

static SAMPLER: tokio::sync::Mutex<Option<Sampler>> = tokio::sync::Mutex::const_new(None);

/// Approximate footprint of LiteDock itself: (cpu_percent, ram_bytes), summing
/// our process + all its descendants (WebView2 browser/renderer/GPU, wsl.exe
/// helpers) + the WSL VM process (`vmmemWSL`, else `vmmem`). CPU% is relative
/// to the whole machine (all cores = 100). Returns (0, 0) if the probe fails.
pub async fn app_usage() -> (f64, u64) {
    let mut guard = SAMPLER.lock().await;
    let s = guard.get_or_insert_with(Sampler::default);
    if s.last_at.is_some_and(|t| t.elapsed() < CACHE_TTL) {
        return s.last;
    }
    if s.prev_at.map_or(true, |t| t.elapsed() > BASELINE_MAX_AGE) {
        s.sample();
        tokio::time::sleep(FIRST_WINDOW).await;
    }
    let r = s.sample();
    s.last = r;
    s.last_at = Some(Instant::now());
    r
}

impl Sampler {
    fn sample(&mut self) -> (f64, u64) {
        let Some(procs) = snapshot(&mut self.buf_hint) else {
            return (0.0, 0);
        };
        let now = Instant::now();
        let now_ft = filetime_now();
        let tracked = tracked(&procs);

        let ram: u64 = tracked.iter().map(|p| p.working_set).sum();
        let mut delta = 0u64;
        let mut cur = HashMap::with_capacity(tracked.len());
        for p in &tracked {
            let key = (p.pid, p.create_time);
            match self.prev.get(&key) {
                Some(before) => delta += p.cpu.saturating_sub(*before),
                // Started since the previous sample: all its CPU time is new.
                None if self.prev_at.is_some() && p.create_time >= self.prev_filetime => {
                    delta += p.cpu
                }
                None => {}
            }
            cur.insert(key, p.cpu);
        }
        let cpu = match self.prev_at {
            Some(t) => {
                let wall_100ns = now.duration_since(t).as_nanos() as f64 / 100.0;
                let cores = std::thread::available_parallelism()
                    .map(|n| n.get())
                    .unwrap_or(1) as f64;
                if wall_100ns > 0.0 {
                    (delta as f64 / (wall_100ns * cores) * 100.0).clamp(0.0, 100.0)
                } else {
                    0.0
                }
            }
            None => 0.0,
        };
        self.prev = cur;
        self.prev_at = Some(now);
        self.prev_filetime = now_ft;
        ((cpu * 10.0).round() / 10.0, ram)
    }
}

/// Our process, its whole descendant tree, and the WSL VM process(es).
fn tracked(procs: &[Proc]) -> Vec<&Proc> {
    let own = std::process::id() as usize;
    let mut children: HashMap<usize, Vec<&Proc>> = HashMap::new();
    for p in procs {
        if p.pid != p.parent {
            children.entry(p.parent).or_default().push(p);
        }
    }
    let mut out: Vec<&Proc> = Vec::new();
    let mut seen: HashSet<usize> = HashSet::new();
    let mut queue: Vec<&Proc> = procs.iter().filter(|p| p.pid == own).collect();
    while let Some(p) = queue.pop() {
        if !seen.insert(p.pid) {
            continue;
        }
        out.push(p);
        if let Some(kids) = children.get(&p.pid) {
            // A child created before its "parent" is a reused parent PID.
            queue.extend(kids.iter().filter(|c| c.create_time >= p.create_time));
        }
    }
    let vm = if procs.iter().any(|p| p.name == "vmmemwsl") {
        "vmmemwsl"
    } else {
        "vmmem"
    };
    out.extend(
        procs
            .iter()
            .filter(|p| p.name == vm && !seen.contains(&p.pid)),
    );
    out
}

/// Current time as a FILETIME (100 ns intervals since 1601-01-01).
fn filetime_now() -> i64 {
    const UNIX_EPOCH_AS_FILETIME: i64 = 116_444_736_000_000_000;
    let since_unix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| (d.as_nanos() / 100) as i64)
        .unwrap_or(0);
    UNIX_EPOCH_AS_FILETIME + since_unix
}

#[cfg(not(windows))]
fn snapshot(_buf_hint: &mut usize) -> Option<Vec<Proc>> {
    None
}

#[cfg(windows)]
fn snapshot(buf_hint: &mut usize) -> Option<Vec<Proc>> {
    use std::ffi::c_void;

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct UnicodeString {
        length: u16,
        maximum_length: u16,
        buffer: *const u16,
    }

    /// Leading part of `SYSTEM_PROCESS_INFORMATION` (winternl.h / ntexapi.h),
    /// up to `WorkingSetSize`; natural C layout matches on x86 and x64.
    #[repr(C)]
    #[derive(Clone, Copy)]
    #[allow(dead_code)]
    struct SystemProcessInformation {
        next_entry_offset: u32,
        number_of_threads: u32,
        working_set_private_size: i64,
        hard_fault_count: u32,
        number_of_threads_high_watermark: u32,
        cycle_time: u64,
        create_time: i64,
        user_time: i64,
        kernel_time: i64,
        image_name: UnicodeString,
        base_priority: i32,
        unique_process_id: usize,
        inherited_from_unique_process_id: usize,
        handle_count: u32,
        session_id: u32,
        unique_process_key: usize,
        peak_virtual_size: usize,
        virtual_size: usize,
        page_fault_count: u32,
        peak_working_set_size: usize,
        working_set_size: usize,
    }

    #[link(name = "ntdll")]
    extern "system" {
        fn NtQuerySystemInformation(
            class: u32,
            info: *mut c_void,
            len: u32,
            ret_len: *mut u32,
        ) -> i32;
    }
    const SYSTEM_PROCESS_INFORMATION: u32 = 5;
    const STATUS_INFO_LENGTH_MISMATCH: i32 = 0xC000_0004_u32 as i32;
    const STATUS_BUFFER_TOO_SMALL: i32 = 0xC000_0023_u32 as i32;
    let entry_size = std::mem::size_of::<SystemProcessInformation>();

    let mut bytes = (*buf_hint).max(256 * 1024);
    for _ in 0..5 {
        // u64 storage → 8-byte aligned, as the kernel expects.
        let mut buf = vec![0u64; bytes.div_ceil(8)];
        let len = u32::try_from(buf.len() * 8).ok()?;
        let mut needed = 0u32;
        // SAFETY: `buf` is a writable buffer of `len` bytes owned by us.
        let status = unsafe {
            NtQuerySystemInformation(
                SYSTEM_PROCESS_INFORMATION,
                buf.as_mut_ptr().cast(),
                len,
                &mut needed,
            )
        };
        if status == STATUS_INFO_LENGTH_MISMATCH || status == STATUS_BUFFER_TOO_SMALL {
            // Processes come and go between calls: leave some headroom.
            bytes = (needed as usize).max(bytes) + 64 * 1024;
            continue;
        }
        if status < 0 {
            return None;
        }
        *buf_hint = bytes;

        let base = buf.as_ptr() as *const u8;
        let total = len as usize;
        let mut out = Vec::with_capacity(512);
        let mut off = 0usize;
        while off + entry_size <= total {
            // SAFETY: the entry lies within `buf` (bounds checked above).
            let e: SystemProcessInformation =
                unsafe { std::ptr::read_unaligned(base.add(off).cast()) };
            // The name points into `buf`; only read it if it really does.
            let name_ptr = e.image_name.buffer as usize;
            let name_len = e.image_name.length as usize;
            let name = if name_ptr >= base as usize
                && name_ptr + name_len <= base as usize + total
                && name_len > 0
            {
                // SAFETY: range validated against `buf` just above.
                let units =
                    unsafe { std::slice::from_raw_parts(e.image_name.buffer, name_len / 2) };
                let s = String::from_utf16_lossy(units).to_lowercase();
                s.strip_suffix(".exe").map(str::to_string).unwrap_or(s)
            } else {
                String::new()
            };
            out.push(Proc {
                pid: e.unique_process_id,
                parent: e.inherited_from_unique_process_id,
                create_time: e.create_time,
                cpu: (e.user_time.max(0) as u64).saturating_add(e.kernel_time.max(0) as u64),
                working_set: e.working_set_size as u64,
                name,
            });
            if e.next_entry_offset == 0 {
                break;
            }
            off += e.next_entry_offset as usize;
        }
        return Some(out);
    }
    None
}
