use crate::DesktopState;
use crate::dto::*;
use aletheia_output::OutputAdapter;

/// Real-time health checks for the operator dashboard.
/// Each item reflects actual machine state at call time.
pub fn live_health(state: &DesktopState) -> Result<Vec<HealthItemDto>, String> {
    let mut items = Vec::new();

    // 1. Database connectivity
    match state.lock_store() {
        Ok(store) => {
            let row_count: i64 = store
                .connection()
                .query_row("SELECT COUNT(*) FROM audit_log", [], |r| r.get(0))
                .unwrap_or(0);
            items.push(HealthItemDto {
                label: "Local database".to_string(),
                state: "healthy".to_string(),
                detail: format!("{row_count} audit events recorded"),
                action: String::new(),
            });
        }
        Err(e) => {
            items.push(HealthItemDto {
                label: "Local database".to_string(),
                state: "degraded".to_string(),
                detail: format!("Store lock failed: {e}"),
                action: "Restart Aletheia".to_string(),
            });
        }
    }

    // 2. Disk space — check the drive where the database lives
    {
        let db_path = &state.database_path;
        let available_mb = free_mb_for_path(db_path);
        let (disk_state, disk_detail, disk_action) = if let Some(available_mb) = available_mb {
            if available_mb >= 500 {
                (
                    "healthy",
                    format!("{available_mb} MB free on database drive"),
                    String::new(),
                )
            } else if available_mb >= 100 {
                (
                    "degraded",
                    format!("Only {available_mb} MB free — export support bundle soon"),
                    "Free disk space".to_string(),
                )
            } else {
                (
                    "offline",
                    format!("Critical: only {available_mb} MB free — audio capture may fail"),
                    "Free disk space immediately".to_string(),
                )
            }
        } else {
            (
                "degraded",
                "Disk free-space check is unavailable on this runtime.".to_string(),
                "Confirm free disk space before service".to_string(),
            )
        };
        items.push(HealthItemDto {
            label: "Disk space".to_string(),
            state: disk_state.to_string(),
            detail: disk_detail,
            action: disk_action,
        });
    }

    // 3. vMix
    if let Ok(adapter) = state.lock_vmix() {
        use crate::output_health_to_state_detail;
        let (s, d) = output_health_to_state_detail(adapter.status().health);
        let action = if s == "connected" || s == "ready" {
            String::new()
        } else {
            "Check vMix Web Controller settings".to_string()
        };
        items.push(HealthItemDto {
            label: "vMix".to_string(),
            state: s,
            detail: d,
            action,
        });
    }

    // 4. OBS
    if let Ok(adapter) = state.lock_obs() {
        use crate::output_health_to_state_detail;
        let (s, d) = output_health_to_state_detail(adapter.status().health);
        let obs_action = if d.contains("not configured") {
            "Configure OBS WebSocket".to_string()
        } else {
            String::new()
        };
        items.push(HealthItemDto {
            label: "OBS Studio".to_string(),
            state: s,
            detail: d,
            action: obs_action,
        });
    }

    // 5. Scripture library
    if let Ok(store) = state.lock_store() {
        let verse_count: i64 = store
            .connection()
            .query_row("SELECT COUNT(*) FROM verses", [], |r| r.get(0))
            .unwrap_or(0);
        let (lib_state, lib_detail) = if verse_count > 30_000 {
            ("healthy", format!("{verse_count} KJV verses indexed"))
        } else if verse_count > 0 {
            (
                "degraded",
                format!("Only {verse_count} verses — library may be incomplete"),
            )
        } else {
            (
                "offline",
                "Scripture library not seeded — search will fail".to_string(),
            )
        };
        items.push(HealthItemDto {
            label: "Scripture library".to_string(),
            state: lib_state.to_string(),
            detail: lib_detail.to_string(),
            action: if lib_state == "offline" {
                "Restart to reseed".to_string()
            } else {
                String::new()
            },
        });
    }

    // 6. Vector semantic scripture retrieval sidecar
    match crate::vector_kb_health(std::time::Duration::from_millis(450)) {
        Ok(body) => {
            let translation_count = serde_json::from_str::<serde_json::Value>(&body)
                .ok()
                .and_then(|json| {
                    json.get("translations")
                        .and_then(|value| value.as_array())
                        .map(|items| items.len())
                })
                .unwrap_or(0);
            items.push(HealthItemDto {
                label: "Vector Knowledge Base".to_string(),
                state: if translation_count > 0 {
                    "healthy".to_string()
                } else {
                    "degraded".to_string()
                },
                detail: if translation_count > 0 {
                    format!("{translation_count} semantic translation indexes online")
                } else {
                    "Sidecar online but no translation indexes reported".to_string()
                },
                action: if translation_count > 0 {
                    String::new()
                } else {
                    "Rebuild vector indexes".to_string()
                },
            });
        }
        Err(error) => {
            items.push(HealthItemDto {
                label: "Vector Knowledge Base".to_string(),
                state: "degraded".to_string(),
                detail: error,
                action: "Start or rebuild semantic search sidecar".to_string(),
            });
        }
    }

    Ok(items)
}

