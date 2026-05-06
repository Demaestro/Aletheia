//! CCLI usage log — durable via the chained-hash audit log.
//!
//! Every call to `log_ccli_usage` appends a `LiveOutputSent` audit entry with a
//! canonical `CCLI:<number>|<session>|<operator>|<title>` detail prefix. The
//! usage log can therefore be rebuilt deterministically by walking the audit
//! log and filtering for that prefix — there is no separate mutable table the
//! operator can silently edit, so the quarterly CCLI report is tamper-evident.

use std::sync::Mutex;

use aletheia_core::{AuditAction, now_ms};
use tauri::State;

use crate::DesktopState;
use crate::audit::record_audit_state;
use crate::dto::CcliUsageEntryDto;

const CCLI_DETAIL_PREFIX: &str = "CCLI:";
const CCLI_DEDUPE_WINDOW_MS: u64 = 90_000;
const CCLI_CACHE_CAP: usize = 2000;

/// In-memory cache of CCLI usage entries. Rebuilt on demand from the audit log
/// (see `rebuild_cache_from_audit`). Persisted state is the audit log itself,
/// not this cache.
pub struct CcliUsageCache {
    entries: Mutex<Vec<CcliUsageEntryDto>>,
    rebuilt: Mutex<bool>,
}

impl Default for CcliUsageCache {
    fn default() -> Self {
        Self {
            entries: Mutex::new(Vec::new()),
            rebuilt: Mutex::new(false),
        }
    }
}

fn encode_detail(entry: &CcliUsageEntryDto) -> String {
    // Pipe-separated for robust parsing; CCLI numbers are numeric so no escapes needed.
    // JSON would also work but the pipe form shows up cleaner in audit-log dumps.
    format!(
        "{prefix}{ccli}|{session}|{operator}|{title}",
        prefix = CCLI_DETAIL_PREFIX,
        ccli = entry.ccli_number.replace('|', "/"),
        session = entry.service_session_id.replace('|', "/"),
        operator = entry.operator.replace('|', "/"),
        title = entry.song_title.replace('|', "/")
    )
}

fn decode_detail(detail: &str, timestamp_ms: u64) -> Option<CcliUsageEntryDto> {
    let rest = detail.strip_prefix(CCLI_DETAIL_PREFIX)?;
    let parts: Vec<&str> = rest.splitn(4, '|').collect();
    if parts.len() < 4 {
        return None;
    }
    Some(CcliUsageEntryDto {
        id: format!("ccli-{timestamp_ms}"),
        ccli_number: parts[0].to_string(),
        service_session_id: parts[1].to_string(),
        operator: parts[2].to_string(),
        song_title: parts[3].to_string(),
        sent_live_at_ms: timestamp_ms,
    })
}

/// Walk the audit log once per process lifetime and hydrate the in-memory cache.
fn ensure_cache_hydrated(state: &DesktopState) -> Result<(), String> {
    {
        let rebuilt = state
            .ccli_usage
            .rebuilt
            .lock()
            .map_err(|_| "ccli cache rebuilt-flag poisoned".to_string())?;
        if *rebuilt {
            return Ok(());
        }
    }
    let rows = {
        let store = state.lock_store()?;
        let mut stmt = store
            .connection()
            .prepare(
                "SELECT timestamp_ms, detail FROM audit_log \
                 WHERE detail LIKE 'CCLI:%' \
                 ORDER BY timestamp_ms DESC \
                 LIMIT 2000",
            )
            .map_err(|e| e.to_string())?;
        let mapped = stmt
            .query_map([], |row| {
                let ts: i64 = row.get(0)?;
                let detail: String = row.get(1)?;
                Ok((ts as u64, detail))
            })
            .map_err(|e| e.to_string())?;
        mapped.flatten().collect::<Vec<(u64, String)>>()
    };
    let mut hydrated: Vec<CcliUsageEntryDto> = rows
        .into_iter()
        .filter_map(|(ts, detail)| decode_detail(&detail, ts))
        .collect();
    // DB returned DESC; ensure cache is newest-first.
    hydrated.sort_by(|a, b| b.sent_live_at_ms.cmp(&a.sent_live_at_ms));
    {
        let mut entries = state
            .ccli_usage
            .entries
            .lock()
            .map_err(|_| "ccli cache poisoned".to_string())?;
        *entries = hydrated;
    }
    {
        let mut rebuilt = state
            .ccli_usage
            .rebuilt
            .lock()
            .map_err(|_| "ccli cache rebuilt-flag poisoned".to_string())?;
        *rebuilt = true;
    }
    Ok(())
}

