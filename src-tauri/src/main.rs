#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod engine {
    pub mod matcher;
}
mod storage {
    pub mod crypto;
    pub mod db;
}

use engine::matcher::{
    scan_system_applications, search_items, search_spotlight_files, SearchResultItem,
    SearchableItem,
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use storage::crypto::{
    decrypt_aes256_gcm, derive_key_argon2, encrypt_aes256_gcm, generate_random_bytes,
    generate_totp_code,
};
use storage::db::AppDatabase;
use tauri::{AppHandle, Manager, State};

#[cfg(target_os = "macos")]
static LAST_ACTIVE_DISPLAY_ID: AtomicU32 = AtomicU32::new(0);
#[cfg(target_os = "macos")]
static IS_PINNED_ON_TOP: AtomicBool = AtomicBool::new(false);
#[cfg(target_os = "macos")]
static HIDE_ON_BLUR_ENABLED: AtomicBool = AtomicBool::new(true);
#[cfg(target_os = "macos")]
static IS_DETACHED_WINDOW: AtomicBool = AtomicBool::new(false);
#[cfg(target_os = "macos")]
static DETACHED_WINDOW_COUNT: AtomicU32 = AtomicU32::new(0);
#[cfg(target_os = "macos")]
static SUPER_PANEL_ENABLED: AtomicBool = AtomicBool::new(true);
#[cfg(target_os = "macos")]
static SUPER_PANEL_HOLD_MS: AtomicU64 = AtomicU64::new(250);
#[cfg(target_os = "macos")]
static SUPER_PANEL_TRIGGER_MODE: AtomicU32 = AtomicU32::new(0); // 0: 右键长按, 1: 中键长按, 2: Ctrl+右键
#[cfg(target_os = "macos")]
static PRE_MAXIMIZED_FRAMES: std::sync::OnceLock<
    Mutex<std::collections::HashMap<String, CGRect>>,
> = std::sync::OnceLock::new();

#[cfg(target_os = "macos")]
fn pre_maximized_frames_map() -> &'static Mutex<std::collections::HashMap<String, CGRect>> {
    PRE_MAXIMIZED_FRAMES.get_or_init(|| Mutex::new(std::collections::HashMap::new()))
}

static LAST_WINDOW_HIDDEN_AT_MS: AtomicU64 = AtomicU64::new(0);
static LAST_CLIPBOARD_CHANGE_COUNT: AtomicI64 = AtomicI64::new(-1);
static LAST_CLIPBOARD_COPY_AT_MS: AtomicU64 = AtomicU64::new(0);

fn now_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[cfg(target_os = "macos")]
#[repr(C)]
#[derive(Copy, Clone, Debug)]
struct CGPoint {
    x: f64,
    y: f64,
}

#[cfg(target_os = "macos")]
#[repr(C)]
#[derive(Copy, Clone, Debug)]
struct CGSize {
    width: f64,
    height: f64,
}

#[cfg(target_os = "macos")]
#[repr(C)]
#[derive(Copy, Clone, Debug)]
struct CGRect {
    origin: CGPoint,
    size: CGSize,
}

#[cfg(target_os = "macos")]
#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    fn CGEventSourceFlagsState(state_id: u32) -> u64;
    fn CGEventSourceKeyState(state_id: u32, key: u16) -> bool;
    fn CGEventSourceButtonState(state_id: u32, button: u32) -> bool;
    fn CGEventSourceCounterForEventType(state_id: u32, event_type: u32) -> u32;
    fn CGEventCreate(source: *const std::ffi::c_void) -> *mut std::ffi::c_void;
    fn CGEventGetLocation(event: *const std::ffi::c_void) -> CGPoint;
    fn CFRelease(cf: *const std::ffi::c_void);
    fn CGMainDisplayID() -> u32;
    fn CGGetActiveDisplayList(
        max_displays: u32,
        active_displays: *mut u32,
        display_count: *mut u32,
    ) -> i32;
    fn CGDisplayBounds(display: u32) -> CGRect;
}

#[cfg(target_os = "macos")]
#[link(name = "objc")]
extern "C" {
    fn objc_getClass(name: *const std::os::raw::c_char) -> *mut std::ffi::c_void;
    fn sel_registerName(name: *const std::os::raw::c_char) -> *mut std::ffi::c_void;
    fn objc_msgSend();
    fn objc_allocateClassPair(
        superclass: *mut std::ffi::c_void,
        name: *const std::os::raw::c_char,
        extra_bytes: usize,
    ) -> *mut std::ffi::c_void;
    fn objc_registerClassPair(cls: *mut std::ffi::c_void);
    fn class_addIvar(
        cls: *mut std::ffi::c_void,
        name: *const std::os::raw::c_char,
        size: usize,
        alignment: u8,
        types: *const std::os::raw::c_char,
    ) -> i8;
    fn class_addMethod(
        cls: *mut std::ffi::c_void,
        name: *mut std::ffi::c_void,
        imp: *const std::ffi::c_void,
        types: *const std::os::raw::c_char,
    ) -> i8;
    fn object_getClass(obj: *mut std::ffi::c_void) -> *mut std::ffi::c_void;
    fn object_setClass(
        obj: *mut std::ffi::c_void,
        cls: *mut std::ffi::c_void,
    ) -> *mut std::ffi::c_void;
}

#[cfg(target_os = "macos")]
extern "C" fn omnibox_panel_can_become_key(
    _self: *mut std::ffi::c_void,
    _cmd: *mut std::ffi::c_void,
) -> i8 {
    1 // YES: Receives keyboard input directly
}

#[cfg(target_os = "macos")]
extern "C" fn omnibox_panel_can_become_main(
    _self: *mut std::ffi::c_void,
    _cmd: *mut std::ffi::c_void,
) -> i8 {
    0 // NO: Never steals Main Window status from the underlying Full Screen app
}

#[cfg(target_os = "macos")]
fn get_or_register_omnibox_panel_class() -> *mut std::ffi::c_void {
    unsafe {
        let existing = objc_getClass(b"OmniBoxFullscreenPanel\0".as_ptr() as _);
        if !existing.is_null() {
            return existing;
        }
        let nspanel_cls = objc_getClass(b"NSPanel\0".as_ptr() as _);
        let panel_cls =
            objc_allocateClassPair(nspanel_cls, b"OmniBoxFullscreenPanel\0".as_ptr() as _, 0);
        if !panel_cls.is_null() {
            // Preserve TaoWindow's 1-byte AtomicBool ivar layout
            let _ = class_addIvar(
                panel_cls,
                b"focusable\0".as_ptr() as _,
                1,
                0,
                b"B\0".as_ptr() as _,
            );
            let sel_key = sel_registerName(b"canBecomeKeyWindow\0".as_ptr() as _);
            let _ = class_addMethod(
                panel_cls,
                sel_key,
                omnibox_panel_can_become_key as *const std::ffi::c_void,
                b"B@:\0".as_ptr() as _,
            );
            let sel_main = sel_registerName(b"canBecomeMainWindow\0".as_ptr() as _);
            let _ = class_addMethod(
                panel_cls,
                sel_main,
                omnibox_panel_can_become_main as *const std::ffi::c_void,
                b"B@:\0".as_ptr() as _,
            );
            objc_registerClassPair(panel_cls);
        }
        panel_cls
    }
}

/// Set NSApplicationActivationPolicyAccessory (1) when no standalone detached windows exist.
/// When standalone detached windows exist, keep Regular (0) so user can switch back via Command+Tab / Dock!
#[cfg(target_os = "macos")]
fn ensure_macos_accessory_policy() {
    if DETACHED_WINDOW_COUNT.load(Ordering::Relaxed) > 0 {
        return;
    }
    use std::ffi::c_void;
    unsafe {
        let send_id: unsafe extern "C" fn(*mut c_void, *mut c_void) -> *mut c_void =
            std::mem::transmute(objc_msgSend as *const ());
        let send_isize: unsafe extern "C" fn(*mut c_void, *mut c_void) -> isize =
            std::mem::transmute(objc_msgSend as *const ());
        let send_bool_isize: unsafe extern "C" fn(*mut c_void, *mut c_void, isize) -> i8 =
            std::mem::transmute(objc_msgSend as *const ());

        let app_cls = objc_getClass(b"NSApplication\0".as_ptr() as _);
        let sel_shared = sel_registerName(b"sharedApplication\0".as_ptr() as _);
        let ns_app = send_id(app_cls, sel_shared);
        if !ns_app.is_null() {
            let sel_cur_policy = sel_registerName(b"activationPolicy\0".as_ptr() as _);
            let current_policy = send_isize(ns_app, sel_cur_policy);
            if current_policy != 1 {
                let sel_set_policy = sel_registerName(b"setActivationPolicy:\0".as_ptr() as _);
                let _ = send_bool_isize(ns_app, sel_set_policy, 1);
            }
        }
    }
}

#[cfg(target_os = "macos")]
fn set_macos_regular_policy_and_activate() {
    use std::ffi::c_void;
    unsafe {
        let send_id: unsafe extern "C" fn(*mut c_void, *mut c_void) -> *mut c_void =
            std::mem::transmute(objc_msgSend as *const ());
        let send_isize: unsafe extern "C" fn(*mut c_void, *mut c_void) -> isize =
            std::mem::transmute(objc_msgSend as *const ());
        let send_bool_isize: unsafe extern "C" fn(*mut c_void, *mut c_void, isize) -> i8 =
            std::mem::transmute(objc_msgSend as *const ());
        let send_void_bool: unsafe extern "C" fn(*mut c_void, *mut c_void, i8) =
            std::mem::transmute(objc_msgSend as *const ());

        let app_cls = objc_getClass(b"NSApplication\0".as_ptr() as _);
        let sel_shared = sel_registerName(b"sharedApplication\0".as_ptr() as _);
        let ns_app = send_id(app_cls, sel_shared);
        if !ns_app.is_null() {
            let sel_cur_policy = sel_registerName(b"activationPolicy\0".as_ptr() as _);
            let current_policy = send_isize(ns_app, sel_cur_policy);
            if current_policy != 0 {
                let sel_set_policy = sel_registerName(b"setActivationPolicy:\0".as_ptr() as _);
                let _ = send_bool_isize(ns_app, sel_set_policy, 0);
            }
            let sel_activate = sel_registerName(b"activateIgnoringOtherApps:\0".as_ptr() as _);
            send_void_bool(ns_app, sel_activate, 1);
        }
    }
}

