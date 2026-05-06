//! Local HTTP server for the stream overlay browser source.
//!
//! Runs on loopback only. The server reads a shared `Arc<Mutex<StreamOverlayStateDto>>`
//! on every request and emits a self-contained HTML page, so the OBS/vMix browser
//! source sees live updates without the operator having to re-export.
//!
//! Binds to 127.0.0.1 on a stable default port (7777) so the operator can
//! configure vMix Browser Input once with `http://127.0.0.1:7777/projection`
//! and never have to update the URL between launches. If 7777 is taken (e.g.
//! a leftover instance or another tool), we fall back to an ephemeral port
//! so the service still starts. The chosen port is always reported back to
//! the frontend.

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
                url: Some(format!("http://127.0.0.1:{}/projection", r.port)),
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

/// Static cinematic projection page.
///
/// The page is rendered once and never reloads — meta-refresh caused the
/// vMix Browser Input to flicker every two seconds and reset any in-flight
/// CSS animation (architectural-vision call-out: "no jarring cuts when content
/// changes"). Instead the page fetches `/state` once a second, diffs against
/// the previously-rendered reference, and crossfades the reference card and
/// translation stack when content changes.
///
/// The state JSON path stays compatible with the existing `/state` endpoint,
/// so nothing else needs to change.
fn render_html() -> String {
    PROJECTION_PAGE.to_string()
}

