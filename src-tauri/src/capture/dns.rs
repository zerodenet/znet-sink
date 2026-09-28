//! Host DNS capture belongs to the client; the kernel still owns DNS answers.
use crate::{
    errors::{AppError, AppResult},
    models::zero_runtime::GuiTunStatus,
};
use serde::Serialize;
use std::{
    io,
    net::{IpAddr, Ipv4Addr},
    sync::Mutex,
};

mod controller;
#[cfg(any(target_os = "linux", target_os = "windows"))]
mod helper;
mod platform;
mod preflight;
pub(crate) use controller::spawn;
#[cfg(test)]
mod tests;

#[derive(Clone, Debug, PartialEq, Eq)]
struct Target {
    interface: String,
    server: String,
    pid: u32,
}
trait CaptureGuard {
    fn refresh(&mut self, target: &Target) -> AppResult<()>;
    fn stop(&mut self) -> AppResult<()>;
}
#[derive(Default)]
struct State<G = platform::Guard> {
    target: Option<Target>,
    guard: Option<G>,
    error: Option<String>,
}
static OWNED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static STATE: Mutex<State> = Mutex::new(State {
    target: None,
    guard: None,
    error: None,
});

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub state: &'static str,
    pub server: Option<String>,
    pub error: Option<String>,
}

pub(crate) fn validate_target(interface: &str, server: &str, pid: u32) -> io::Result<()> {
    let ip = server.parse::<IpAddr>().map_err(io::Error::other)?;
    if interface.is_empty()
        || interface.len() > 64
        || !interface
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"_-. ".contains(&c))
        || pid == 0
        || pid > i32::MAX as u32
        || ip.is_loopback()
        || ip.is_unspecified()
        || ip.is_multicast()
    {
        return Err(io::Error::other("invalid TUN DNS target"));
    }
    Ok(())
}

fn target(tun: &GuiTunStatus, pid: u32) -> AppResult<Option<Target>> {
    if !tun.enabled || !tun.auto_route || !tun.dns_hijack {
        return Ok(None);
    }
    if !tun.healthy {
        return Err(AppError::internal("TUN 不健康，暂不修改系统 DNS"));
    }
    let interface = tun
        .name
        .clone()
        .ok_or_else(|| AppError::internal("TUN 未提供接口名称"))?;
    // The synthetic peer is routed to TUN but is not a local host address.
    // Sending queries to the local address itself would bypass packet ingress.
    let cidr = tun
        .addresses
        .iter()
        .find(|cidr| {
            cidr.split('/')
                .next()
                .is_some_and(|ip| ip.parse::<Ipv4Addr>().is_ok())
        })
        .or(tun.addr.as_ref())
        .ok_or_else(|| AppError::internal("TUN DNS 接管需要 IPv4 接口地址"))?;
    let (ip, prefix) = cidr
        .split_once('/')
        .ok_or_else(|| AppError::internal("TUN DNS 接管需要带前缀的接口地址"))?;
    let ip = ip
        .parse::<Ipv4Addr>()
        .map_err(|_| AppError::internal("TUN DNS 接管需要 IPv4 接口地址"))?;
    let prefix = prefix
        .parse::<u32>()
        .map_err(|_| AppError::internal("TUN DNS 前缀无效"))?;
    if prefix > 30 {
        return Err(AppError::internal(
            "TUN DNS 接管需要可用的对端地址，IPv4 前缀必须不大于 /30",
        ));
    }
    let raw = u32::from(ip);
    let peer = raw
        .checked_add(1)
        .ok_or_else(|| AppError::internal("TUN DNS 对端地址溢出"))?;
    let mask = if prefix == 0 {
        0
    } else {
        u32::MAX << (32 - prefix)
    };
    if raw & mask != peer & mask || peer & !mask == !mask {
        return Err(AppError::internal("TUN DNS 对端地址不是可用的子网地址"));
    }
    let server = Ipv4Addr::from(peer).to_string();
    validate_target(&interface, &server, pid).map_err(|e| AppError::internal(e.to_string()))?;
    Ok(Some(Target {
        interface,
        server,
        pid,
    }))
}