/// Focus the inner WryWebView / WKWebView inside NSWindow's contentView (TaoView)
/// so the HTML <input> inside the webview always receives keyboard focus and shows the blinking caret!
#[cfg(target_os = "macos")]
fn focus_webview_first_responder(ns_win: *mut std::ffi::c_void) {
    use std::ffi::c_void;
    if ns_win.is_null() {
        return;
    }
    unsafe {
        let send_id: unsafe extern "C" fn(*mut c_void, *mut c_void) -> *mut c_void =
            std::mem::transmute(objc_msgSend as *const ());
        let send_usize: unsafe extern "C" fn(*mut c_void, *mut c_void) -> usize =
            std::mem::transmute(objc_msgSend as *const ());
        let send_id_usize: unsafe extern "C" fn(*mut c_void, *mut c_void, usize) -> *mut c_void =
            std::mem::transmute(objc_msgSend as *const ());
        let send_bool_id: unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void) -> i8 =
            std::mem::transmute(objc_msgSend as *const ());

        let sel_content_view = sel_registerName(b"contentView\0".as_ptr() as _);
        let content_view = send_id(ns_win, sel_content_view);
        if content_view.is_null() {
            return;
        }

        let sel_subviews = sel_registerName(b"subviews\0".as_ptr() as _);
        let sel_count = sel_registerName(b"count\0".as_ptr() as _);
        let sel_obj_at = sel_registerName(b"objectAtIndex:\0".as_ptr() as _);

        let mut target_view = content_view;
        let subviews = send_id(content_view, sel_subviews);
        if !subviews.is_null() {
            let count = send_usize(subviews, sel_count);
            if count > 0 {
                let first_sub = send_id_usize(subviews, sel_obj_at, 0);
                if !first_sub.is_null() {
                    target_view = first_sub;
                }
            }
        }

        let sel_first_resp = sel_registerName(b"firstResponder\0".as_ptr() as _);
        let current_first = send_id(ns_win, sel_first_resp);
        if current_first != target_view {
            let sel_make_first = sel_registerName(b"makeFirstResponder:\0".as_ptr() as _);
            let _ = send_bool_id(ns_win, sel_make_first, target_view);
        }
    }
}

/// Upgrade NSWindow to a true non-activating NSPanel (NSWindowStyleMaskNonactivatingPanel = 1 << 7)
/// so macOS WindowServer allows it to appear directly on top of any Full Screen Space!
#[cfg(target_os = "macos")]
fn configure_nswindow_overlay_props(ns_win: *mut std::ffi::c_void) {
    use std::ffi::c_void;
    if ns_win.is_null() {
        return;
    }
    unsafe {
        let send_usize: unsafe extern "C" fn(*mut c_void, *mut c_void) -> usize =
            std::mem::transmute(objc_msgSend as *const ());
        let send_void_usize: unsafe extern "C" fn(*mut c_void, *mut c_void, usize) =
            std::mem::transmute(objc_msgSend as *const ());
        let send_void_isize: unsafe extern "C" fn(*mut c_void, *mut c_void, isize) =
            std::mem::transmute(objc_msgSend as *const ());
        let send_void_bool: unsafe extern "C" fn(*mut c_void, *mut c_void, i8) =
            std::mem::transmute(objc_msgSend as *const ());

        // 1. Dynamically swizzle ISA from TaoWindow (NSWindow) -> OmniBoxFullscreenPanel (NSPanel)
        let panel_cls = get_or_register_omnibox_panel_class();
        if !panel_cls.is_null() && object_getClass(ns_win) != panel_cls {
            let _ = object_setClass(ns_win, panel_cls);
        }

        // 2. Apply NSWindowStyleMaskNonactivatingPanel (1 << 7) | NSWindowStyleMaskResizable (1 << 3)
        let sel_get_mask = sel_registerName(b"styleMask\0".as_ptr() as _);
        let cur_mask = send_usize(ns_win, sel_get_mask);
        let nonactivating_bit: usize = 1 << 7;
        if (cur_mask & nonactivating_bit) == 0 {
            let sel_set_mask = sel_registerName(b"setStyleMask:\0".as_ptr() as _);
            send_void_usize(ns_win, sel_set_mask, cur_mask | nonactivating_bit | (1 << 3));
        }

        // 3. Configure NSPanel floating & key behavior
        let sel_floating = sel_registerName(b"setFloatingPanel:\0".as_ptr() as _);
        send_void_bool(ns_win, sel_floating, 1);

        let sel_becomes_key = sel_registerName(b"setBecomesKeyOnlyIfNeeded:\0".as_ptr() as _);
        send_void_bool(ns_win, sel_becomes_key, 0);

        let sel_works_modal = sel_registerName(b"setWorksWhenModal:\0".as_ptr() as _);
        send_void_bool(ns_win, sel_works_modal, 1);

        let sel_hides = sel_registerName(b"setHidesOnDeactivate:\0".as_ptr() as _);
        send_void_bool(ns_win, sel_hides, 0);

        // 4. NSWindowCollectionBehaviorCanJoinAllSpaces (1 << 0)
        //    | NSWindowCollectionBehaviorTransient (1 << 3)
        //    | NSWindowCollectionBehaviorIgnoresCycle (1 << 6)
        //    | NSWindowCollectionBehaviorFullScreenAuxiliary (1 << 8)
        let behavior: usize = (1 << 0) | (1 << 3) | (1 << 6) | (1 << 8);
        let sel_behavior = sel_registerName(b"setCollectionBehavior:\0".as_ptr() as _);
        send_void_usize(ns_win, sel_behavior, behavior);

        // 5. NSPopUpMenuWindowLevel (101) — floats directly above full-screen spaces
        let sel_level = sel_registerName(b"setLevel:\0".as_ptr() as _);
        send_void_isize(ns_win, sel_level, 101);

        focus_webview_first_responder(ns_win);
    }
}

/// Find the display ID and CoreGraphics bounds (top-left origin in points) where the mouse cursor currently resides.
#[cfg(target_os = "macos")]
fn get_mouse_display_bounds() -> (u32, CGRect, f64) {
    unsafe {
        let main_id = CGMainDisplayID();
        let main_bounds = CGDisplayBounds(main_id);
        let main_h = main_bounds.size.height;

        let ev = CGEventCreate(std::ptr::null());
        if ev.is_null() {
            return (main_id, main_bounds, main_h);
        }
        let mouse_pt = CGEventGetLocation(ev);
        CFRelease(ev);

        let mut displays = [0u32; 16];
        let mut count: u32 = 0;
        if CGGetActiveDisplayList(16, displays.as_mut_ptr(), &mut count) == 0 {
            for i in 0..(count as usize) {
                let d_id = displays[i];
                let b = CGDisplayBounds(d_id);
                if mouse_pt.x >= b.origin.x
                    && mouse_pt.x < b.origin.x + b.size.width
                    && mouse_pt.y >= b.origin.y
                    && mouse_pt.y < b.origin.y + b.size.height
                {
                    return (d_id, b, main_h);
                }
            }
        }
        (main_id, main_bounds, main_h)
    }
}

/// Move NSWindow to the golden spot (centered horizontally, 20% from top) of the screen where the mouse currently is.
#[cfg(target_os = "macos")]
fn place_nswindow_on_active_mouse_screen(ns_win: *mut std::ffi::c_void, reset_compact: bool) -> u32 {
    use std::ffi::c_void;
    let (display_id, bounds, main_h) = get_mouse_display_bounds();
    if ns_win.is_null() {
        return display_id;
    }
    unsafe {
        let send_rect: unsafe extern "C" fn(*mut c_void, *mut c_void) -> CGRect =
            std::mem::transmute(objc_msgSend as *const ());
        let send_set_frame: unsafe extern "C" fn(*mut c_void, *mut c_void, CGRect, i8) =
            std::mem::transmute(objc_msgSend as *const ());

        let sel_frame = sel_registerName(b"frame\0".as_ptr() as _);
        let cur_frame = send_rect(ns_win, sel_frame);
        let win_w = if reset_compact {
            860.0
        } else if cur_frame.size.width > 100.0 {
            cur_frame.size.width
        } else {
            860.0
        };
        let win_h = if reset_compact {
            172.0
        } else if cur_frame.size.height > 40.0 {
            cur_frame.size.height
        } else {
            172.0
        };

        // CoreGraphics top-left coordinate on the target monitor
        let cg_x = bounds.origin.x + (bounds.size.width - win_w) / 2.0;
        let cg_top_y = bounds.origin.y + bounds.size.height * 0.20;

        // Convert to Cocoa bottom-left origin coordinate (main screen bottom-left is 0,0)
        let cocoa_x = cg_x.round();
        let cocoa_y = (main_h - cg_top_y - win_h).round();

        let new_frame = CGRect {
            origin: CGPoint {
                x: cocoa_x,
                y: cocoa_y,
            },
            size: CGSize {
                width: win_w,
                height: win_h,
            },
        };
        let sel_set_frame = sel_registerName(b"setFrame:display:\0".as_ptr() as _);
        send_set_frame(ns_win, sel_set_frame, new_frame, 1);
    }
    LAST_ACTIVE_DISPLAY_ID.store(display_id, Ordering::Relaxed);
    display_id
}

