use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::PoisonError;

use codex_tools::ToolName;

/// Turn-scoped availability policy for tools.
///
/// Extensions can attach this to turn-scoped [`ExtensionData`](crate::ExtensionData)
/// when they own context that makes one or more tools inappropriate for the
/// current turn. Core tool planning can omit unavailable tools, while handlers
/// can use the same policy for defensive rejection if an unavailable tool is
/// invoked anyway.
#[derive(Debug, Default)]
pub struct ToolAvailability {
    unavailability_reason_by_tool_name: Mutex<HashMap<ToolName, String>>,
}

impl ToolAvailability {
    /// Marks a tool unavailable for the current turn with the reason to return
    /// to the model if the unavailable tool is invoked anyway.
    pub fn mark_unavailable(&self, tool_name: ToolName, unavailability_reason: impl Into<String>) {
        let _replaced = self
            .unavailability_reason_by_tool_name()
            .insert(tool_name, unavailability_reason.into());
    }

    /// Returns the model-facing unavailability reason for `tool_name`, if one exists.
    pub fn unavailability_reason(&self, tool_name: &ToolName) -> Option<String> {
        self.unavailability_reason_by_tool_name()
            .get(tool_name)
            .cloned()
    }

    fn unavailability_reason_by_tool_name(
        &self,
    ) -> std::sync::MutexGuard<'_, HashMap<ToolName, String>> {
        self.unavailability_reason_by_tool_name
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }
}