/// Production-gate health checks (heavier checks for pre-service sign-off).
pub fn production_health(state: &DesktopState) -> Result<Vec<HealthItemDto>, String> {
    let mut items = live_health(state)?;

    // Add ProPresenter check
    if let Ok(adapter) = state.lock_propresenter() {
        use crate::output_health_to_state_detail;
        let (s, d) = output_health_to_state_detail(adapter.status().health);
        items.push(HealthItemDto {
            label: "ProPresenter".to_string(),
            state: s,
            detail: d,
            action: String::new(),
        });
    }

    // Add Bitfocus Companion check
    if let Ok(adapter) = state.lock_companion() {
        use crate::output_health_to_state_detail;
        let (s, d) = output_health_to_state_detail(adapter.status().health);
        items.push(HealthItemDto {
            label: "Bitfocus Companion".to_string(),
            state: s,
            detail: d,
            action: String::new(),
        });
    }

    Ok(items)
}

/// Returns approximate free disk space in MB for the volume containing `path`.
fn free_mb_for_path(path: &std::path::Path) -> Option<u64> {
    // Walk up to find the deepest existing ancestor
    let mut check = path;
    loop {
        if check.exists() {
            break;
        }
        match check.parent() {
            Some(p) => check = p,
            None => return None,
        }
    }
    let check = if check.is_file() {
        check.parent().unwrap_or(check)
    } else {
        check
    };

    // Use statvfs on Unix or GetDiskFreeSpaceEx on Windows via std
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::ffi::OsStrExt;
        let wide: Vec<u16> = check
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        let mut free_bytes: u64 = 0;
        let mut total_bytes: u64 = 0;
        let mut total_free: u64 = 0;
        let ok = unsafe {
            windows_get_disk_free(
                wide.as_ptr(),
                &mut free_bytes,
                &mut total_bytes,
                &mut total_free,
            )
        };
        if ok == 0 {
            None
        } else {
            Some(free_bytes / (1024 * 1024))
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        None
    }
}

#[cfg(target_os = "windows")]
unsafe fn windows_get_disk_free(
    path: *const u16,
    free_caller: &mut u64,
    total: &mut u64,
    total_free: &mut u64,
) -> i32 {
    // Use GetDiskFreeSpaceExW via a raw declaration
    unsafe extern "system" {
        fn GetDiskFreeSpaceExW(
            lpDirectoryName: *const u16,
            lpFreeBytesAvailableToCaller: *mut u64,
            lpTotalNumberOfBytes: *mut u64,
            lpTotalNumberOfFreeBytes: *mut u64,
        ) -> i32;
    }
    unsafe { GetDiskFreeSpaceExW(path, free_caller, total, total_free) }
}