#[cfg(target_os = "macos")]
fn hide_nswindow_native(ns_win: *mut std::ffi::c_void) {
    use std::ffi::c_void;
    LAST_WINDOW_HIDDEN_AT_MS.store(now_unix_ms(), Ordering::Relaxed);
    if ns_win.is_null() {
        return;
    }
    unsafe {
        let send_void_ptr: unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void) =
            std::mem::transmute(objc_msgSend as *const ());
        let sel_order_out = sel_registerName(b"orderOut:\0".as_ptr() as _);
        send_void_ptr(ns_win, sel_order_out, std::ptr::null_mut());
    }
}

#[cfg(target_os = "macos")]
fn set_nswindow_alpha(ns_win: *mut std::ffi::c_void, opacity_percent: f64) {
    use std::ffi::c_void;
    if ns_win.is_null() {
        return;
    }
    let alpha = (opacity_percent / 100.0).clamp(0.75, 1.0);
    unsafe {
        let send_void_f64: unsafe extern "C" fn(*mut c_void, *mut c_void, f64) =
            std::mem::transmute(objc_msgSend as *const ());
        let sel_alpha = sel_registerName(b"setAlphaValue:\0".as_ptr() as _);
        send_void_f64(ns_win, sel_alpha, alpha);
    }
}

/// Poll `NSPasteboard.generalPasteboard.changeCount` (sub-microsecond) to record the exact timestamp
/// whenever the user copies new text to the macOS clipboard!
#[cfg(target_os = "macos")]
fn check_macos_pasteboard_change() {
    use std::ffi::c_void;
    unsafe {
        let cls = objc_getClass(b"NSPasteboard\0".as_ptr() as _);
        if cls.is_null() {
            return;
        }
        let send_id: unsafe extern "C" fn(*mut c_void, *mut c_void) -> *mut c_void =
            std::mem::transmute(objc_msgSend as *const ());
        let send_isize: unsafe extern "C" fn(*mut c_void, *mut c_void) -> isize =
            std::mem::transmute(objc_msgSend as *const ());

        let sel_gen = sel_registerName(b"generalPasteboard\0".as_ptr() as _);
        let pb = send_id(cls, sel_gen);
        if pb.is_null() {
            return;
        }
        let sel_cnt = sel_registerName(b"changeCount\0".as_ptr() as _);
        let count = send_isize(pb, sel_cnt) as i64;
        let prev = LAST_CLIPBOARD_CHANGE_COUNT.swap(count, Ordering::Relaxed);
        if prev != -1 && prev != count {
            LAST_CLIPBOARD_COPY_AT_MS.store(now_unix_ms(), Ordering::Relaxed);
        }
    }
}

#[cfg(target_os = "macos")]
fn read_macos_clipboard_text() -> String {
    if let Ok(out) = Command::new("pbpaste").output() {
        let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if text.chars().count() > 3000 {
            return text.chars().take(3000).collect();
        }
        return text;
    }
    String::new()
}

#[cfg(target_os = "macos")]
fn apply_macos_launch_at_login(enabled: bool) -> Result<(), String> {
    let home = dirs::home_dir().ok_or("Cannot resolve home directory")?;
    let agents_dir = home.join("Library").join("LaunchAgents");
    let plist_path = agents_dir.join("com.omnibox.launcher.plist");

    if enabled {
        let exe_path = std::env::current_exe().map_err(|e| e.to_string())?;
        let _ = std::fs::create_dir_all(&agents_dir);
        let plist_xml = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>com.omnibox.launcher</string>
    <key>ProgramArguments</key>
    <array>
        <string>{}</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>ProcessType</key>
    <string>Interactive</string>
</dict>
</plist>
"#,
            exe_path.to_string_lossy()
        );
        std::fs::write(&plist_path, plist_xml).map_err(|e| e.to_string())?;
    } else if plist_path.exists() {
        let _ = std::fs::remove_file(&plist_path);
    }
    Ok(())
}

/// Convert key token (e.g. "Space", "K", "F", "1") into macOS CGKeyCode (`kVK_*`)
#[cfg(target_os = "macos")]
fn hotkey_token_to_keycode(token: &str) -> Option<u16> {
    match token.trim().to_uppercase().as_str() {
        "SPACE" => Some(49),
        "A" => Some(0),
        "S" => Some(1),
        "D" => Some(2),
        "F" => Some(3),
        "H" => Some(4),
        "G" => Some(5),
        "Z" => Some(6),
        "X" => Some(7),
        "C" => Some(8),
        "V" => Some(9),
        "B" => Some(11),
        "Q" => Some(12),
        "W" => Some(13),
        "E" => Some(14),
        "R" => Some(15),
        "Y" => Some(16),
        "T" => Some(17),
        "1" => Some(18),
        "2" => Some(19),
        "3" => Some(20),
        "4" => Some(21),
        "6" => Some(22),
        "5" => Some(23),
        "=" => Some(24),
        "9" => Some(25),
        "7" => Some(26),
        "-" => Some(27),
        "8" => Some(28),
        "0" => Some(29),
        "]" => Some(30),
        "O" => Some(31),
        "U" => Some(32),
        "[" => Some(33),
        "I" => Some(34),
        "P" => Some(35),
        "RETURN" | "ENTER" => Some(36),
        "L" => Some(37),
        "J" => Some(38),
        "'" => Some(39),
        "K" => Some(40),
        ";" => Some(41),
        "\\" => Some(42),
        "," => Some(43),
        "/" => Some(44),
        "N" => Some(45),
        "M" => Some(46),
        "." => Some(47),
        "TAB" => Some(48),
        "`" | "BACKQUOTE" => Some(50),
        "F1" => Some(122),
        "F2" => Some(120),
        "F3" => Some(99),
        "F4" => Some(118),
        "F5" => Some(96),
        "F6" => Some(97),
        "F7" => Some(98),
        "F8" => Some(100),
        "F9" => Some(101),
        "F10" => Some(109),
        "F11" => Some(103),
        "F12" => Some(111),
        _ => None,
    }
}

/// Summon OmniBox NSPanel directly above the currently active Full Screen app AND on the monitor where the mouse is!
#[cfg(target_os = "macos")]
fn summon_macos_overlay_on_current_space_and_screen(win: &tauri::WebviewWindow) {
    use std::ffi::c_void;
    ensure_macos_accessory_policy();
    if let Ok(ns_win) = win.ns_window() {
        if ns_win.is_null() {
            return;
        }
        unsafe {
            let send_void: unsafe extern "C" fn(*mut c_void, *mut c_void) =
                std::mem::transmute(objc_msgSend as *const ());
            let send_void_ptr: unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void) =
                std::mem::transmute(objc_msgSend as *const ());

            // Step 1: Ensure NSPanel class + NonactivatingPanel styleMask + FullScreenAuxiliary
            configure_nswindow_overlay_props(ns_win);

            // Step 2: Move panel frame to the monitor where the mouse cursor is currently located (Screen 1, 2, or 3)
            place_nswindow_on_active_mouse_screen(ns_win, true);

            // Step 3: Make Non-activating NSPanel key and order front WITHOUT calling [NSApp activateIgnoringOtherApps:YES]!
            // Calling activateIgnoringOtherApps forces macOS out of the Full Screen Space;
            // a Non-activating NSPanel becomes Key Window directly on top of the active Full Screen app!
            let sel_order_front = sel_registerName(b"orderFrontRegardless\0".as_ptr() as _);
            send_void(ns_win, sel_order_front);

            let sel_key_front = sel_registerName(b"makeKeyAndOrderFront:\0".as_ptr() as _);
            send_void_ptr(ns_win, sel_key_front, std::ptr::null_mut());

            let sel_make_key = sel_registerName(b"makeKeyWindow\0".as_ptr() as _);
            send_void(ns_win, sel_make_key);

            // Step 4: Ensure the inner WryWebView (WKWebView) is the firstResponder of the panel
            focus_webview_first_responder(ns_win);
        }
    }
}

