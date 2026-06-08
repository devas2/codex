use std::time::Duration;

use tokio::time::timeout;

use super::McpOperationGate;

#[tokio::test]
async fn retirement_waits_for_started_operations_and_rejects_new_ones() {
    let gate = McpOperationGate::new();
    let permit = gate
        .begin_operation()
        .expect("operation should start before retirement");

    gate.begin_retirement();
    assert!(gate.begin_operation().is_none());

    let wait_for_operations = gate.wait_for_operations();
    tokio::pin!(wait_for_operations);
    timeout(Duration::from_millis(50), &mut wait_for_operations)
        .await
        .expect_err("retirement should wait for the active operation");

    drop(permit);
    timeout(Duration::from_secs(1), wait_for_operations)
        .await
        .expect("retirement should finish after the active operation");
}
