//! Generic key/value commands backed by `app_kv` in the local SQLite store.
//!
//! Lets the React stores (service plan, song library, stream overlay, clip
//! EDL) persist through the same store used by every other piece of state —
//! survives app reinstalls, sits inside the booth-pack export, and is the
//! single source of truth that `discover-brand`/`fleet-sync` already sees.
//!
//! Values are opaque JSON strings; the UI is responsible for serialising and
//! deserialising. A non-existent key returns `None`. There is no in-memory
//! cache: the writes are infrequent (operator clicks) and SQLite handles
//! concurrent reads via WAL.

use aletheia_core::now_ms;
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::DesktopState;

#[derive(Clone, Debug, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/gen/")]
#[serde(rename_all = "camelCase")]
pub struct KvKeyDto {
    pub key: String,
    pub updated_at_ms: u64,
}

/// Returns the value JSON for a key, or `None` if not present.
#[tauri::command]
pub fn kv_get(key: String, state: State<'_, DesktopState>) -> Result<Option<String>, String> {
    if key.is_empty() {
        return Err("kv key is empty".into());
    }
    let store = state.lock_store()?;
    store.kv_get(&key).map_err(|e| e.to_string())
}

/// Upserts a key. The value is stored verbatim — caller serialises.
#[tauri::command]
pub fn kv_set(
    key: String,
    value_json: String,
    state: State<'_, DesktopState>,
) -> Result<KvKeyDto, String> {
    if key.is_empty() {
        return Err("kv key is empty".into());
    }
    // Soft cap: refuse documents over 4 MB so a runaway store can't bloat the DB.
    if value_json.len() > 4 * 1024 * 1024 {
        return Err("kv value exceeds 4 MB cap".into());
    }
    let now = now_ms();
    let store = state.lock_store()?;
    store
        .kv_set(&key, &value_json, now as i64)
        .map_err(|e| e.to_string())?;
    Ok(KvKeyDto {
        key,
        updated_at_ms: now,
    })
}

/// Removes a key. Idempotent — returns Ok even if the key was absent.
#[tauri::command]
pub fn kv_delete(key: String, state: State<'_, DesktopState>) -> Result<(), String> {
    if key.is_empty() {
        return Err("kv key is empty".into());
    }
    let store = state.lock_store()?;
    store.kv_delete(&key).map_err(|e| e.to_string())?;
    Ok(())
}

/// Returns every `(key, updated_at_ms)` pair so the UI can do mtime-based
/// dirty checks without hitting the values themselves.
#[tauri::command]
pub fn kv_list_keys(state: State<'_, DesktopState>) -> Result<Vec<KvKeyDto>, String> {
    let store = state.lock_store()?;
    let rows = store.kv_list_keys().map_err(|e| e.to_string())?;
    Ok(rows
        .into_iter()
        .map(|(key, updated_at_ms)| KvKeyDto {
            key,
            updated_at_ms: updated_at_ms.max(0) as u64,
        })
        .collect())
}
