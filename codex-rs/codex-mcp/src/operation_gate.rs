use std::sync::Mutex;

use tokio_util::task::TaskTracker;
use tokio_util::task::task_tracker::TaskTrackerToken;

/// Coordinates operation admission with retirement of an MCP manager.
///
/// Operations hold a tracker token for their full lifetime. Retirement closes
/// admission atomically with respect to token acquisition, then waits for
/// previously admitted operations to release their tokens.
#[derive(Debug)]
pub(crate) struct McpOperationGate {
    accepting: Mutex<bool>,
    operations: TaskTracker,
}

impl McpOperationGate {
    pub(crate) fn new() -> Self {
        Self {
            accepting: Mutex::new(true),
            operations: TaskTracker::new(),
        }
    }

    /// Admits an operation while the manager is active.
    ///
    /// The returned token must be retained until the operation finishes.
    /// Returns `None` once retirement has begun.
    pub(crate) fn begin_operation(&self) -> Option<TaskTrackerToken> {
        let accepting = self
            .accepting
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if !*accepting {
            return None;
        }
        Some(self.operations.token())
    }

    pub(crate) fn is_accepting(&self) -> bool {
        *self
            .accepting
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Prevents new operations from starting while allowing admitted work to finish.
    pub(crate) fn begin_retirement(&self) {
        let mut accepting = self
            .accepting
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *accepting = false;
        self.operations.close();
    }

    /// Waits for every operation admitted before retirement to finish.
    ///
    /// Callers must begin retirement first so the tracker cannot admit more
    /// operations while this future is waiting.
    pub(crate) async fn wait_for_operations(&self) {
        self.operations.wait().await;
    }
}

impl Default for McpOperationGate {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[path = "operation_gate_tests.rs"]
mod tests;
