use std::collections::{HashMap, HashSet};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

use serde::Serialize;
use tauri::State;

use crate::errors::{AppError, AppResult};
use crate::models::core_process::CoreProcessState;
use crate::services::core_process;
use crate::state::app_state::AppState;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeProcessMetrics {
    pub role: &'static str,
    pub label: &'static str,
    pub pid: Option<u32>,
    pub tracked: bool,
    pub cpu_percent: Option<f64>,
    pub memory_bytes: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimePerformanceSnapshot {
    pub sampled_at_unix_ms: u64,
    pub total_cpu_percent: Option<f64>,
    pub total_memory_bytes: Option<u64>,
    pub process_count: u32,
    pub tracked_process_count: u32,
    pub partial: bool,
    pub gui: RuntimeProcessMetrics,
    pub core: Option<RuntimeProcessMetrics>,
}

#[derive(Debug, Clone, Copy, Default)]
struct RawProcessSample {
    cpu_time_ns: Option<u64>,
    memory_bytes: Option<u64>,
}

#[derive(Debug, Clone, Copy)]
struct PreviousCpuSample {
    cpu_time_ns: u64,
    sampled_at: Instant,
}

#[derive(Default)]
struct RuntimeSampler {
    previous_cpu: HashMap<u32, PreviousCpuSample>,
}

static SAMPLER: OnceLock<Mutex<RuntimeSampler>> = OnceLock::new();

fn sampler() -> &'static Mutex<RuntimeSampler> {
    SAMPLER.get_or_init(|| Mutex::new(RuntimeSampler::default()))
}

fn normalized_cpu_percent(
    previous: PreviousCpuSample,
    current_cpu_time_ns: u64,
    sampled_at: Instant,
) -> Option<f64> {
    if current_cpu_time_ns < previous.cpu_time_ns {
        return Some(0.0);
    }
    let elapsed_ns = sampled_at
        .checked_duration_since(previous.sampled_at)?
        .as_nanos();
    if elapsed_ns == 0 {
        return None;
    }

    let logical_cpus = std::thread::available_parallelism()
        .map(|count| count.get())
        .unwrap_or(1) as f64;
    let used_ns = current_cpu_time_ns - previous.cpu_time_ns;
    let percent = (used_ns as f64 / elapsed_ns as f64) * 100.0 / logical_cpus;
    Some(percent.clamp(0.0, 100.0))
}

fn process_metrics(
    role: &'static str,
    label: &'static str,
    pid: Option<u32>,
    raw: Option<RawProcessSample>,
    sampled_at: Instant,
    sampler: &mut RuntimeSampler,
) -> RuntimeProcessMetrics {
    let Some(pid) = pid else {
        return RuntimeProcessMetrics {
            role,
            label,
            pid: None,
            tracked: false,
            cpu_percent: None,
            memory_bytes: None,
        };
    };

    let cpu_percent = raw
        .and_then(|sample| sample.cpu_time_ns)
        .and_then(|cpu_time_ns| {
            let previous = sampler.previous_cpu.get(&pid).copied();
            sampler.previous_cpu.insert(
                pid,
                PreviousCpuSample {
                    cpu_time_ns,
                    sampled_at,
                },
            );
            previous.and_then(|previous| normalized_cpu_percent(previous, cpu_time_ns, sampled_at))
        });

    RuntimeProcessMetrics {
        role,
        label,
        pid: Some(pid),
        tracked: raw.is_some(),
        cpu_percent,
        memory_bytes: raw.and_then(|sample| sample.memory_bytes),
    }
}

fn sum_f64(values: impl IntoIterator<Item = Option<f64>>) -> Option<f64> {
    let values = values.into_iter().flatten().collect::<Vec<_>>();
    (!values.is_empty()).then(|| values.into_iter().sum::<f64>().clamp(0.0, 100.0))
}

fn sum_u64(values: impl IntoIterator<Item = Option<u64>>) -> Option<u64> {
    let values = values.into_iter().flatten().collect::<Vec<_>>();
    (!values.is_empty()).then(|| values.into_iter().sum())
}

