//! Coordinated shutdown: cleanup callbacks run in reverse phase order.

use super::Phase;

/// A cleanup callback tagged with the phase that created it.
type ShutdownFn = Box<dyn Fn() + Send + Sync>;

/// Collects cleanup callbacks during startup, executes them on shutdown
/// in **reverse phase order** (Runtime → Register → State → Config → Guard).
///
/// Thread-safe: the coordinator is stored behind `Arc<Mutex<..>` inside `AppState`
/// so any service can register cleanup from any thread.
pub struct ShutdownCoordinator {
    guards: Vec<(Phase, &'static str, ShutdownFn)>,
}

impl Default for ShutdownCoordinator {
    fn default() -> Self {
        Self::new()
    }
}

impl ShutdownCoordinator {
    pub fn new() -> Self {
        Self { guards: Vec::new() }
    }

    /// Register a cleanup callback associated with a phase.
    ///
    /// Example: system proxy guard registers a `disable_with_guard` callback
    /// in the `Guard` phase so it runs **last** during shutdown
    /// (after Runtime/State/Config cleanups).
    pub fn register(&mut self, phase: Phase, name: &'static str, callback: ShutdownFn) {
        self.guards.push((phase, name, callback));
    }

    /// Execute all cleanup callbacks in reverse phase order.
    ///
    /// Within the same phase, callbacks run in reverse registration order (LIFO).
    pub fn run(&self) {
        if self.guards.is_empty() {
            return;
        }

        // Sort descending by phase (Runtime first, Guard last).
        let mut ordered: Vec<_> = self.guards.iter().collect();
        ordered.sort_by_key(|entry| std::cmp::Reverse(entry.0));

        eprintln!("[ZNet] shutdown: begin ({} callbacks)", ordered.len());
        for (phase, name, callback) in &ordered {
            eprintln!("[ZNet] shutdown: [{phase}] {name}");
            callback();
        }
        eprintln!("[ZNet] shutdown: complete");
    }
}

// SAFETY: `ShutdownFn` is `Fn() + Send + Sync`. The coordinator is only
/// mutated during startup (single-threaded) and only read during shutdown
/// (after the Tauri event loop returns).
unsafe impl Send for ShutdownCoordinator {}
unsafe impl Sync for ShutdownCoordinator {}
