//! Local HTTP server for the stream overlay browser source.
//!
//! Runs on loopback only. The server reads a shared `Arc<Mutex<StreamOverlayStateDto>>`
//! on every request and emits a self-contained HTML page, so the OBS/vMix browser
//! source sees live updates without the operator having to re-export.
//!
//! Binds to 127.0.0.1 on an ephemeral port (port 0 → OS picks). The chosen port
//! is reported to the frontend and the browser-source URL is `http://127.0.0.1:<port>/`.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use aletheia_core::now_ms;
use serde::{Deserialize, Serialize};
use tauri::State;
use tiny_http::{Header, Response, Server};

use crate::DesktopState;
use crate::dto::{StreamOverlayServerStatusDto, StreamOverlayStateDto};

pub struct StreamOverlayServer {
    inner: Mutex<Option<RunningServer>>,
    shared_state: Arc<Mutex<StreamOverlayStateDto>>,
}

struct RunningServer {
    shutdown: Arc<std::sync::atomic::AtomicBool>,
    port: u16,
    started_at_ms: u64,
    thread: Option<JoinHandle<()>>,
}

#[derive(Clone, Default, Serialize, Deserialize)]
pub struct InitialOverlayState;

impl Default for StreamOverlayServer {
    fn default() -> Self {
        Self {
            inner: Mutex::new(None),
            shared_state: Arc::new(Mutex::new(StreamOverlayStateDto {
                ticker_text: "Welcome — we're glad you're here.".to_string(),
                armed: false,
                live_reference: None,
                live_text: None,
                translations: HashMap::new(),
            })),
        }
    }
}

impl StreamOverlayServer {
    pub fn status(&self) -> StreamOverlayServerStatusDto {
        let guard = match self.inner.lock() {
            Ok(g) => g,
            Err(_) => {
                return StreamOverlayServerStatusDto {
                    running: false,
                    port: None,
                    url: None,
                    started_at_ms: None,
                };
            }
        };
        match &*guard {
            Some(r) => StreamOverlayServerStatusDto {
                running: true,
                port: Some(r.port),
                url: Some(format!("http://127.0.0.1:{}/", r.port)),
                started_at_ms: Some(r.started_at_ms),
            },
            None => StreamOverlayServerStatusDto {
                running: false,
                port: None,
                url: None,
                started_at_ms: None,
            },
        }
    }
}