pub(crate) fn ensure(tun: &GuiTunStatus, pid: u32) -> AppResult<()> {
    let next = target(tun, pid)?;
    let mut state = STATE
        .lock()
        .map_err(|_| AppError::internal("TUN DNS state lock poisoned"))?;
    transition(&mut state, next, platform::Guard::install)
}
fn transition<G: CaptureGuard>(
    state: &mut State<G>,
    next: Option<Target>,
    install: impl FnOnce(&Target) -> AppResult<G>,
) -> AppResult<()> {
    if state.target == next {
        if let Some(error) = &state.error {
            return Err(AppError::internal(error.clone()));
        }
        if let (Some(guard), Some(target)) = (&mut state.guard, &next) {
            let result = guard.refresh(target);
            if let Err(error) = &result {
                state.error = Some(error.message.clone());
            }
            return result;
        }
        return Ok(());
    }
    stop(state)?;
    state.target = next.clone();
    if let Some(target) = next {
        match install(&target) {
            Ok(guard) => state.guard = Some(guard),
            Err(error) => {
                let message = format!("系统 DNS 尚未接管到 TUN：{}", error.message);
                state.error = Some(message.clone());
                return Err(AppError::internal(message));
            }
        }
    }
    Ok(())
}
fn stop<G: CaptureGuard>(state: &mut State<G>) -> AppResult<()> {
    if let Some(guard) = state.guard.as_mut() {
        if let Err(error) = guard.stop() {
            state.error = Some(format!("系统 DNS 撤销未完成：{}", error.message));
            return Err(error);
        }
    }
    state.guard.take();
    state.target = None;
    state.error = None;
    Ok(())
}
pub(crate) fn release() -> AppResult<()> {
    let mut state = STATE
        .lock()
        .map_err(|_| AppError::internal("TUN DNS state lock poisoned"))?;
    stop(&mut state)
}
pub(crate) fn retry_failed() -> AppResult<()> {
    let mut state = STATE
        .lock()
        .map_err(|_| AppError::internal("TUN DNS state lock poisoned"))?;
    if state.error.is_some() {
        stop(&mut state)?;
    }
    Ok(())
}
fn record_failure(tun: &GuiTunStatus, pid: u32, message: String) -> AppResult<()> {
    let mut state = STATE
        .lock()
        .map_err(|_| AppError::internal("TUN DNS state lock poisoned"))?;
    stop(&mut state)?;
    state.target = Some(Target {
        interface: tun.name.clone().unwrap_or_default(),
        server: String::new(),
        pid,
    });
    state.error = Some(message);
    Ok(())
}
pub(crate) fn status(tun: &GuiTunStatus) -> Status {
    let state = STATE.lock().unwrap_or_else(|e| e.into_inner());
    let applies = state.target.as_ref().is_some_and(|target| {
        (tun.enabled && tun.name.as_deref().unwrap_or_default() == target.interface)
            || (!tun.enabled && state.error.is_some())
    });
    Status {
        state: if applies && state.error.is_some() {
            "error"
        } else if !OWNED.load(std::sync::atomic::Ordering::Relaxed)
            || !tun.enabled
            || !tun.dns_hijack
            || !tun.auto_route
        {
            "inactive"
        } else if applies && state.guard.is_some() {
            "configured"
        } else {
            "pending"
        },
        server: applies
            .then(|| state.target.as_ref().unwrap().server.clone())
            .filter(|s| !s.is_empty()),
        error: applies.then(|| state.error.clone()).flatten(),
    }
}

pub fn run_if_requested() -> Option<io::Result<()>> {
    #[cfg(any(target_os = "linux", target_os = "windows"))]
    {
        helper::run_if_requested()
    }
    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        None
    }
}
