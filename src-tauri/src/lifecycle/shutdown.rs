//! Coordinated shutdown: cleanup callbacks run in reverse phase order.

use super::Phase;

/// A cleanup callback tagged with the phase that created it.
type ShutdownFn = Box<dyn Fn() + Send + Sync>;

/// Collects cleanup callbacks during startup, executes them on shutdown
/// in **reverse phase order** (Runtime → Register → State → Config → Guard).
///
/// The application registers callbacks during startup and runs them after the
/// event loop exits. Callback bounds make the coordinator Send + Sync naturally.
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

        // Start in reverse registration order. Stable sorting then preserves
        // LIFO within each phase while putting Runtime first and Guard last.
        let mut ordered: Vec<_> = self.guards.iter().rev().collect();
        ordered.sort_by_key(|entry| std::cmp::Reverse(entry.0));

        eprintln!("[ZNet] shutdown: begin ({} callbacks)", ordered.len());
        let mut failures = 0;
        for (phase, name, callback) in &ordered {
            eprintln!("[ZNet] shutdown: [{phase}] {name}");
            // Cleanup callbacks have no shared borrowed coordinator state.
            // A service panic must not skip later system-resource cleanup.
            if std::panic::catch_unwind(std::panic::AssertUnwindSafe(callback)).is_err() {
                failures += 1;
                eprintln!("[ZNet] shutdown: [{phase}] {name} panicked; continuing cleanup");
            }
        }
        if failures == 0 {
            eprintln!("[ZNet] shutdown: complete");
        } else {
            eprintln!("[ZNet] shutdown: finished with {failures} failed callbacks");
        }
    }
}
