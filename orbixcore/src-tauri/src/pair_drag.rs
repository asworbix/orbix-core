//! Drag-to-pair: the user drags Orbix's pair handle onto another app's
//! window; Bob works out who that is and pairs with it (peer.rs).
//!
//! The gesture starts inside the webview, but the drop lands on another
//! process's window, so no DOM event can see it. Instead: a transparent,
//! click-through Tauri window covers the screen and draws the bridge,
//! while a Rust task polls the global cursor and mouse-button state
//! (Core Graphics, no permissions) and finds the window under the
//! cursor with CGWindowListCopyWindowInfo (owner pid + bounds need no
//! Screen Recording grant). On release the overlay closes and the peer
//! ladder runs. macOS only for now; other platforms return an error and
//! keep the by-name path (pair_app) working.

use serde::Serialize;
use std::sync::atomic::{AtomicBool, Ordering};

pub const OVERLAY_LABEL: &str = "pair-overlay";
static ACTIVE: AtomicBool = AtomicBool::new(false);

/// The window under the cursor, in overlay-local logical coordinates.
#[derive(Debug, Clone, Serialize)]
pub struct DragTarget {
    pub pid: i64,
    pub name: String,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

#[derive(Debug, Clone, Serialize)]
struct DragFrame {
    x: f64,
    y: f64,
    ox: f64,
    oy: f64,
    target: Option<DragTarget>,
    active: bool,
}

pub fn cancel() {
    ACTIVE.store(false, Ordering::SeqCst);
}

#[cfg(target_os = "macos")]
mod cg {
    use core_foundation::array::{CFArray, CFArrayRef};
    use core_foundation::base::{CFType, TCFType};
    use core_foundation::dictionary::CFDictionary;
    use core_foundation::number::CFNumber;
    use core_foundation::string::CFString;
    use std::ffi::c_void;

    #[repr(C)]
    #[derive(Clone, Copy, Debug, Default)]
    pub struct CGPoint {
        pub x: f64,
        pub y: f64,
    }
    #[repr(C)]
    #[derive(Clone, Copy, Debug, Default)]
    pub struct CGSize {
        pub width: f64,
        pub height: f64,
    }
    #[repr(C)]
    #[derive(Clone, Copy, Debug, Default)]
    pub struct CGRect {
        pub origin: CGPoint,
        pub size: CGSize,
    }

    const ON_SCREEN_ONLY: u32 = 1 << 0;
    const EXCLUDE_DESKTOP_ELEMENTS: u32 = 1 << 4;
    const NULL_WINDOW_ID: u32 = 0;
    const COMBINED_SESSION_STATE: i32 = 0;
    const MOUSE_BUTTON_LEFT: u32 = 0;

    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        fn CGEventCreate(source: *const c_void) -> *const c_void;
        fn CGEventGetLocation(event: *const c_void) -> CGPoint;
        fn CGEventSourceButtonState(state_id: i32, button: u32) -> bool;
        fn CGWindowListCopyWindowInfo(option: u32, relative_to_window: u32) -> CFArrayRef;
        fn CGRectMakeWithDictionaryRepresentation(dict: *const c_void, rect: *mut CGRect) -> bool;
    }
    extern "C" {
        fn CFRelease(cf: *const c_void);
    }

    /// Cursor position in global display points (origin: main display's
    /// top-left, y down). No permission needed.
    pub fn cursor() -> (f64, f64) {
        unsafe {
            let ev = CGEventCreate(std::ptr::null());
            if ev.is_null() {
                return (0.0, 0.0);
            }
            let p = CGEventGetLocation(ev);
            CFRelease(ev);
            (p.x, p.y)
        }
    }

    pub fn left_button_down() -> bool {
        unsafe { CGEventSourceButtonState(COMBINED_SESSION_STATE, MOUSE_BUTTON_LEFT) }
    }

    pub struct WindowHit {
        pub pid: i64,
        pub owner: String,
        pub x: f64,
        pub y: f64,
        pub w: f64,
        pub h: f64,
    }

    /// Topmost normal-layer window under a point, skipping our own pid.
    pub fn window_at(x: f64, y: f64, exclude_pid: i64) -> Option<WindowHit> {
        let raw = unsafe { CGWindowListCopyWindowInfo(ON_SCREEN_ONLY | EXCLUDE_DESKTOP_ELEMENTS, NULL_WINDOW_ID) };
        if raw.is_null() {
            return None;
        }
        let arr: CFArray<CFDictionary<CFString, CFType>> = unsafe { CFArray::wrap_under_create_rule(raw) };
        for dict in arr.iter() {
            let num_i64 = |k: &str| {
                dict.find(CFString::new(k))
                    .and_then(|v| v.downcast::<CFNumber>())
                    .and_then(|n| n.to_i64())
            };
            let num_f64 = |k: &str| {
                dict.find(CFString::new(k))
                    .and_then(|v| v.downcast::<CFNumber>())
                    .and_then(|n| n.to_f64())
            };
            if num_i64("kCGWindowLayer").unwrap_or(0) != 0 {
                continue;
            }
            let pid = num_i64("kCGWindowOwnerPID").unwrap_or(0);
            if pid == 0 || pid == exclude_pid {
                continue;
            }
            if num_f64("kCGWindowAlpha").unwrap_or(1.0) <= 0.05 {
                continue;
            }
            let Some(bounds) = dict.find(CFString::new("kCGWindowBounds")) else { continue };
            let mut rect = CGRect::default();
            let ok = unsafe { CGRectMakeWithDictionaryRepresentation(bounds.as_CFTypeRef(), &mut rect) };
            if !ok || rect.size.width < 60.0 || rect.size.height < 40.0 {
                continue;
            }
            let inside = x >= rect.origin.x
                && x <= rect.origin.x + rect.size.width
                && y >= rect.origin.y
                && y <= rect.origin.y + rect.size.height;
            if !inside {
                continue;
            }
            let owner = dict
                .find(CFString::new("kCGWindowOwnerName"))
                .and_then(|v| v.downcast::<CFString>())
                .map(|s| s.to_string())
                .unwrap_or_default();
            return Some(WindowHit {
                pid,
                owner,
                x: rect.origin.x,
                y: rect.origin.y,
                w: rect.size.width,
                h: rect.size.height,
            });
        }
        None
    }
}