pub fn runtime_performance_snapshot(
    state: State<'_, AppState>,
    _include_threads: bool,
) -> AppResult<RuntimePerformanceSnapshot> {
    let core_status = core_process::status(state)?;
    let core_running = core_status.state == CoreProcessState::Running;
    let core_pid = core_running.then_some(core_status.pid).flatten();
    let gui_pid = std::process::id();

    let mut pids = vec![gui_pid];
    if let Some(pid) = core_pid {
        if pid != gui_pid {
            pids.push(pid);
        }
    }

    let raw_samples = platform::sample_many(&pids);
    let sampled_at = Instant::now();
    let mut sampler = sampler()
        .lock()
        .map_err(|_| AppError::internal("runtime performance sampler lock poisoned"))?;

    let gui = process_metrics(
        "gui",
        "ZNet Sink",
        Some(gui_pid),
        raw_samples.get(&gui_pid).copied(),
        sampled_at,
        &mut sampler,
    );
    let core = core_running.then(|| {
        process_metrics(
            "core",
            "Zero",
            core_pid,
            core_pid.and_then(|pid| raw_samples.get(&pid).copied()),
            sampled_at,
            &mut sampler,
        )
    });

    let active_pids = pids.into_iter().collect::<HashSet<_>>();
    sampler
        .previous_cpu
        .retain(|pid, _| active_pids.contains(pid));

    let process_count = 1 + u32::from(core_running);
    let tracked_process_count = u32::from(gui.tracked)
        + core
            .as_ref()
            .map(|metrics| u32::from(metrics.tracked))
            .unwrap_or(0);
    let partial = tracked_process_count < process_count;

    let total_cpu_percent = sum_f64([
        gui.cpu_percent,
        core.as_ref().and_then(|metrics| metrics.cpu_percent),
    ]);
    let total_memory_bytes = sum_u64([
        gui.memory_bytes,
        core.as_ref().and_then(|metrics| metrics.memory_bytes),
    ]);

    Ok(RuntimePerformanceSnapshot {
        sampled_at_unix_ms: crate::services::common::now_unix_ms(),
        total_cpu_percent,
        total_memory_bytes,
        process_count,
        tracked_process_count,
        partial,
        gui,
        core,
    })
}

#[cfg(target_os = "windows")]
mod platform {
    use std::collections::HashMap;

    use windows_sys::Win32::Foundation::{CloseHandle, FILETIME};
    use windows_sys::Win32::System::ProcessStatus::{
        K32GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS,
    };
    use windows_sys::Win32::System::Threading::{
        GetProcessTimes, OpenProcess, PROCESS_QUERY_INFORMATION, PROCESS_VM_READ,
    };

    use super::RawProcessSample;

    fn filetime_value(value: FILETIME) -> u64 {
        ((value.dwHighDateTime as u64) << 32) | value.dwLowDateTime as u64
    }

    fn sample_process(pid: u32) -> Option<RawProcessSample> {
        let handle = unsafe { OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_VM_READ, 0, pid) };
        if handle.is_null() {
            return None;
        }

        let mut creation = unsafe { std::mem::zeroed::<FILETIME>() };
        let mut exit = unsafe { std::mem::zeroed::<FILETIME>() };
        let mut kernel = unsafe { std::mem::zeroed::<FILETIME>() };
        let mut user = unsafe { std::mem::zeroed::<FILETIME>() };
        let times_ok =
            unsafe { GetProcessTimes(handle, &mut creation, &mut exit, &mut kernel, &mut user) }
                != 0;

        let mut memory = unsafe { std::mem::zeroed::<PROCESS_MEMORY_COUNTERS>() };
        memory.cb = std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32;
        let memory_ok = unsafe {
            K32GetProcessMemoryInfo(
                handle,
                &mut memory,
                std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32,
            )
        } != 0;
        unsafe {
            CloseHandle(handle);
        }

        Some(RawProcessSample {
            cpu_time_ns: times_ok
                .then(|| (filetime_value(kernel) + filetime_value(user)).saturating_mul(100)),
            memory_bytes: memory_ok.then_some(memory.WorkingSetSize as u64),
        })
    }

    pub fn sample_many(pids: &[u32]) -> HashMap<u32, RawProcessSample> {
        pids.iter()
            .filter_map(|pid| sample_process(*pid).map(|sample| (*pid, sample)))
            .collect()
    }
}

#[cfg(target_os = "linux")]
mod platform {
    use std::collections::HashMap;
    use std::fs;
    use std::os::raw::{c_int, c_long};
    use std::sync::OnceLock;

    use super::RawProcessSample;

    const SC_CLK_TCK: c_int = 2;

    unsafe extern "C" {
        fn sysconf(name: c_int) -> c_long;
    }

    fn clock_ticks_per_second() -> u64 {
        static VALUE: OnceLock<u64> = OnceLock::new();
        *VALUE.get_or_init(|| {
            let value = unsafe { sysconf(SC_CLK_TCK) };
            if value > 0 {
                value as u64
            } else {
                100
            }
        })
    }

    fn cpu_time_ns(pid: u32) -> Option<u64> {
        let stat = fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
        let end = stat.rfind(')')?;
        let fields = stat.get(end + 2..)?.split_whitespace().collect::<Vec<_>>();
        let user_ticks = fields.get(11)?.parse::<u64>().ok()?;
        let system_ticks = fields.get(12)?.parse::<u64>().ok()?;
        let ticks = user_ticks.saturating_add(system_ticks);
        let hz = clock_ticks_per_second();
        Some(((ticks as u128 * 1_000_000_000u128) / hz as u128) as u64)
    }

