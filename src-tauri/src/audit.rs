use aletheia_core::{AuditAction, now_ms};
use aletheia_store::{AletheiaStore, AuditEventRecord, IntegrationEventRecord};
use sha2::{Digest, Sha256};
use std::fmt::Write as _;

use crate::DesktopState;

/// Appends a hash-chained audit event to the audit log.
///
/// The chain works by: every row stores the SHA-256 hash of the previous row's
/// `event_hash`. The first row's `previous_hash` is the literal string `"genesis"`.
/// This creates a tamper-evident linked list — if any row is modified or deleted,
/// the chain breaks and can be detected by walking the log.
pub fn record_audit(
    store: &AletheiaStore,
    action: AuditAction,
    actor: &str,
    detail: &str,
) -> Result<(), String> {
    let previous_hash: String = store
        .connection()
        .query_row(
            "SELECT event_hash FROM audit_log ORDER BY id DESC LIMIT 1",
            [],
            |row| row.get(0),
        )
        .unwrap_or_else(|_| "genesis".to_string());

    let timestamp_ms = now_ms();
    let action_str = format!("{action:?}");
    let event_hash = sha256_hex(&format!(
        "{previous_hash}|{action_str}|{actor}|{detail}|{timestamp_ms}"
    ));

    store
        .insert_audit_event(&AuditEventRecord {
            timestamp_ms,
            action: action_str,
            actor: actor.to_string(),
            detail: detail.to_string(),
            previous_hash,
            event_hash,
        })
        .map_err(|e| e.to_string())
}

/// Convenience wrapper that acquires the store lock, calls `record_audit`,
/// and releases. Safe to call only from contexts where the store mutex is
/// currently free — do NOT call if the caller already holds `state.lock_store()`.
pub fn record_audit_state(
    state: &DesktopState,
    action: AuditAction,
    actor: &str,
    detail: &str,
) -> Result<(), String> {
    let store = state.lock_store()?;
    record_audit(&store, action, actor, detail)
}

/// Records an integration event into the integration_events table.
pub fn record_integration_event_state(
    state: &DesktopState,
    integration_id: &str,
    severity: &str,
    action: &str,
    detail: &str,
) -> Result<(), String> {
    let store = state.lock_store()?;
    store
        .insert_integration_event(&IntegrationEventRecord {
            timestamp_ms: now_ms(),
            integration_id: integration_id.to_string(),
            severity: severity.to_string(),
            action: action.to_string(),
            detail: detail.to_string(),
            receipt_json: String::new(),
        })
        .map_err(|e| e.to_string())
}

/// Trims the audit log to the last `keep` rows.
///
/// Because the log is hash-chained, deleting old rows would break verification
/// from the beginning. This function:
///   1. Deletes all rows except the newest `keep`.
///   2. Resets `previous_hash` on the oldest surviving row to `"genesis"` so
///      the chain is still internally consistent from that point forward.
///
/// Returns the number of rows deleted.
pub fn trim_audit_log(store: &AletheiaStore, keep: usize) -> Result<usize, String> {
    let conn = store.connection();

    // Count current rows
    let total: i64 = conn
        .query_row("SELECT COUNT(*) FROM audit_log", [], |row| row.get(0))
        .map_err(|e| e.to_string())?;

    let keep_i64 = keep as i64;
    if total <= keep_i64 {
        return Ok(0);
    }

    let to_delete = (total - keep_i64) as usize;

    // Delete the oldest rows
    conn.execute(
        "DELETE FROM audit_log WHERE id IN (
            SELECT id FROM audit_log ORDER BY id ASC LIMIT ?1
        )",
        rusqlite::params![to_delete],
    )
    .map_err(|e| e.to_string())?;

    // Reset the chain anchor on the new oldest surviving row
    conn.execute(
        "UPDATE audit_log SET previous_hash = 'genesis'
         WHERE id = (SELECT id FROM audit_log ORDER BY id ASC LIMIT 1)",
        [],
    )
    .map_err(|e| e.to_string())?;

    Ok(to_delete)
}

fn sha256_hex(input: &str) -> String {
    let digest = Sha256::digest(input.as_bytes());
    let mut output = String::with_capacity(digest.len() * 2);
    for byte in digest {
        let _ = write!(&mut output, "{byte:02x}");
    }
    output
}