pub struct AppState {
    pub db: Mutex<AppDatabase>,
    pub items: Mutex<Vec<SearchableItem>>,
    pub sys_apps: Mutex<Vec<SearchableItem>>,
    pub icon_cache_dir: PathBuf,
    pub global_hotkey: Arc<Mutex<String>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TotpCodeResponse {
    pub code: String,
    pub remaining: u64,
}

fn build_core_items(sys_apps: &[SearchableItem]) -> Vec<SearchableItem> {
    let mut list = vec![
        SearchableItem::new(
            "plugin:calc-pad",
            "计算稿纸 (Calc Pad)",
            "支持多行变量赋值、单位换算、进制转换与实时聚合求和的智能计算笔记",
            "plugin",
            "Calculator",
            None,
            vec!["calc", "math", "numi", "计算稿纸", "计算器", "单位换算", "稿纸"]
                .into_iter()
                .map(String::from)
                .collect(),
            "calc-pad",
        ),
        SearchableItem::new(
            "plugin:json-studio",
            "JSON 助手 (JSON Studio)",
            "极速 JSON 格式化、压缩、转义、JSONPath 实时过滤与 TS/Rust/Go 结构体生成",
            "plugin",
            "Braces",
            None,
            vec!["json", "format", "jsonpath", "struct", "格式化", "转义"]
                .into_iter()
                .map(String::from)
                .collect(),
            "json-studio",
        ),
        SearchableItem::new(
            "plugin:diff-viewer",
            "文本对比 (Diff Viewer)",
            "左右双栏与行内字符级代码/文本差异对比工具，支持忽略空白与差异块定位",
            "plugin",
            "GitCompare",
            None,
            vec!["diff", "compare", "文本对比", "代码对比", "差异"]
                .into_iter()
                .map(String::from)
                .collect(),
            "diff-viewer",
        ),
        SearchableItem::new(
            "plugin:authenticator",
            "2FA 验证码管理 (Authenticator)",
            "RFC 6238 TOTP 双重认证动态码管理器，支持 otpauth 导入与主搜索框直达复制",
            "plugin",
            "ShieldCheck",
            None,
            vec!["2fa", "totp", "otp", "auth", "验证码", "双重认证", "令牌"]
                .into_iter()
                .map(String::from)
                .collect(),
            "authenticator",
        ),
        SearchableItem::new(
            "plugin:password-vault",
            "密码本 (Password Vault)",
            "基于 Argon2id + AES-256-GCM 硬件级加密的本地密码保险箱与强密码生成器",
            "plugin",
            "KeyRound",
            None,
            vec!["password", "vault", "key", "密码本", "保险箱", "账号"]
                .into_iter()
                .map(String::from)
                .collect(),
            "password-vault",
        ),
        SearchableItem::new(
            "plugin:utools-hub",
            "uTools 插件中心与沙箱挂载",
            "突破 10 个应用限制，无限挂载本地与开源 uTools 生态插件 (plugin.json)",
            "plugin",
            "Blocks",
            None,
            vec!["utools", "rubick", "plugin", "插件中心", "扩展"]
                .into_iter()
                .map(String::from)
                .collect(),
            "utools-hub",
        ),
        SearchableItem::new(
            "plugin:user-center",
            "个人中心与偏好设置 (User Center)",
            "管理本机聚焦应用索引、全局快捷键、自定义别名、WebDAV 同步、安全策略与主题外观",
            "plugin",
            "SlidersHorizontal",
            None,
            vec![
                "settings",
                "profile",
                "preferences",
                "个人中心",
                "设置",
                "同步",
                "快捷键",
                "grzx",
                "sz",
            ]
            .into_iter()
            .map(String::from)
            .collect(),
            "user-center",
        ),
    ];

    list.extend_from_slice(sys_apps);
    list
}

#[tauri::command]
fn search_query(query: String, state: State<'_, AppState>) -> Vec<SearchResultItem> {
    let items = state.items.lock().unwrap();
    let frecency = state
        .db
        .lock()
        .map(|db| db.get_frecency_map())
        .unwrap_or_default();
    search_items(&items, &query, &frecency)
}

#[tauri::command]
fn get_system_apps(state: State<'_, AppState>) -> Vec<SearchableItem> {
    state.sys_apps.lock().unwrap().clone()
}

#[tauri::command]
async fn rescan_system_apps(
    clear_icon_cache: bool,
    state: State<'_, AppState>,
) -> Result<Vec<SearchableItem>, String> {
    let cache_dir = state.icon_cache_dir.clone();
    let fresh_apps = tauri::async_runtime::spawn_blocking(move || {
        if clear_icon_cache && cache_dir.exists() {
            let _ = std::fs::remove_dir_all(&cache_dir);
        }
        scan_system_applications(&cache_dir)
    })
    .await
    .map_err(|e| e.to_string())?;

    let combined = build_core_items(&fresh_apps);
    *state.sys_apps.lock().unwrap() = fresh_apps.clone();
    *state.items.lock().unwrap() = combined;
    Ok(fresh_apps)
}

#[tauri::command]
async fn search_local_files(query: String) -> Result<Vec<SearchResultItem>, String> {
    tauri::async_runtime::spawn_blocking(move || search_spotlight_files(&query))
        .await
        .map_err(|e| e.to_string())
}

/// Find the display ID and CoreGraphics bounds where the given Cocoa window frame is currently located.
#[cfg(target_os = "macos")]
fn get_window_display_bounds(cocoa_frame: CGRect) -> (u32, CGRect, f64) {
    unsafe {
        let main_id = CGMainDisplayID();
        let main_bounds = CGDisplayBounds(main_id);
        let main_h = main_bounds.size.height;

        let center_x = cocoa_frame.origin.x + cocoa_frame.size.width / 2.0;
        let center_y = main_h - (cocoa_frame.origin.y + cocoa_frame.size.height / 2.0);

        let mut displays = [0u32; 16];
        let mut count: u32 = 0;
        if CGGetActiveDisplayList(16, displays.as_mut_ptr(), &mut count) == 0 {
            for i in 0..(count as usize) {
                let d_id = displays[i];
                let b = CGDisplayBounds(d_id);
                if center_x >= b.origin.x
                    && center_x < b.origin.x + b.size.width
                    && center_y >= b.origin.y
                    && center_y < b.origin.y + b.size.height
                {
                    return (d_id, b, main_h);
                }
            }
        }
        get_mouse_display_bounds()
    }
}

#[tauri::command]
fn resize_window(
    height: f64,
    width: Option<f64>,
    _resizable: Option<bool>,
    init_position: Option<bool>,
    maximized: Option<bool>,
    window: tauri::Window,
) -> Result<(), String> {
    let target_width = width.unwrap_or(860.0);
    let is_maximized = maximized.unwrap_or(false);

    #[cfg(target_os = "macos")]
    {
        use std::ffi::c_void;
        if let Ok(ns_win) = window.ns_window() {
            if !ns_win.is_null() {
                unsafe {
                    let send_rect: unsafe extern "C" fn(*mut c_void, *mut c_void) -> CGRect =
                        std::mem::transmute(objc_msgSend as *const ());
                    let send_set_frame: unsafe extern "C" fn(
                        *mut c_void,
                        *mut c_void,
                        CGRect,
                        i8,
                    ) = std::mem::transmute(objc_msgSend as *const ());

                    let sel_frame = sel_registerName(b"frame\0".as_ptr() as _);
                    let sel_set_frame = sel_registerName(b"setFrame:display:\0".as_ptr() as _);
                    let old_frame = send_rect(ns_win, sel_frame);
                    let label = window.label().to_string();
                    let is_vis = if window.is_visible().unwrap_or(false) {
                        1
                    } else {
                        0
                    };

                    if init_position.unwrap_or(false) {
                        if let Ok(mut map) = pre_maximized_frames_map().lock() {
                            map.remove(&label);
                        }
                        let (display_id, bounds, main_h) = get_mouse_display_bounds();
                        let cg_x = bounds.origin.x + (bounds.size.width - target_width) / 2.0;
                        let cg_top_y = bounds.origin.y + bounds.size.height * 0.20;
                        let new_frame = CGRect {
                            origin: CGPoint {
                                x: cg_x.round(),
                                y: (main_h - cg_top_y - height).round(),
                            },
                            size: CGSize {
                                width: target_width,
                                height,
                            },
                        };
                        send_set_frame(ns_win, sel_set_frame, new_frame, 1);
                        if label == "main" {
                            configure_nswindow_overlay_props(ns_win);
                            LAST_ACTIVE_DISPLAY_ID.store(display_id, Ordering::Relaxed);
                        }
                    } else if is_maximized {
                        // Save pre-maximized compact frame once before expanding to screen-adaptive 86% x 84%
                        if let Ok(mut map) = pre_maximized_frames_map().lock() {
                            map.entry(label.clone()).or_insert(old_frame);
                        }
                        let (_, bounds, main_h) = get_window_display_bounds(old_frame);
                        let max_w = (bounds.size.width * 0.86)
                            .clamp(1120.0, (bounds.size.width - 48.0).max(860.0))
                            .round();
                        let max_h = (bounds.size.height * 0.84)
                            .clamp(720.0, (bounds.size.height - 72.0).max(580.0))
                            .round();

                        let cg_x = bounds.origin.x + (bounds.size.width - max_w) / 2.0;
                        let cg_top_y = bounds.origin.y + (bounds.size.height - max_h) * 0.46;
                        let new_frame = CGRect {
                            origin: CGPoint {
                                x: cg_x.round(),
                                y: (main_h - cg_top_y - max_h).round(),
                            },
                            size: CGSize {
                                width: max_w,
                                height: max_h,
                            },
                        };
                        send_set_frame(ns_win, sel_set_frame, new_frame, is_vis);
                    } else {
                        // Check if restoring from maximized state back to compact size & original position
                        let restored_from = pre_maximized_frames_map()
                            .lock()
                            .ok()
                            .and_then(|mut map| map.remove(&label));

                        if let Some(saved) = restored_from {
                            let saved_top_y = saved.origin.y + saved.size.height;
                            let new_x = if (saved.size.width - target_width).abs() <= 4.0 {
                                saved.origin.x
                            } else {
                                (saved.origin.x + (saved.size.width - target_width) / 2.0).round()
                            };
                            let new_y = (saved_top_y - height).round();
                            let new_frame = CGRect {
                                origin: CGPoint { x: new_x, y: new_y },
                                size: CGSize {
                                    width: target_width,
                                    height,
                                },
                            };
                            send_set_frame(ns_win, sel_set_frame, new_frame, is_vis);
                        } else if (old_frame.size.width - target_width).abs() > 0.5
                            || (old_frame.size.height - height).abs() > 0.5
                        {
                            // Normal height/width transition while keeping top edge anchored
                            let top_y = old_frame.origin.y + old_frame.size.height;
                            let new_x = if (old_frame.size.width - target_width).abs() <= 2.0 {
                                old_frame.origin.x
                            } else {
                                (old_frame.origin.x + (old_frame.size.width - target_width) / 2.0)
                                    .round()
                            };
                            let new_y = (top_y - height).round();
                            let new_frame = CGRect {
                                origin: CGPoint { x: new_x, y: new_y },
                                size: CGSize {
                                    width: target_width,
                                    height,
                                },
                            };
                            send_set_frame(ns_win, sel_set_frame, new_frame, is_vis);
                        }
                    }
                }
                return Ok(());
            }
        }
    }

    #[cfg(not(target_os = "macos"))]
    {
        let scale = window.scale_factor().unwrap_or(2.0);
        let new_phys_w = (target_width * scale).round() as u32;
        let new_phys_h = (height * scale).round() as u32;
        window
            .set_size(tauri::Size::Physical(tauri::PhysicalSize {
                width: new_phys_w,
                height: new_phys_h,
            }))
            .map_err(|e| e.to_string())?;
    }

    Ok(())
}

#[tauri::command]
fn start_window_drag(window: tauri::Window) -> Result<(), String> {
    window.start_dragging().map_err(|e| e.to_string())
}

#[tauri::command]
fn minimize_window(window: tauri::Window) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    if let Ok(ns_win) = window.ns_window() {
        if !ns_win.is_null() {
            unsafe {
                use std::ffi::c_void;
                let send_usize: unsafe extern "C" fn(*mut c_void, *mut c_void) -> usize =
                    std::mem::transmute(objc_msgSend as *const ());
                let send_void_usize: unsafe extern "C" fn(*mut c_void, *mut c_void, usize) =
                    std::mem::transmute(objc_msgSend as *const ());
                let sel_get_mask = sel_registerName(b"styleMask\0".as_ptr() as _);
                let sel_set_mask = sel_registerName(b"setStyleMask:\0".as_ptr() as _);
                let cur_mask = send_usize(ns_win, sel_get_mask);
                send_void_usize(ns_win, sel_set_mask, cur_mask | (1 << 2));
            }
        }
    }
    let _ = window.set_minimizable(true);
    if window.minimize().is_err() {
        let _ = window.hide();
    }
    Ok(())
}