#[tauri::command]
pub fn log_ccli_usage(
    ccli_number: String,
    song_title: String,
    service_session_id: String,
    operator: String,
    state: State<'_, DesktopState>,
) -> Result<CcliUsageEntryDto, String> {
    if ccli_number.trim().is_empty() {
        return Err("ccli number is required".into());
    }
    ensure_cache_hydrated(&state)?;
    let now = now_ms();

    // Dedupe: if the same ccli number was logged in the last 90 seconds, skip.
    {
        let entries = state
            .ccli_usage
            .entries
            .lock()
            .map_err(|_| "ccli cache poisoned".to_string())?;
        if let Some(recent) = entries.iter().find(|e| e.ccli_number == ccli_number)
            && now.saturating_sub(recent.sent_live_at_ms) < CCLI_DEDUPE_WINDOW_MS
        {
            return Ok(recent.clone());
        }
    }

    let entry = CcliUsageEntryDto {
        id: format!("ccli-{now}-{}", ccli_number.replace(['|', ' '], "-")),
        ccli_number,
        song_title,
        sent_live_at_ms: now,
        service_session_id,
        operator: operator.clone(),
    };
    let detail = encode_detail(&entry);
    record_audit_state(&state, AuditAction::LiveOutputSent, &operator, &detail)?;

    let mut entries = state
        .ccli_usage
        .entries
        .lock()
        .map_err(|_| "ccli cache poisoned".to_string())?;
    entries.insert(0, entry.clone());
    if entries.len() > CCLI_CACHE_CAP {
        entries.truncate(CCLI_CACHE_CAP);
    }
    Ok(entry)
}

#[tauri::command]
pub fn list_ccli_usage(
    limit: Option<u32>,
    state: State<'_, DesktopState>,
) -> Result<Vec<CcliUsageEntryDto>, String> {
    ensure_cache_hydrated(&state)?;
    let entries = state
        .ccli_usage
        .entries
        .lock()
        .map_err(|_| "ccli cache poisoned".to_string())?;
    let take = limit.map(|n| n as usize).unwrap_or(entries.len());
    Ok(entries.iter().take(take).cloned().collect())
}

#[tauri::command]
pub fn export_ccli_usage_csv(state: State<'_, DesktopState>) -> Result<String, String> {
    ensure_cache_hydrated(&state)?;
    let entries = state
        .ccli_usage
        .entries
        .lock()
        .map_err(|_| "ccli cache poisoned".to_string())?;
    let mut out =
        String::from("ccli_number,song_title,sent_live_at_iso,service_session_id,operator\n");
    for e in entries.iter() {
        let iso = iso_from_ms(e.sent_live_at_ms);
        out.push_str(&format!(
            "\"{}\",\"{}\",\"{}\",\"{}\",\"{}\"\n",
            csv_escape(&e.ccli_number),
            csv_escape(&e.song_title),
            iso,
            csv_escape(&e.service_session_id),
            csv_escape(&e.operator)
        ));
    }
    Ok(out)
}

fn csv_escape(input: &str) -> String {
    input.replace('"', "\"\"")
}

fn iso_from_ms(ms: u64) -> String {
    // Minimal ISO-8601 UTC stamp without chrono. Good enough for CCLI reporting.
    let secs = ms / 1000;
    let millis = ms % 1000;
    let days_since_epoch = secs / 86_400;
    let sod = secs % 86_400;
    let h = sod / 3600;
    let m = (sod % 3600) / 60;
    let s = sod % 60;
    // Convert days since 1970-01-01 to Y-M-D using civil_from_days (Howard Hinnant).
    let z = days_since_epoch as i64 + 719_468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u64;
    let yoe = (doe
        .saturating_sub(doe / 1460)
        .saturating_sub(doe / 36524)
        .saturating_add(doe / 146096))
        / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if month <= 2 { y + 1 } else { y };
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z",
        year, month, d, h, m, s, millis
    )
}