    fn memory_bytes(pid: u32) -> Option<u64> {
        let status = fs::read_to_string(format!("/proc/{pid}/status")).ok()?;
        status.lines().find_map(|line| {
            line.strip_prefix("VmRSS:").and_then(|value| {
                value
                    .split_whitespace()
                    .next()
                    .and_then(|value| value.parse::<u64>().ok())
                    .map(|kilobytes| kilobytes.saturating_mul(1024))
            })
        })
    }

    fn sample_process(pid: u32) -> Option<RawProcessSample> {
        let cpu_time_ns = cpu_time_ns(pid);
        let memory_bytes = memory_bytes(pid);
        if cpu_time_ns.is_none() && memory_bytes.is_none() {
            return None;
        }
        Some(RawProcessSample {
            cpu_time_ns,
            memory_bytes,
        })
    }

    pub fn sample_many(pids: &[u32]) -> HashMap<u32, RawProcessSample> {
        pids.iter()
            .filter_map(|pid| sample_process(*pid).map(|sample| (*pid, sample)))
            .collect()
    }
}

#[cfg(target_os = "macos")]
mod platform {
    use std::collections::HashMap;
    use std::ffi::c_void;
    use std::os::raw::c_int;
    use std::sync::OnceLock;

    use super::RawProcessSample;

    const PROC_PIDTASKINFO: c_int = 4;

    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    struct ProcTaskInfo {
        pti_virtual_size: u64,
        pti_resident_size: u64,
        pti_total_user: u64,
        pti_total_system: u64,
        pti_threads_user: u64,
        pti_threads_system: u64,
        pti_policy: i32,
        pti_faults: i32,
        pti_pageins: i32,
        pti_cow_faults: i32,
        pti_messages_sent: i32,
        pti_messages_received: i32,
        pti_syscalls_mach: i32,
        pti_syscalls_unix: i32,
        pti_csw: i32,
        pti_threadnum: i32,
        pti_numrunning: i32,
        pti_priority: i32,
    }

    #[repr(C)]
    struct MachTimebaseInfo {
        numer: u32,
        denom: u32,
    }

    #[link(name = "proc")]
    unsafe extern "C" {
        fn proc_pidinfo(
            pid: c_int,
            flavor: c_int,
            arg: u64,
            buffer: *mut c_void,
            buffersize: c_int,
        ) -> c_int;
    }

    unsafe extern "C" {
        fn mach_timebase_info(info: *mut MachTimebaseInfo) -> c_int;
    }

    fn timebase() -> (u64, u64) {
        static VALUE: OnceLock<(u64, u64)> = OnceLock::new();
        *VALUE.get_or_init(|| {
            let mut info = MachTimebaseInfo { numer: 1, denom: 1 };
            let result = unsafe { mach_timebase_info(&mut info) };
            if result == 0 && info.denom != 0 {
                (info.numer as u64, info.denom as u64)
            } else {
                (1, 1)
            }
        })
    }

    fn ticks_to_ns(ticks: u64) -> u64 {
        let (numer, denom) = timebase();
        ((ticks as u128 * numer as u128) / denom as u128) as u64
    }

    fn sample_process(pid: u32) -> Option<RawProcessSample> {
        let mut info = ProcTaskInfo::default();
        let size = std::mem::size_of::<ProcTaskInfo>();
        let read = unsafe {
            proc_pidinfo(
                pid as c_int,
                PROC_PIDTASKINFO,
                0,
                (&mut info as *mut ProcTaskInfo).cast::<c_void>(),
                size as c_int,
            )
        };
        if read != size as c_int {
            return None;
        }

        Some(RawProcessSample {
            cpu_time_ns: Some(ticks_to_ns(
                info.pti_total_user.saturating_add(info.pti_total_system),
            )),
            memory_bytes: Some(info.pti_resident_size),
        })
    }

    pub fn sample_many(pids: &[u32]) -> HashMap<u32, RawProcessSample> {
        pids.iter()
            .filter_map(|pid| sample_process(*pid).map(|sample| (*pid, sample)))
            .collect()
    }
}

#[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
mod platform {
    use std::collections::HashMap;

    use super::RawProcessSample;

    pub fn sample_many(_pids: &[u32]) -> HashMap<u32, RawProcessSample> {
        HashMap::new()
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::{normalized_cpu_percent, PreviousCpuSample};

    #[test]
    fn cpu_percent_is_normalized_to_machine_capacity() {
        let now = Instant::now();
        let previous = PreviousCpuSample {
            cpu_time_ns: 1_000_000_000,
            sampled_at: now - Duration::from_secs(1),
        };
        let percent = normalized_cpu_percent(previous, 1_500_000_000, now).unwrap();
        assert!(percent >= 0.0 && percent <= 100.0);
    }

    #[test]
    fn cpu_counter_reset_does_not_report_a_spike() {
        let now = Instant::now();
        let previous = PreviousCpuSample {
            cpu_time_ns: 2_000_000_000,
            sampled_at: now - Duration::from_secs(1),
        };
        assert_eq!(normalized_cpu_percent(previous, 1, now), Some(0.0));
    }
}