#[tauri::command]
fn hide_window(window: tauri::Window) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        if let Ok(mut map) = pre_maximized_frames_map().lock() {
            map.remove(window.label());
        }
        if let Ok(ns_win) = window.ns_window() {
            hide_nswindow_native(ns_win);
        }
    }
    let _ = window.hide();
    Ok(())
}

#[tauri::command]
fn close_current_window(window: tauri::Window, app_handle: AppHandle) -> Result<(), String> {
    let label = window.label().to_string();
    if label == "main" {
        return hide_window(window);
    }
    #[cfg(target_os = "macos")]
    {
        if let Ok(mut map) = pre_maximized_frames_map().lock() {
            map.remove(&label);
        }
        if let Ok(ns_win) = window.ns_window() {
            hide_nswindow_native(ns_win);
        }
    }
    let _ = window.close();

    #[cfg(target_os = "macos")]
    {
        let remaining_detached = app_handle
            .webview_windows()
            .keys()
            .filter(|k| k.starts_with("detached_") && **k != label)
            .count() as u32;
        DETACHED_WINDOW_COUNT.store(remaining_detached, Ordering::Relaxed);
        if remaining_detached == 0 {
            ensure_macos_accessory_policy();
        }
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn configure_standalone_nswindow(ns_win: *mut std::ffi::c_void, width: f64, height: f64) {
    use std::ffi::c_void;
    if ns_win.is_null() {
        return;
    }
    unsafe {
        let send_void_isize: unsafe extern "C" fn(*mut c_void, *mut c_void, isize) =
            std::mem::transmute(objc_msgSend as *const ());
        let send_void_bool: unsafe extern "C" fn(*mut c_void, *mut c_void, i8) =
            std::mem::transmute(objc_msgSend as *const ());
        let send_void: unsafe extern "C" fn(*mut c_void, *mut c_void) =
            std::mem::transmute(objc_msgSend as *const ());
        let send_void_ptr: unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void) =
            std::mem::transmute(objc_msgSend as *const ());
        let send_set_frame: unsafe extern "C" fn(*mut c_void, *mut c_void, CGRect, i8) =
            std::mem::transmute(objc_msgSend as *const ());

        // 1. Crucial: Never hide or close when clicking other applications!
        let sel_hides = sel_registerName(b"setHidesOnDeactivate:\0".as_ptr() as _);
        send_void_bool(ns_win, sel_hides, 0);

        // 2. Default to normal window level (0 = NSNormalWindowLevel) so it does NOT stay in front of
        //    other apps when user clicks other apps (unless user explicitly clicks Pin 📌)
        let sel_level = sel_registerName(b"setLevel:\0".as_ptr() as _);
        send_void_isize(ns_win, sel_level, 0);

        // 3. Center on the current active mouse display
        let (_, bounds, main_h) = get_mouse_display_bounds();
        let cg_x = bounds.origin.x + (bounds.size.width - width) / 2.0;
        let cg_top_y = bounds.origin.y + (bounds.size.height - height) * 0.42;
        let new_frame = CGRect {
            origin: CGPoint {
                x: cg_x.round(),
                y: (main_h - cg_top_y - height).round(),
            },
            size: CGSize { width, height },
        };
        let sel_set_frame = sel_registerName(b"setFrame:display:\0".as_ptr() as _);
        send_set_frame(ns_win, sel_set_frame, new_frame, 1);

        // 4. Activate application and bring standalone window to front
        set_macos_regular_policy_and_activate();

        let sel_key_front = sel_registerName(b"makeKeyAndOrderFront:\0".as_ptr() as _);
        send_void_ptr(ns_win, sel_key_front, std::ptr::null_mut());

        let sel_make_key = sel_registerName(b"makeKeyWindow\0".as_ptr() as _);
        send_void(ns_win, sel_make_key);

        focus_webview_first_responder(ns_win);
    }
}

#[tauri::command]
async fn spawn_detached_plugin_window(
    plugin_id: String,
    ext_id: Option<String>,
    title: String,
    app_handle: AppHandle,
) -> Result<String, String> {
    let safe_id: String = plugin_id
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let label = format!("detached_{}_{}", safe_id, now_unix_ms() % 1_000_000);
    let mut url_str = format!("index.html?standalone={}", plugin_id);
    if let Some(eid) = ext_id {
        if !eid.is_empty() {
            url_str.push_str(&format!("&ext={}", eid));
        }
    }

    #[cfg(target_os = "macos")]
    DETACHED_WINDOW_COUNT.fetch_add(1, Ordering::Relaxed);

    // 1. Build a truly independent WebviewWindow first (default always_on_top = false)
    let win = tauri::WebviewWindowBuilder::new(
        &app_handle,
        &label,
        tauri::WebviewUrl::App(url_str.into()),
    )
    .title(&title)
    .inner_size(1040.0, 680.0)
    .min_inner_size(720.0, 480.0)
    .resizable(true)
    .minimizable(true)
    .maximizable(true)
    .decorations(false)
    .transparent(true)
    .always_on_top(false)
    .center()
    .focused(true)
    .visible(true)
    .build()
    .map_err(|e| e.to_string())?;

    // 2. Hide and reset the main launcher overlay window after standalone window is created
    let handle_for_main = app_handle.clone();
    let win_clone = win.clone();
    let _ = app_handle.run_on_main_thread(move || {
        if let Some(main_win) = handle_for_main.get_webview_window("main") {
            let _ = main_win
                .eval("window.__omniboxResetState && window.__omniboxResetState()");
            #[cfg(target_os = "macos")]
            if let Ok(ns_main) = main_win.ns_window() {
                hide_nswindow_native(ns_main);
            }
            let _ = main_win.hide();
        }

        #[cfg(target_os = "macos")]
        if let Ok(ns_win) = win_clone.ns_window() {
            configure_standalone_nswindow(ns_win, 1040.0, 680.0);
        }
    });

    Ok(label)
}

#[tauri::command]
fn update_global_hotkey(hotkey: String, state: State<'_, AppState>) -> Result<(), String> {
    if let Ok(mut hk) = state.global_hotkey.lock() {
        *hk = hotkey;
    }
    Ok(())
}

#[tauri::command]
fn set_window_always_on_top(pinned: bool, window: tauri::Window) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        if window.label() == "main" {
            IS_PINNED_ON_TOP.store(pinned, Ordering::Relaxed);
        } else if let Ok(ns_win) = window.ns_window() {
            if !ns_win.is_null() {
                unsafe {
                    use std::ffi::c_void;
                    let send_void_isize: unsafe extern "C" fn(*mut c_void, *mut c_void, isize) =
                        std::mem::transmute(objc_msgSend as *const ());
                    let sel_level = sel_registerName(b"setLevel:\0".as_ptr() as _);
                    if pinned {
                        send_void_isize(ns_win, sel_level, 3);
                    } else {
                        send_void_isize(ns_win, sel_level, 0);
                    }
                }
            }
        }
    }
    window.set_always_on_top(pinned).map_err(|e| e.to_string())
}

#[tauri::command]
fn sync_native_preferences(
    hide_on_blur: bool,
    detached: bool,
    super_panel_enabled: bool,
    super_panel_hold_ms: u64,
    super_panel_trigger: String,
    window_opacity: f64,
    launch_at_login: bool,
    window: tauri::Window,
) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        if window.label() == "main" {
            HIDE_ON_BLUR_ENABLED.store(hide_on_blur, Ordering::Relaxed);
            IS_DETACHED_WINDOW.store(detached, Ordering::Relaxed);
            SUPER_PANEL_ENABLED.store(super_panel_enabled, Ordering::Relaxed);
            SUPER_PANEL_HOLD_MS.store(super_panel_hold_ms.clamp(150, 1000), Ordering::Relaxed);
            let trigger_mode = match super_panel_trigger.as_str() {
                "长按鼠标中键" => 1,
                "Ctrl + 鼠标右键" => 2,
                _ => 0,
            };
            SUPER_PANEL_TRIGGER_MODE.store(trigger_mode, Ordering::Relaxed);
            let _ = apply_macos_launch_at_login(launch_at_login);
        }

        if let Ok(ns_win) = window.ns_window() {
            set_nswindow_alpha(ns_win, window_opacity);
        }
    }
    Ok(())
}

fn build_window_shown_eval_script() -> String {
    let now = now_unix_ms();
    let last_hidden = LAST_WINDOW_HIDDEN_AT_MS.load(Ordering::Relaxed);
    let hidden_ms_ago = if last_hidden == 0 {
        999_999_999
    } else {
        now.saturating_sub(last_hidden)
    };

    #[cfg(target_os = "macos")]
    check_macos_pasteboard_change();

    let last_copy = LAST_CLIPBOARD_COPY_AT_MS.load(Ordering::Relaxed);
    let copied_ms_ago = if last_copy == 0 {
        999_999_999
    } else {
        now.saturating_sub(last_copy)
    };

    #[cfg(target_os = "macos")]
    let clipboard_text = if copied_ms_ago <= 30_000 {
        // Consume this copy timestamp once so the same copied text won't be re-pasted repeatedly
        LAST_CLIPBOARD_COPY_AT_MS.store(0, Ordering::Relaxed);
        read_macos_clipboard_text()
    } else {
        String::new()
    };
    #[cfg(not(target_os = "macos"))]
    let clipboard_text = String::new();

    let payload = serde_json::json!({
        "hiddenMsAgo": hidden_ms_ago,
        "copiedMsAgo": copied_ms_ago,
        "clipboardText": clipboard_text,
    });
    format!(
        "window.__omniboxOnWindowShown && window.__omniboxOnWindowShown({})",
        payload
    )
}

