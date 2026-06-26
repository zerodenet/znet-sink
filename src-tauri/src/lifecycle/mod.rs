//! Application lifecycle: phased startup, coordinated shutdown, future plugin hooks.
//!
//! # Phases
//!
//! ```text
//! Guard → Config → State → Register → Runtime
//! ```
//!
//! Each phase is a clearly named step that runs to completion before the next begins.
//! On shutdown, cleanup callbacks registered during startup execute in **reverse phase order**.
//!
//! # Extensibility
//!
//! Future plugin systems implement [`OnPhase`] and register via [`Lifecycle::add_hook`].

pub mod phases;
pub mod shutdown;

use std::fmt;

use crate::errors::AppResult;
use crate::services::file_logger;

// ── Phases ──

/// Ordered phases of application startup.
///
/// Derives `Ord` so phases sort in startup order.
/// Shutdown runs in reverse order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Phase {
    /// Earliest: panic hooks, crash guards. Runs before anything else.
    Guard,
    /// Load configuration, domain data, logs from disk.
    Config,
    /// Construct `AppState` and other runtime structures.
    State,
    /// Register Tauri commands, plugins, event handlers.
    Register,
    /// System tray, auto-start core, window setup.
    Runtime,
}

impl Phase {
    /// Startup order (smallest → largest).
    pub const STARTUP: [Phase; 5] = [
        Phase::Guard,
        Phase::Config,
        Phase::State,
        Phase::Register,
        Phase::Runtime,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Phase::Guard => "guard",
            Phase::Config => "config",
            Phase::State => "state",
            Phase::Register => "register",
            Phase::Runtime => "runtime",
        }
    }
}

impl fmt::Display for Phase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

// ── Hook trait (future plugin interface) ──

/// A unit of work bound to a specific lifecycle phase.
///
/// Built-in subsystems and future plugins implement this trait
/// and register themselves with [`Lifecycle::add_hook`].
pub trait OnPhase: Send + Sync {
    /// Which phase this hook runs in.
    fn phase(&self) -> Phase;

    /// Human-readable name for logging.
    fn name(&self) -> &str;

    /// Execute the hook. Called once during startup at the declared phase.
    fn run(&self) -> AppResult<()>;
}

// ── Lifecycle runner ──

/// Orchestrates phased startup and collects shutdown guards.
///
/// Typical usage:
/// ```ignore
/// let mut lc = Lifecycle::new();
/// lc.add_hook(Box::new(GuardPhase));
/// lc.add_hook(Box::new(ConfigPhase::new(...)));
/// // ...
/// lc.startup()?;           // runs hooks in phase order
/// // ... tauri builder ...
/// lc.shutdown();            // runs guards in reverse order
/// ```
pub struct Lifecycle {
    hooks: Vec<Box<dyn OnPhase>>,
    shutdown: shutdown::ShutdownCoordinator,
}

impl Lifecycle {
    pub fn new() -> Self {
        Self {
            hooks: Vec::new(),
            shutdown: shutdown::ShutdownCoordinator::new(),
        }
    }

    /// Register a lifecycle hook. Phase ordering is determined by [`Phase::ord`].
    pub fn add_hook(&mut self, hook: Box<dyn OnPhase>) {
        self.hooks.push(hook);
    }

    /// Access the shutdown coordinator (e.g. to register cleanup from a `setup` closure).
    pub fn shutdown_coordinator(&self) -> &shutdown::ShutdownCoordinator {
        &self.shutdown
    }

    /// Access the shutdown coordinator mutably (to register cleanup callbacks).
    pub fn shutdown_coordinator_mut(&mut self) -> &mut shutdown::ShutdownCoordinator {
        &mut self.shutdown
    }

    /// Run all registered hooks in phase order.
    pub fn startup(&mut self) -> AppResult<()> {
        // Sort hooks by phase (Guard first, Runtime last).
        self.hooks.sort_by_key(|h| h.phase());

        let mut current_phase = None;
        for hook in &self.hooks {
            let phase = hook.phase();
            if current_phase != Some(phase) {
                file_logger::line(&format!("lifecycle: entering phase {phase}"));
                current_phase = Some(phase);
            }
            file_logger::line(&format!("lifecycle:   → {}", hook.name()));
            hook.run()?;
        }
        file_logger::line("lifecycle: startup complete");
        Ok(())
    }

    /// Run all shutdown guards in reverse phase order.
    pub fn shutdown(&self) {
        self.shutdown.run();
    }
}
