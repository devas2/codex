use std::sync::Mutex;

use tokio_util::task::TaskTracker;
use tokio_util::task::task_tracker::TaskTrackerToken;

/// Tracks MCP operations that must finish before their manager can be retired.
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

    pub(crate) fn begin_retirement(&self) {
        let mut accepting = self
            .accepting
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *accepting = false;
        self.operations.close();
    }

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