const PROJECTION_PAGE: &str = r#"<!doctype html>
<html><head><meta charset="utf-8"/><title>Aletheia Stream Overlay</title>
<style>
  :root { color-scheme: dark; }
  html,body { margin:0; padding:0; background:transparent;
    font-family:Inter,system-ui,-apple-system,Segoe UI,sans-serif; color:#fff; }
  .wrap { position:fixed; inset:0; pointer-events:none; }
  .ref-box { position:absolute; left:4%; bottom:14%; padding:14px 22px; border-radius:8px;
    background:linear-gradient(90deg,rgba(124,58,237,.88),rgba(79,70,229,.88));
    box-shadow:0 12px 36px -10px rgba(124,58,237,.55);
    max-width:62%;
    opacity:0; transform:translateY(14px);
    transition:opacity .55s ease, transform .55s cubic-bezier(.2,.7,.2,1); }
  .ref-box.show { opacity:1; transform:translateY(0); }
  .ref { font-size:30px; font-weight:600; letter-spacing:.01em; line-height:1.2; }
  .verse { margin-top:8px; font-size:22px; font-weight:500; opacity:.94; line-height:1.4; }
  .tr-stack { position:absolute; right:4%; bottom:14%;
    display:flex; flex-direction:column; gap:6px; max-width:32%;
    opacity:0; transform:translateY(14px);
    transition:opacity .55s ease .08s, transform .55s cubic-bezier(.2,.7,.2,1) .08s; }
  .tr-stack.show { opacity:1; transform:translateY(0); }
  .tr { display:flex; align-items:baseline; gap:8px; padding:6px 10px; border-radius:4px;
    background:rgba(10,10,14,.72); box-shadow:0 4px 16px -6px rgba(0,0,0,.5); font-size:16px; }
  .tag { display:inline-block; min-width:28px; font-family:ui-monospace,Consolas,monospace; font-size:10px;
    text-transform:uppercase; letter-spacing:.08em; color:#c4b5fd; }
  .tx { color:#fff; }
  .ticker { position:absolute; left:0; right:0; bottom:0; overflow:hidden;
    background:rgba(10,10,14,.78); padding:8px 0; font-size:16px; white-space:nowrap; }
  .ticker > span { display:inline-block; padding-left:100%; animation:marq 28s linear infinite; }
  @keyframes marq { 0%{transform:translateX(0)} 100%{transform:translateX(-100%)} }
  .swap-out { opacity:0 !important; transform:translateY(-10px) !important;
    transition:opacity .25s ease, transform .25s ease; }
</style></head><body>
<div class="wrap">
  <div id="refBox" class="ref-box" hidden>
    <div id="refLine" class="ref"></div>
    <div id="verseLine" class="verse"></div>
  </div>
  <div id="trStack" class="tr-stack" hidden></div>
  <div class="ticker"><span id="tickerText"></span></div>
</div>
<script>
  // Cinematic projection page. Polls /state every 1s and crossfades on change.
  // Plain ES5-flavored JS so vMix's older embedded browser can run it without
  // transpilation.
  (function () {
    var POLL_MS = 1000;
    var refBox = document.getElementById('refBox');
    var refLine = document.getElementById('refLine');
    var verseLine = document.getElementById('verseLine');
    var trStack = document.getElementById('trStack');
    var tickerText = document.getElementById('tickerText');
    var lastKey = null;
    var lastTicker = null;

    function escapeHtml(s) {
      return String(s == null ? '' : s)
        .replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')
        .replace(/"/g, '&quot;').replace(/'/g, '&#39;');
    }

    function renderTranslations(map) {
      if (!map) return '';
      var keys = Object.keys(map).sort();
      if (!keys.length) return '';
      var html = '';
      for (var i = 0; i < keys.length; i++) {
        html += '<div class="tr"><span class="tag">' + escapeHtml(keys[i])
          + '</span><span class="tx">' + escapeHtml(map[keys[i]]) + '</span></div>';
      }
      return html;
    }

    function setReference(ref, verse, trMap) {
      var hasContent = (ref && ref.length) || (verse && verse.length);
      if (!hasContent) {
        refBox.classList.remove('show');
        refBox.hidden = true;
        trStack.classList.remove('show');
        trStack.hidden = true;
        return;
      }
      refBox.hidden = false;
      refLine.textContent = ref || '';
      verseLine.textContent = verse || '';
      verseLine.style.display = (verse && verse.length) ? '' : 'none';
      // force layout, then trigger transition
      void refBox.offsetWidth;
      refBox.classList.add('show');

      var trHtml = renderTranslations(trMap);
      if (trHtml) {
        trStack.hidden = false;
        trStack.innerHTML = trHtml;
        void trStack.offsetWidth;
        trStack.classList.add('show');
      } else {
        trStack.classList.remove('show');
        trStack.hidden = true;
      }
    }

    function applyState(s) {
      var ref = s.liveReference || '';
      var verse = s.liveText || '';
      var tr = s.translations || {};
      var trKey = Object.keys(tr).sort().map(function (k) { return k + '=' + tr[k]; }).join('|');
      var key = ref + '\u0001' + verse + '\u0001' + trKey;

      if (key !== lastKey) {
        if (lastKey !== null) {
          // brief swap-out, then swap in new content
          refBox.classList.add('swap-out');
          trStack.classList.add('swap-out');
          setTimeout(function () {
            refBox.classList.remove('swap-out');
            trStack.classList.remove('swap-out');
            setReference(ref, verse, tr);
            lastKey = key;
          }, 250);
        } else {
          setReference(ref, verse, tr);
          lastKey = key;
        }
      }

      var ticker = s.tickerText || '';
      if (ticker !== lastTicker) {
        tickerText.textContent = ticker;
        lastTicker = ticker;
      }
    }

    function tick() {
      var url = '/state?_=' + Date.now();
      try {
        var xhr = new XMLHttpRequest();
        xhr.open('GET', url, true);
        xhr.onreadystatechange = function () {
          if (xhr.readyState !== 4) return;
          if (xhr.status >= 200 && xhr.status < 300) {
            try {
              var s = JSON.parse(xhr.responseText);
              applyState(s);
            } catch (_) { /* malformed payload — leave last known good */ }
          }
        };
        xhr.send();
      } catch (_) { /* swallow — never crash projection page */ }
    }

    tick();
    setInterval(tick, POLL_MS);
  })();
</script>
</body></html>
"#;

fn render_json(state: &StreamOverlayStateDto) -> String {
    serde_json::to_string(state).unwrap_or_else(|_| "{}".to_string())
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

    // Prefer the architectural-vision stable port 7777 so vMix Browser Input
    // can be configured once with `http://127.0.0.1:7777/projection`.
    // Fall back to ephemeral if 7777 is occupied.
    const STABLE_PROJECTION_PORT: u16 = 7777;
    let requested = port.unwrap_or(STABLE_PROJECTION_PORT);
    let server = match Server::http(format!("127.0.0.1:{requested}")) {
        Ok(s) => s,
        Err(e) if requested == STABLE_PROJECTION_PORT => {
            log::warn!(
                "[stream-server] port {STABLE_PROJECTION_PORT} unavailable ({e}), falling back to ephemeral"
            );
            Server::http("127.0.0.1:0").map_err(|e| format!("bind ephemeral: {e}"))?
        }
        Err(e) => return Err(format!("bind 127.0.0.1:{requested}: {e}")),
    };
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
                    // `/projection` is the architectural-vision canonical path;
                    // `/` is kept as a back-compat alias for older operator
                    // configs. Both serve the same HTML.
                    if url.starts_with("/state") {
                        let snap = shared.lock().map(|g| g.clone()).unwrap_or_else(|_| {
                            StreamOverlayStateDto {
                                ticker_text: String::new(),
                                armed: false,
                                live_reference: None,
                                live_text: None,
                                translations: HashMap::new(),
                            }
                        });
                        body = render_json(&snap);
                        content_type = "application/json; charset=utf-8";
                    } else {
                        // The projection HTML is static — content updates flow
                        // through `/state` polled by the in-page JS, so we
                        // never need to lock `shared` here.
                        body = render_html();
                        content_type = "text/html; charset=utf-8";
                    }
                    let resp = Response::from_string(body)
                        .with_header(
                            Header::from_bytes(&b"Content-Type"[..], content_type.as_bytes())
                                .unwrap(),
                        )
                        .with_header(
                            Header::from_bytes(&b"Cache-Control"[..], &b"no-store, max-age=0"[..])
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
    let mut new_state = new_state;
    auto_fanout_translations(&mut new_state, &state);
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

/// When the operator sets a new live reference but does not pre-populate the
/// per-translation map, look the verse up in every locally-loaded translation
/// pack and fill in the missing translations. The projection page renders
/// whatever is present, so a missing pack just means that language slot is
/// blank rather than a hard error — vision rule "wrong scripture is worse
/// than no scripture" applies: never fabricate, always derive from the DB.
fn auto_fanout_translations(state_dto: &mut StreamOverlayStateDto, state: &DesktopState) {
    let Some(reference) = state_dto.live_reference.as_ref() else {
        return;
    };
    if reference.trim().is_empty() {
        return;
    }
    let Some((book, chapter, verse_start)) = crate::parse_reference(reference) else {
        return;
    };

    // Operator-configurable pack list, sourced from `RuntimeState`. Falls back
    // to KJV-only if the runtime is contended — never fabricate translations.
    let packs: Vec<String> = match state.lock_runtime() {
        Ok(rt) => rt.translation_packs.clone(),
        Err(_) => return,
    };
    if packs.is_empty() {
        return;
    }

    let Ok(store) = state.lock_store() else {
        return;
    };
    for tid in &packs {
        let key = tid.to_uppercase();
        if state_dto.translations.contains_key(&key) {
            continue;
        }
        if let Ok(Some(record)) = store.find_verse(tid, &book, chapter, verse_start) {
            state_dto.translations.insert(key, record.text);
        }
    }
}

#[tauri::command]
pub fn get_stream_overlay_server_status(
    state: State<'_, DesktopState>,
) -> StreamOverlayServerStatusDto {
    state.stream_overlay.status()
}