fn toggle_launcher_window(app_handle: &AppHandle) {
    if let Some(win) = app_handle.get_webview_window("main") {
        let visible = win.is_visible().unwrap_or(false);
        let focused = win.is_focused().unwrap_or(false);

        #[cfg(target_os = "macos")]
        let on_same_display = {
            let (cur_display_id, _, _) = get_mouse_display_bounds();
            let last_display_id = LAST_ACTIVE_DISPLAY_ID.load(Ordering::Relaxed);
            last_display_id == 0 || cur_display_id == last_display_id
        };
        #[cfg(not(target_os = "macos"))]
        let on_same_display = true;

        if visible && focused && on_same_display {
            let _ = win.eval("window.__omniboxOnWindowHidden && window.__omniboxOnWindowHidden()");
            #[cfg(target_os = "macos")]
            if let Ok(ns_win) = win.ns_window() {
                hide_nswindow_native(ns_win);
            }
            let _ = win.hide();
        } else {
            let _ = win.unminimize();
            let eval_script = build_window_shown_eval_script();
            #[cfg(target_os = "macos")]
            {
                summon_macos_overlay_on_current_space_and_screen(&win);
            }
            #[cfg(not(target_os = "macos"))]
            {
                let _ = win.show();
                let _ = win.set_focus();
            }
            let _ = win.eval(&eval_script);
        }
    }
}

#[cfg(target_os = "macos")]
fn check_custom_hotkey_pressed(
    configured: &str,
    cmd_down: bool,
    opt_down: bool,
    ctrl_down: bool,
    shift_down: bool,
) -> (bool, Option<u16>) {
    if configured == "Double Command" || configured.contains("⌘⌘") {
        return (false, None);
    }
    let parts: Vec<&str> = configured.split('+').map(|s| s.trim()).collect();
    if parts.is_empty() {
        return (false, None);
    }

    let mut need_cmd = false;
    let mut need_opt = false;
    let mut need_ctrl = false;
    let mut need_shift = false;
    let mut key_code: Option<u16> = None;

    for p in parts {
        match p.to_uppercase().as_str() {
            "COMMAND" | "CMD" | "META" | "⌘" => need_cmd = true,
            "OPTION" | "ALT" | "OPT" | "⌥" => need_opt = true,
            "CTRL" | "CONTROL" | "⌃" => need_ctrl = true,
            "SHIFT" | "⇧" => need_shift = true,
            other => {
                key_code = hotkey_token_to_keycode(other);
            }
        }
    }

    if let Some(kc) = key_code {
        let mods_match = cmd_down == need_cmd
            && opt_down == need_opt
            && ctrl_down == need_ctrl
            && shift_down == need_shift;
        let pressed = unsafe { CGEventSourceKeyState(0, kc) };
        (mods_match && pressed, Some(kc))
    } else {
        (false, None)
    }
}