/// Start a drag. Returns as soon as the overlay is requested; the drag
/// itself runs in a background task until the mouse button is released
/// (or `cancel()`), then pairs with whatever window it landed on.
/// `origin` is where the wire starts, in global logical points: the port
/// the window drew on its glow. Without it, the cursor position at press.
#[cfg(target_os = "macos")]
pub fn begin(app: tauri::AppHandle, origin: Option<(f64, f64)>) -> Result<(), String> {
    use tauri::Manager;

    if ACTIVE.swap(true, Ordering::SeqCst) {
        return Err("a pairing drag is already in progress".to_string());
    }
    let (ox, oy) = origin.unwrap_or_else(cg::cursor);

    // The overlay covers the monitor the drag starts on. Logical units.
    let main = app.get_webview_window("main");
    let monitor = main
        .as_ref()
        .and_then(|w| w.monitor_from_point(ox, oy).ok().flatten())
        .or_else(|| main.as_ref().and_then(|w| w.primary_monitor().ok().flatten()));
    let (mx, my, mw, mh) = match monitor {
        Some(m) => {
            let s = m.scale_factor();
            (
                m.position().x as f64 / s,
                m.position().y as f64 / s,
                m.size().width as f64 / s,
                m.size().height as f64 / s,
            )
        }
        None => (0.0, 0.0, 1920.0, 1200.0),
    };

    {
        let app = app.clone();
        let _ = app.clone().run_on_main_thread(move || {
            use tauri::{WebviewUrl, WebviewWindowBuilder};
            if let Some(old) = app.get_webview_window(OVERLAY_LABEL) {
                let _ = old.close();
            }
            let built = WebviewWindowBuilder::new(
                &app,
                OVERLAY_LABEL,
                WebviewUrl::App("index.html?window=pair-overlay".into()),
            )
            .title("Pair")
            .position(mx, my)
            .inner_size(mw, mh)
            .decorations(false)
            .transparent(true)
            .always_on_top(true)
            .focused(false)
            .shadow(false)
            .skip_taskbar(true)
            .visible_on_all_workspaces(true)
            .resizable(false)
            .accept_first_mouse(false)
            .build();
            match built {
                Ok(w) => {
                    let _ = w.set_ignore_cursor_events(true);
                }
                Err(e) => eprintln!("[pair] could not open the drag overlay: {}", e),
            }
        });
    }

    tauri::async_runtime::spawn(async move {
        use tauri::Emitter;
        let own_pid = std::process::id() as i64;
        let mut released_on: Option<cg::WindowHit> = None;
        let mut cancelled = false;
        // Give the overlay a moment to exist before the first frame.
        tokio::time::sleep(std::time::Duration::from_millis(60)).await;
        loop {
            if !ACTIVE.load(Ordering::SeqCst) {
                cancelled = true;
                break;
            }
            let (x, y) = cg::cursor();
            let down = cg::left_button_down();
            let hit = cg::window_at(x, y, own_pid);
            let frame = DragFrame {
                x: x - mx,
                y: y - my,
                ox: ox - mx,
                oy: oy - my,
                target: hit.as_ref().map(|h| DragTarget {
                    pid: h.pid,
                    name: h.owner.clone(),
                    x: h.x - mx,
                    y: h.y - my,
                    w: h.w,
                    h: h.h,
                }),
                active: down,
            };
            let _ = app.emit_to(OVERLAY_LABEL, "pair-drag", &frame);
            if !down {
                released_on = hit;
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(16)).await;
        }
        ACTIVE.store(false, Ordering::SeqCst);
        // Let the overlay play its release frame, then take it down.
        tokio::time::sleep(std::time::Duration::from_millis(220)).await;
        {
            let app = app.clone();
            let _ = app.clone().run_on_main_thread(move || {
                if let Some(w) = app.get_webview_window(OVERLAY_LABEL) {
                    let _ = w.close();
                }
            });
        }
        match (cancelled, released_on) {
            (false, Some(hit)) => {
                crate::peer::pair_pid_and_announce(app, hit.pid, &hit.owner).await;
            }
            _ => {
                crate::peer::emit_event(&app, "cancelled", "Pairing cancelled.", None);
            }
        }
    });
    Ok(())
}

#[cfg(not(target_os = "macos"))]
pub fn begin(_app: tauri::AppHandle, _origin: Option<(f64, f64)>) -> Result<(), String> {
    Err("drag-to-pair is macOS only for now. Say \"pair with <app name>\" instead.".to_string())
}