fn render_html(state: &StreamOverlayStateDto) -> String {
    let reference = state
        .live_reference
        .as_deref()
        .map(escape_html)
        .unwrap_or_default();
    let live_text = state
        .live_text
        .as_deref()
        .map(escape_html)
        .unwrap_or_default();
    let ticker = escape_html(&state.ticker_text);

    // Build translation blocks, sorted for stable output.
    let mut codes: Vec<&String> = state.translations.keys().collect();
    codes.sort();
    let mut translations_html = String::new();
    for code in codes {
        if let Some(t) = state.translations.get(code) {
            translations_html.push_str(&format!(
                r#"<div class="tr"><span class="tag">{}</span><span class="tx">{}</span></div>"#,
                escape_html(code),
                escape_html(t)
            ));
        }
    }

    format!(
        r#"<!doctype html>
<html><head><meta charset="utf-8"/><title>Aletheia Stream Overlay</title>
<meta http-equiv="refresh" content="2">
<style>
  :root {{ color-scheme: dark; }}
  html,body {{ margin:0; padding:0; background:transparent; font-family:Inter,system-ui,-apple-system,Segoe UI,sans-serif; color:#fff; }}
  .wrap {{ position:fixed; inset:0; pointer-events:none; }}
  .ref-box {{ position:absolute; left:4%; bottom:14%; padding:10px 18px; border-radius:6px;
    background:linear-gradient(90deg,rgba(124,58,237,.88),rgba(79,70,229,.88));
    box-shadow:0 8px 28px -8px rgba(124,58,237,.6); max-width:62%; }}
  .ref {{ font-size:28px; font-weight:600; letter-spacing:.01em; }}
  .verse {{ margin-top:6px; font-size:20px; font-weight:500; opacity:.92; line-height:1.35; }}
  .tr-stack {{ position:absolute; right:4%; bottom:14%; display:flex; flex-direction:column; gap:6px; max-width:32%; }}
  .tr {{ display:flex; align-items:baseline; gap:8px; padding:6px 10px; border-radius:4px;
    background:rgba(10,10,14,.72); box-shadow:0 4px 16px -6px rgba(0,0,0,.5); font-size:16px; }}
  .tag {{ display:inline-block; min-width:28px; font-family:ui-monospace,Consolas,monospace; font-size:10px;
    text-transform:uppercase; letter-spacing:.08em; color:#c4b5fd; }}
  .tx {{ color:#fff; }}
  .ticker {{ position:absolute; left:0; right:0; bottom:0; overflow:hidden;
    background:rgba(10,10,14,.78); padding:8px 0; font-size:16px; white-space:nowrap; }}
  .ticker > span {{ display:inline-block; padding-left:100%; animation:marq 28s linear infinite; }}
  @keyframes marq {{ 0%{{transform:translateX(0)}} 100%{{transform:translateX(-100%)}} }}
</style></head><body>
<div class="wrap">
  {ref_block}
  {translations_block}
  <div class="ticker"><span>{ticker}</span></div>
</div></body></html>"#,
        ref_block = if reference.is_empty() && live_text.is_empty() {
            String::new()
        } else {
            format!(
                r#"<div class="ref-box"><div class="ref">{reference}</div>{verse}</div>"#,
                reference = reference,
                verse = if live_text.is_empty() {
                    String::new()
                } else {
                    format!(r#"<div class="verse">{live_text}</div>"#, live_text = live_text)
                }
            )
        },
        translations_block = if translations_html.is_empty() {
            String::new()
        } else {
            format!(r#"<div class="tr-stack">{}</div>"#, translations_html)
        },
        ticker = ticker,
    )
}

fn render_json(state: &StreamOverlayStateDto) -> String {
    serde_json::to_string(state).unwrap_or_else(|_| "{}".to_string())
}

fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

#[tauri::command]
pub fn start_stream_overlay_server(
    port: Option<u16>,
    state: State<'_, DesktopState>,
) -> Result<StreamOverlayServerStatusDto, String> {
    {
        let guard = state
            .stream_overlay
            .inner
            .lock()
            .map_err(|_| "stream server state poisoned".to_string())?;
        if guard.is_some() {
            drop(guard);
            return Ok(state.stream_overlay.status());
        }
    }

    let bind_port = port.unwrap_or(0);
    let addr = format!("127.0.0.1:{bind_port}");
    let server = Server::http(&addr).map_err(|e| format!("bind {addr}: {e}"))?;
    let actual_port = server
        .server_addr()
        .to_ip()
        .map(|sa| sa.port())
        .ok_or_else(|| "could not read bound port".to_string())?;

    let shutdown = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let shutdown_th = shutdown.clone();
    let shared = state.stream_overlay.shared_state.clone();

    let thread = std::thread::spawn(move || {
        // tiny_http recv_timeout lets us cooperatively honour the shutdown flag
        // without leaving sockets open.
        loop {
            if shutdown_th.load(std::sync::atomic::Ordering::Relaxed) {
                break;
            }
            match server.recv_timeout(std::time::Duration::from_millis(250)) {
                Ok(Some(request)) => {
                    let url = request.url().to_string();
                    let body;
                    let content_type;
                    if url.starts_with("/state") {
                        let snap = shared
                            .lock()
                            .map(|g| g.clone())
                            .unwrap_or_else(|_| StreamOverlayStateDto {
                                ticker_text: String::new(),
                                armed: false,
                                live_reference: None,
                                live_text: None,
                                translations: HashMap::new(),
                            });
                        body = render_json(&snap);
                        content_type = "application/json; charset=utf-8";
                    } else {
                        let snap = shared
                            .lock()
                            .map(|g| g.clone())
                            .unwrap_or_else(|_| StreamOverlayStateDto {
                                ticker_text: String::new(),
                                armed: false,
                                live_reference: None,
                                live_text: None,
                                translations: HashMap::new(),
                            });
                        body = render_html(&snap);
                        content_type = "text/html; charset=utf-8";
                    }
                    let resp = Response::from_string(body)
                        .with_header(
                            Header::from_bytes(&b"Content-Type"[..], content_type.as_bytes())
                                .unwrap(),
                        )
                        .with_header(
                            Header::from_bytes(
                                &b"Cache-Control"[..],
                                &b"no-store, max-age=0"[..],
                            )
                            .unwrap(),
                        );
                    let _ = request.respond(resp);
                }
                Ok(None) => {}
                Err(_) => break,
            }
        }
    });

    {
        let mut guard = state
            .stream_overlay
            .inner
            .lock()
            .map_err(|_| "stream server state poisoned".to_string())?;
        *guard = Some(RunningServer {
            shutdown,
            port: actual_port,
            started_at_ms: now_ms(),
            thread: Some(thread),
        });
    }
    Ok(state.stream_overlay.status())
}

#[tauri::command]
pub fn stop_stream_overlay_server(
    state: State<'_, DesktopState>,
) -> Result<StreamOverlayServerStatusDto, String> {
    let running = {
        let mut guard = state
            .stream_overlay
            .inner
            .lock()
            .map_err(|_| "stream server state poisoned".to_string())?;
        guard.take()
    };
    if let Some(mut r) = running {
        r.shutdown.store(true, std::sync::atomic::Ordering::Relaxed);
        if let Some(th) = r.thread.take() {
            let _ = th.join();
        }
    }
    Ok(state.stream_overlay.status())
}

#[tauri::command]
pub fn update_stream_overlay_state(
    new_state: StreamOverlayStateDto,
    state: State<'_, DesktopState>,
) -> Result<StreamOverlayServerStatusDto, String> {
    {
        let mut shared = state
            .stream_overlay
            .shared_state
            .lock()
            .map_err(|_| "stream shared state poisoned".to_string())?;
        *shared = new_state;
    }
    Ok(state.stream_overlay.status())
}

#[tauri::command]
pub fn get_stream_overlay_server_status(
    state: State<'_, DesktopState>,
) -> StreamOverlayServerStatusDto {
    state.stream_overlay.status()
}