#[cfg(target_os = "macos")]
fn spawn_macos_global_hotkey_listener(app_handle: AppHandle, hotkey_ref: Arc<Mutex<String>>) {
    std::thread::spawn(move || {
        const FLAG_CMD: u64 = 0x0010_0000;
        const FLAG_OPT: u64 = 0x0008_0000;
        const FLAG_CTRL: u64 = 0x0004_0000;
        const FLAG_SHIFT: u64 = 0x0002_0000;
        const KEY_SPACE: u16 = 49;
        const KEY_ESCAPE: u16 = 53;
        const EVENT_KEY_DOWN: u32 = 10;

        let mut prev_cmd_down = false;
        let mut cmd_press_start = Instant::now();
        let mut cmd_start_key_counter: u32 = 0;
        let mut cmd_tainted = false;
        let mut last_clean_cmd_release: Option<Instant> = None;
        let mut prev_custom_key_down = false;
        let mut prev_opt_space_down = false;
        let mut prev_esc_down = false;
        let mut prev_mouse_down = false;
        let mut last_trigger_at = Instant::now() - Duration::from_secs(2);
        let mut tick_counter: u64 = 0;

        // Super Panel Mouse Hold State
        let mut prev_sp_btn_down = false;
        let mut sp_press_start = Instant::now();
        let mut sp_start_pt = CGPoint { x: 0.0, y: 0.0 };
        let mut sp_triggered_for_hold = false;

        // Initialize pasteboard changeCount baseline
        check_macos_pasteboard_change();

        loop {
            std::thread::sleep(Duration::from_millis(12));
            tick_counter = tick_counter.wrapping_add(1);

            if tick_counter % 10 == 0 {
                check_macos_pasteboard_change();
            }

            unsafe {
                let flags = CGEventSourceFlagsState(0);
                let space_down = CGEventSourceKeyState(0, KEY_SPACE);
                let esc_down = CGEventSourceKeyState(0, KEY_ESCAPE);
                let left_btn = CGEventSourceButtonState(0, 0);
                let right_btn = CGEventSourceButtonState(0, 1);
                let mid_btn = CGEventSourceButtonState(0, 2);
                let mouse_down = left_btn || right_btn;
                let key_counter = CGEventSourceCounterForEventType(0, EVENT_KEY_DOWN);

                let cmd_down = (flags & FLAG_CMD) != 0;
                let opt_down = (flags & FLAG_OPT) != 0;
                let ctrl_down = (flags & FLAG_CTRL) != 0;
                let shift_down = (flags & FLAG_SHIFT) != 0;

                // 0a. Click outside window bounds -> automatically close/hide OmniBox
                //     (Only when HIDE_ON_BLUR_ENABLED is true AND not pinned on top AND not detached)
                if mouse_down
                    && !prev_mouse_down
                    && HIDE_ON_BLUR_ENABLED.load(Ordering::Relaxed)
                    && !IS_PINNED_ON_TOP.load(Ordering::Relaxed)
                    && !IS_DETACHED_WINDOW.load(Ordering::Relaxed)
                {
                    let handle_clone = app_handle.clone();
                    let _ = app_handle.run_on_main_thread(move || {
                        if let Some(win) = handle_clone.get_webview_window("main") {
                            if win.is_visible().unwrap_or(false) {
                                if let Ok(ns_win) = win.ns_window() {
                                    if !ns_win.is_null() {
                                        use std::ffi::c_void;
                                        let main_h =
                                            CGDisplayBounds(CGMainDisplayID()).size.height;
                                        let ev = CGEventCreate(std::ptr::null());
                                        if !ev.is_null() {
                                            let mouse_pt = CGEventGetLocation(ev);
                                            CFRelease(ev);
                                            let cocoa_x = mouse_pt.x;
                                            let cocoa_y = main_h - mouse_pt.y;

                                            let send_rect: unsafe extern "C" fn(
                                                *mut c_void,
                                                *mut c_void,
                                            ) -> CGRect =
                                                std::mem::transmute(objc_msgSend as *const ());
                                            let sel_frame =
                                                sel_registerName(b"frame\0".as_ptr() as _);
                                            let frame = send_rect(ns_win, sel_frame);

                                            let inside = cocoa_x >= frame.origin.x
                                                && cocoa_x <= frame.origin.x + frame.size.width
                                                && cocoa_y >= frame.origin.y
                                                && cocoa_y <= frame.origin.y + frame.size.height;
                                            if !inside {
                                                let _ = win.eval("window.__omniboxOnWindowHidden ? window.__omniboxOnWindowHidden() : (window.__omniboxResetState && window.__omniboxResetState())");
                                                hide_nswindow_native(ns_win);
                                                let _ = win.hide();
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    });
                }
                prev_mouse_down = mouse_down;

                // 0b. Super Panel Mouse Hold Detection (Long-press Right Click / Middle Click / Ctrl+Right Click)
                if SUPER_PANEL_ENABLED.load(Ordering::Relaxed) {
                    let trigger_mode = SUPER_PANEL_TRIGGER_MODE.load(Ordering::Relaxed);
                    let sp_btn_down = match trigger_mode {
                        1 => mid_btn,
                        2 => right_btn && ctrl_down,
                        _ => right_btn && !ctrl_down && !cmd_down && !opt_down,
                    };

                    if sp_btn_down && !prev_sp_btn_down {
                        sp_press_start = Instant::now();
                        sp_triggered_for_hold = false;
                        let ev = CGEventCreate(std::ptr::null());
                        if !ev.is_null() {
                            sp_start_pt = CGEventGetLocation(ev);
                            CFRelease(ev);
                        }
                    } else if sp_btn_down && !sp_triggered_for_hold {
                        let hold_target_ms = SUPER_PANEL_HOLD_MS.load(Ordering::Relaxed) as u128;
                        if sp_press_start.elapsed().as_millis() >= hold_target_ms {
                            let ev = CGEventCreate(std::ptr::null());
                            if !ev.is_null() {
                                let cur_pt = CGEventGetLocation(ev);
                                CFRelease(ev);
                                let dist = ((cur_pt.x - sp_start_pt.x).powi(2)
                                    + (cur_pt.y - sp_start_pt.y).powi(2))
                                .sqrt();
                                if dist <= 6.0 {
                                    sp_triggered_for_hold = true;
                                    let handle_clone = app_handle.clone();
                                    let _ = app_handle.run_on_main_thread(move || {
                                        if let Some(win) = handle_clone.get_webview_window("main") {
                                            let clip = read_macos_clipboard_text();
                                            let payload = serde_json::json!({
                                                "clipboardText": clip
                                            });
                                            let _ = win.unminimize();
                                            summon_macos_overlay_on_current_space_and_screen(&win);
                                            let _ = win.eval(&format!(
                                                "window.__omniboxOpenSuperPanel && window.__omniboxOpenSuperPanel({})",
                                                payload
                                            ));
                                        }
                                    });
                                } else {
                                    sp_triggered_for_hold = true;
                                }
                            }
                        }
                    }
                    prev_sp_btn_down = sp_btn_down;
                } else {
                    prev_sp_btn_down = false;
                }

                // 0. Native Escape (kVK_Escape = 53) handler when OmniBox window is visible
                if esc_down && !prev_esc_down && !cmd_down && !opt_down && !ctrl_down {
                    let handle_clone = app_handle.clone();
                    let _ = app_handle.run_on_main_thread(move || {
                        if let Some(win) = handle_clone.get_webview_window("main") {
                            if win.is_visible().unwrap_or(false) {
                                let _ = win.eval(
                                    "window.__omniboxNativeEscape && window.__omniboxNativeEscape()",
                                );
                            }
                        }
                    });
                }
                prev_esc_down = esc_down;

                // 1. Check Option+Space (always active fallback) + Any Configured Custom Hotkey
                let configured = hotkey_ref
                    .lock()
                    .map(|s| s.clone())
                    .unwrap_or_else(|_| "Option + Space".to_string());

                let opt_space_now =
                    space_down && opt_down && !cmd_down && !ctrl_down && !shift_down;
                let (custom_match_now, _) = check_custom_hotkey_pressed(
                    &configured,
                    cmd_down,
                    opt_down,
                    ctrl_down,
                    shift_down,
                );

                if ((opt_space_now && !prev_opt_space_down)
                    || (custom_match_now && !prev_custom_key_down))
                    && last_trigger_at.elapsed() > Duration::from_millis(250)
                {
                    last_trigger_at = Instant::now();
                    cmd_tainted = true;
                    let handle_clone = app_handle.clone();
                    let _ = app_handle.run_on_main_thread(move || {
                        toggle_launcher_window(&handle_clone);
                    });
                }
                prev_opt_space_down = opt_space_now;
                prev_custom_key_down = custom_match_now;

                // 2. Check Double Command (⌘⌘) tap
                if cmd_down && !prev_cmd_down {
                    cmd_press_start = Instant::now();
                    cmd_start_key_counter = key_counter;
                    cmd_tainted = opt_down || ctrl_down || shift_down || space_down;
                } else if cmd_down {
                    if opt_down
                        || ctrl_down
                        || shift_down
                        || space_down
                        || key_counter != cmd_start_key_counter
                    {
                        cmd_tainted = true;
                    }
                } else if !cmd_down && prev_cmd_down {
                    let held_ms = cmd_press_start.elapsed().as_millis();
                    if !cmd_tainted && (15..=360).contains(&held_ms) {
                        if let Some(prev_release) = last_clean_cmd_release {
                            if prev_release.elapsed() <= Duration::from_millis(400)
                                && last_trigger_at.elapsed() > Duration::from_millis(250)
                            {
                                last_clean_cmd_release = None;
                                last_trigger_at = Instant::now();
                                let handle_clone = app_handle.clone();
                                let _ = app_handle.run_on_main_thread(move || {
                                    toggle_launcher_window(&handle_clone);
                                });
                            } else {
                                last_clean_cmd_release = Some(Instant::now());
                            }
                        } else {
                            last_clean_cmd_release = Some(Instant::now());
                        }
                    } else {
                        last_clean_cmd_release = None;
                    }
                }
                prev_cmd_down = cmd_down;
            }
        }
    });
}

#[tauri::command]
fn record_item_usage(item_id: String, state: State<'_, AppState>) -> Result<(), String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.record_usage(&item_id)
}

#[tauri::command]
fn launch_system_app(app_path: String, window: tauri::Window) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        // Hide launcher window immediately in native layer before/while launching target app
        if let Ok(ns_win) = window.ns_window() {
            hide_nswindow_native(ns_win);
        }
        let _ = window.hide();

        if app_path.ends_with(".app") {
            Command::new("open")
                .arg("-a")
                .arg(&app_path)
                .spawn()
                .map_err(|e| e.to_string())?;
        } else {
            Command::new("open")
                .arg(&app_path)
                .spawn()
                .map_err(|e| e.to_string())?;
        }
    }
    #[cfg(target_os = "windows")]
    {
        let _ = window.hide();
        Command::new("cmd")
            .args(["/C", "start", "", &app_path])
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
fn reveal_in_finder(target_path: String, window: tauri::Window) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        if let Ok(ns_win) = window.ns_window() {
            hide_nswindow_native(ns_win);
        }
        let _ = window.hide();

        Command::new("open")
            .arg("-R")
            .arg(&target_path)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
fn execute_system_command(cmd_id: String) -> Result<String, String> {
    #[cfg(target_os = "macos")]
    {
        match cmd_id.as_str() {
            "sys:lock-screen" => {
                Command::new("pmset")
                    .arg("displaysleepnow")
                    .spawn()
                    .map_err(|e| e.to_string())?;
                Ok("已触发屏幕休眠锁定".to_string())
            }
            "sys:empty-trash" => {
                Command::new("osascript")
                    .args(["-e", "tell application \"Finder\" to empty the trash"])
                    .spawn()
                    .map_err(|e| e.to_string())?;
                Ok("已发送清空废纸篓指令".to_string())
            }
            "sys:toggle-dark-mode" => {
                Command::new("osascript")
                    .args([
                        "-e",
                        "tell application \"System Events\" to tell appearance preferences to set dark mode to not dark mode",
                    ])
                    .spawn()
                    .map_err(|e| e.to_string())?;
                Ok("已切换 macOS 系统深浅色模式".to_string())
            }
            "sys:show-hidden-files" => {
                Command::new("osascript")
                    .args([
                        "-e",
                        "do shell script \"defaults write com.apple.finder AppleShowAllFiles -bool true && killall Finder\"",
                    ])
                    .spawn()
                    .map_err(|e| e.to_string())?;
                Ok("已切换访达隐藏文件显示状态".to_string())
            }
            "sys:copy-local-ip" => {
                let out = Command::new("ipconfig")
                    .args(["getifaddr", "en0"])
                    .output()
                    .map_err(|e| e.to_string())?;
                let ip = String::from_utf8_lossy(&out.stdout).trim().to_string();
                let resolved = if ip.is_empty() {
                    "127.0.0.1".to_string()
                } else {
                    ip
                };
                Ok(resolved)
            }
            _ => Err(format!("未知系统命令: {}", cmd_id)),
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        Ok(format!("系统命令已执行: {}", cmd_id))
    }
}

#[tauri::command]
fn compute_totp(
    secret: String,
    period: Option<u64>,
    digits: Option<u32>,
) -> Result<TotpCodeResponse, String> {
    let (code, remaining) = generate_totp_code(&secret, period.unwrap_or(30), digits.unwrap_or(6))?;
    Ok(TotpCodeResponse { code, remaining })
}

#[tauri::command]
fn kv_storage_get(
    plugin_id: String,
    doc_id: String,
    state: State<'_, AppState>,
) -> Result<Option<String>, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.kv_get(&plugin_id, &doc_id)
}

#[tauri::command]
fn kv_storage_put(
    plugin_id: String,
    doc_id: String,
    data_json: String,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.kv_put(&plugin_id, &doc_id, &data_json)
}

#[tauri::command]
fn vault_encrypt_payload(master_password: String, plaintext: String) -> Result<String, String> {
    let salt = generate_random_bytes::<16>();
    let key = derive_key_argon2(&master_password, &salt)?;
    let (nonce, ciphertext) = encrypt_aes256_gcm(&key, plaintext.as_bytes())?;

    let packet = serde_json::json!({
        "salt": base64::Engine::encode(&base64::engine::general_purpose::STANDARD, salt),
        "nonce": base64::Engine::encode(&base64::engine::general_purpose::STANDARD, nonce),
        "ciphertext": base64::Engine::encode(&base64::engine::general_purpose::STANDARD, ciphertext),
    });
    Ok(packet.to_string())
}

#[tauri::command]
fn vault_decrypt_payload(master_password: String, packet_json: String) -> Result<String, String> {
    let parsed: serde_json::Value = serde_json::from_str(&packet_json).map_err(|e| e.to_string())?;
    let salt_b64 = parsed["salt"].as_str().ok_or("Missing salt")?;
    let nonce_b64 = parsed["nonce"].as_str().ok_or("Missing nonce")?;
    let cipher_b64 = parsed["ciphertext"].as_str().ok_or("Missing ciphertext")?;

    let salt = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, salt_b64)
        .map_err(|e| e.to_string())?;
    let nonce = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, nonce_b64)
        .map_err(|e| e.to_string())?;
    let ciphertext = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, cipher_b64)
        .map_err(|e| e.to_string())?;

    let key = derive_key_argon2(&master_password, &salt)?;
    let plain_bytes = decrypt_aes256_gcm(&key, &nonce, &ciphertext)?;
    String::from_utf8(plain_bytes).map_err(|e| e.to_string())
}

#[tauri::command]
async fn capture_screen_region_base64(
    window: tauri::WebviewWindow,
    app_handle: AppHandle,
) -> Result<Option<String>, String> {
    let is_main = window.label() == "main";

    // Step 1: Temporarily hide the current OmniBox window on the macOS AppKit Main Thread
    let win_for_hide = window.clone();
    let _ = app_handle.run_on_main_thread(move || {
        #[cfg(target_os = "macos")]
        {
            if let Ok(ns_win) = win_for_hide.ns_window() {
                hide_nswindow_native(ns_win);
            }
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = win_for_hide.hide();
        }
    });

    // Step 2: Run interactive crosshair region capture in a blocking thread so UI thread never stalls
    let capture_res = tauri::async_runtime::spawn_blocking(move || -> Result<Option<String>, String> {
        // Wait briefly for window hide animation to complete so underlying browser QR code is fully visible
        std::thread::sleep(Duration::from_millis(160));

        let temp_path = std::env::temp_dir().join(format!("omnibox_qr_scan_{}.png", now_unix_ms()));
        let temp_str = temp_path.to_string_lossy().to_string();

        // -i: interactive mouse selection crosshair (Esc cancels)
        // -x: do not play camera shutter sound
        let status = Command::new("/usr/sbin/screencapture")
            .args(["-i", "-x", &temp_str])
            .status()
            .map_err(|e| format!("无法启动系统框选截图: {}", e))?;

        // Small settle delay after mouse release so global mouse-down monitor doesn't see residual click
        std::thread::sleep(Duration::from_millis(80));

        if !status.success() || !temp_path.exists() {
            let _ = std::fs::remove_file(&temp_path);
            return Ok(None);
        }

        let bytes = std::fs::read(&temp_path).unwrap_or_default();
        let _ = std::fs::remove_file(&temp_path);

        if bytes.is_empty() {
            return Ok(None);
        }

        let b64 = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &bytes);
        Ok(Some(format!("data:image/png;base64,{}", b64)))
    })
    .await
    .map_err(|e| e.to_string())??;

    // Step 3: Restore the OmniBox window in-place and refocus on the macOS AppKit Main Thread
    let win_for_show = window.clone();
    let _ = app_handle.run_on_main_thread(move || {
        #[cfg(target_os = "macos")]
        {
            if let Ok(ns_win) = win_for_show.ns_window() {
                if !ns_win.is_null() {
                    unsafe {
                        use std::ffi::c_void;
                        let send_void: unsafe extern "C" fn(*mut c_void, *mut c_void) =
                            std::mem::transmute(objc_msgSend as *const ());
                        let send_void_ptr: unsafe extern "C" fn(
                            *mut c_void,
                            *mut c_void,
                            *mut c_void,
                        ) = std::mem::transmute(objc_msgSend as *const ());

                        if is_main {
                            configure_nswindow_overlay_props(ns_win);
                        } else {
                            set_macos_regular_policy_and_activate();
                        }

                        let sel_order_front =
                            sel_registerName(b"orderFrontRegardless\0".as_ptr() as _);
                        send_void(ns_win, sel_order_front);

                        let sel_key_front =
                            sel_registerName(b"makeKeyAndOrderFront:\0".as_ptr() as _);
                        send_void_ptr(ns_win, sel_key_front, std::ptr::null_mut());

                        let sel_make_key = sel_registerName(b"makeKeyWindow\0".as_ptr() as _);
                        send_void(ns_win, sel_make_key);

                        focus_webview_first_responder(ns_win);
                    }
                }
            }
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = win_for_show.show();
            let _ = win_for_show.set_focus();
        }
    });

    Ok(capture_res)
}

#[tauri::command]
fn read_clipboard_image_base64() -> Result<Option<String>, String> {
    #[cfg(target_os = "macos")]
    unsafe {
        use std::ffi::c_void;
        let cls_pb = objc_getClass(b"NSPasteboard\0".as_ptr() as _);
        let cls_str = objc_getClass(b"NSString\0".as_ptr() as _);
        if cls_pb.is_null() || cls_str.is_null() {
            return Ok(None);
        }

        let send_id: unsafe extern "C" fn(*mut c_void, *mut c_void) -> *mut c_void =
            std::mem::transmute(objc_msgSend as *const ());
        let send_id_ptr: unsafe extern "C" fn(*mut c_void, *mut c_void, *const c_void) -> *mut c_void =
            std::mem::transmute(objc_msgSend as *const ());
        let send_usize: unsafe extern "C" fn(*mut c_void, *mut c_void) -> usize =
            std::mem::transmute(objc_msgSend as *const ());

        let sel_gen = sel_registerName(b"generalPasteboard\0".as_ptr() as _);
        let pb = send_id(cls_pb, sel_gen);
        if pb.is_null() {
            return Ok(None);
        }

        let sel_utf8 = sel_registerName(b"stringWithUTF8String:\0".as_ptr() as _);
        let sel_data_for_type = sel_registerName(b"dataForType:\0".as_ptr() as _);
        let sel_bytes = sel_registerName(b"bytes\0".as_ptr() as _);
        let sel_length = sel_registerName(b"length\0".as_ptr() as _);

        // 1. Try reading public.png directly from NSPasteboard
        let ns_png_type = send_id_ptr(cls_str, sel_utf8, b"public.png\0".as_ptr() as _);
        let mut png_data = send_id_ptr(pb, sel_data_for_type, ns_png_type);

        // 2. Fallback: if clipboard has public.tiff (e.g. macOS screenshot / WeChat), convert via NSBitmapImageRep to PNG
        if png_data.is_null() {
            let ns_tiff_type = send_id_ptr(cls_str, sel_utf8, b"public.tiff\0".as_ptr() as _);
            let tiff_data = send_id_ptr(pb, sel_data_for_type, ns_tiff_type);
            if !tiff_data.is_null() {
                let cls_rep = objc_getClass(b"NSBitmapImageRep\0".as_ptr() as _);
                if !cls_rep.is_null() {
                    let sel_rep_with_data = sel_registerName(b"imageRepWithData:\0".as_ptr() as _);
                    let rep = send_id_ptr(cls_rep, sel_rep_with_data, tiff_data);
                    if !rep.is_null() {
                        let sel_repr =
                            sel_registerName(b"representationUsingType:properties:\0".as_ptr() as _);
                        let send_repr: unsafe extern "C" fn(
                            *mut c_void,
                            *mut c_void,
                            usize,
                            *mut c_void,
                        ) -> *mut c_void = std::mem::transmute(objc_msgSend as *const ());
                        // NSBitmapImageFileTypePNG = 4
                        png_data = send_repr(rep, sel_repr, 4, std::ptr::null_mut());
                    }
                }
            }
        }

        if png_data.is_null() {
            return Ok(None);
        }

        let len = send_usize(png_data, sel_length);
        let ptr = send_id(png_data, sel_bytes) as *const u8;
        if len == 0 || ptr.is_null() {
            return Ok(None);
        }

        let slice = std::slice::from_raw_parts(ptr, len);
        let b64 = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, slice);
        return Ok(Some(format!("data:image/png;base64,{}", b64)));
    }

    #[cfg(not(target_os = "macos"))]
    Ok(None)
}

fn main() {
    let app_data_dir = dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("com.omnibox.launcher");

    let db_path = app_data_dir.join("omni_data.db");
    let icon_cache_dir = app_data_dir.join("app_icons_cache");

    let db = AppDatabase::init(db_path).expect("Failed to initialize OmniBox SQLite database");
    let sys_apps = scan_system_applications(&icon_cache_dir);
    let initial_items = build_core_items(&sys_apps);

    // Load saved globalHotkey from SQLite if present
    let initial_hotkey = db
        .kv_get("system", "user-settings")
        .ok()
        .flatten()
        .and_then(|json_str| serde_json::from_str::<serde_json::Value>(&json_str).ok())
        .and_then(|v| v["globalHotkey"].as_str().map(String::from))
        .unwrap_or_else(|| "Option + Space".to_string());

    let hotkey_arc = Arc::new(Mutex::new(initial_hotkey));
    let hotkey_for_setup = hotkey_arc.clone();

    let app = tauri::Builder::default()
        .manage(AppState {
            db: Mutex::new(db),
            items: Mutex::new(initial_items),
            sys_apps: Mutex::new(sys_apps),
            icon_cache_dir,
            global_hotkey: hotkey_arc,
        })
        .setup(move |app| {
            #[cfg(target_os = "macos")]
            {
                let _ = app.set_activation_policy(tauri::ActivationPolicy::Accessory);
                ensure_macos_accessory_policy();
                if let Some(win) = app.get_webview_window("main") {
                    if let Ok(ns_win) = win.ns_window() {
                        configure_nswindow_overlay_props(ns_win);
                        place_nswindow_on_active_mouse_screen(ns_win, true);
                    }
                }
                spawn_macos_global_hotkey_listener(app.handle().clone(), hotkey_for_setup);
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            search_query,
            get_system_apps,
            rescan_system_apps,
            search_local_files,
            resize_window,
            start_window_drag,
            minimize_window,
            hide_window,
            close_current_window,
            spawn_detached_plugin_window,
            update_global_hotkey,
            set_window_always_on_top,
            sync_native_preferences,
            record_item_usage,
            launch_system_app,
            reveal_in_finder,
            execute_system_command,
            compute_totp,
            kv_storage_get,
            kv_storage_put,
            vault_encrypt_payload,
            vault_decrypt_payload,
            capture_screen_region_base64,
            read_clipboard_image_base64
        ])
        .build(tauri::generate_context!())
        .expect("error while building OmniBox Tauri application");

    app.run(|app_handle, event| {
        #[cfg(target_os = "macos")]
        match event {
            tauri::RunEvent::Ready => {
                let _ = app_handle.set_activation_policy(tauri::ActivationPolicy::Accessory);
                ensure_macos_accessory_policy();
                if let Some(win) = app_handle.get_webview_window("main") {
                    if let Ok(ns_win) = win.ns_window() {
                        configure_nswindow_overlay_props(ns_win);
                    }
                }
            }
            tauri::RunEvent::Reopen { .. } => {
                let mut brought_detached = false;
                for (label, win) in app_handle.webview_windows() {
                    if label.starts_with("detached_") {
                        let _ = win.unminimize();
                        let _ = win.show();
                        let _ = win.set_focus();
                        brought_detached = true;
                    }
                }
                if !brought_detached {
                    if let Some(win) = app_handle.get_webview_window("main") {
                        let _ = win.unminimize();
                        let eval_script = build_window_shown_eval_script();
                        summon_macos_overlay_on_current_space_and_screen(&win);
                        let _ = win.eval(&eval_script);
                    }
                }
            }
            tauri::RunEvent::ExitRequested { api, .. } => {
                api.prevent_exit();
            }
            _ => {}
        }
    });
}
