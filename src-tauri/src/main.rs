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
static ALLOW_APP_EXIT: AtomicBool = AtomicBool::new(false);
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
extern "C" fn omnibox_standalone_can_become_main(
    _self: *mut std::ffi::c_void,
    _cmd: *mut std::ffi::c_void,
) -> i8 {
    1 // YES: Standalone detached window is a first-class macOS Main Window for Cmd+Tab & Dock!
}

#[cfg(target_os = "macos")]
fn get_or_register_standalone_window_class() -> *mut std::ffi::c_void {
    unsafe {
        let existing = objc_getClass(b"OmniBoxStandaloneWindow\0".as_ptr() as _);
        if !existing.is_null() {
            return existing;
        }
        let nswin_cls = objc_getClass(b"NSWindow\0".as_ptr() as _);
        let win_cls =
            objc_allocateClassPair(nswin_cls, b"OmniBoxStandaloneWindow\0".as_ptr() as _, 0);
        if !win_cls.is_null() {
            let _ = class_addIvar(
                win_cls,
                b"focusable\0".as_ptr() as _,
                1,
                0,
                b"B\0".as_ptr() as _,
            );
            let sel_key = sel_registerName(b"canBecomeKeyWindow\0".as_ptr() as _);
            let _ = class_addMethod(
                win_cls,
                sel_key,
                omnibox_panel_can_become_key as *const std::ffi::c_void,
                b"B@:\0".as_ptr() as _,
            );
            let sel_main = sel_registerName(b"canBecomeMainWindow\0".as_ptr() as _);
            let _ = class_addMethod(
                win_cls,
                sel_main,
                omnibox_standalone_can_become_main as *const std::ffi::c_void,
                b"B@:\0".as_ptr() as _,
            );
            objc_registerClassPair(win_cls);
        }
        win_cls
    }
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
    if DETACHED_WINDOW_COUNT.load(Ordering::Relaxed) > 0
        || IS_DETACHED_WINDOW.load(Ordering::Relaxed)
    {
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
fn is_macos_app_active() -> bool {
    use std::ffi::c_void;
    unsafe {
        let send_id: unsafe extern "C" fn(*mut c_void, *mut c_void) -> *mut c_void =
            std::mem::transmute(objc_msgSend as *const ());
        let send_bool: unsafe extern "C" fn(*mut c_void, *mut c_void) -> i8 =
            std::mem::transmute(objc_msgSend as *const ());

        let app_cls = objc_getClass(b"NSApplication\0".as_ptr() as _);
        if app_cls.is_null() {
            return false;
        }
        let sel_shared = sel_registerName(b"sharedApplication\0".as_ptr() as _);
        let ns_app = send_id(app_cls, sel_shared);
        if ns_app.is_null() {
            return false;
        }
        let sel_active = sel_registerName(b"isActive\0".as_ptr() as _);
        send_bool(ns_app, sel_active) != 0
    }
}

/// Inject the custom HyperCube application icon into NSApplication at runtime
/// so macOS Dock, Cmd+Tab App Switcher and Activity Monitor always display our logo even in `tauri dev` mode.
#[cfg(target_os = "macos")]
fn apply_macos_application_icon() {
    use std::ffi::c_void;
    static ICON_BYTES: &[u8] = include_bytes!("../icons/icon.png");
    unsafe {
        let send_id: unsafe extern "C" fn(*mut c_void, *mut c_void) -> *mut c_void =
            std::mem::transmute(objc_msgSend as *const ());
        let send_data: unsafe extern "C" fn(
            *mut c_void,
            *mut c_void,
            *const u8,
            usize,
        ) -> *mut c_void = std::mem::transmute(objc_msgSend as *const ());
        let send_init_data: unsafe extern "C" fn(
            *mut c_void,
            *mut c_void,
            *mut c_void,
        ) -> *mut c_void = std::mem::transmute(objc_msgSend as *const ());
        let send_void_id: unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void) =
            std::mem::transmute(objc_msgSend as *const ());

        let cls_data = objc_getClass(b"NSData\0".as_ptr() as _);
        let cls_img = objc_getClass(b"NSImage\0".as_ptr() as _);
        let cls_app = objc_getClass(b"NSApplication\0".as_ptr() as _);
        if cls_data.is_null() || cls_img.is_null() || cls_app.is_null() {
            return;
        }

        let sel_data_with_bytes = sel_registerName(b"dataWithBytes:length:\0".as_ptr() as _);
        let ns_data = send_data(
            cls_data,
            sel_data_with_bytes,
            ICON_BYTES.as_ptr(),
            ICON_BYTES.len(),
        );
        if ns_data.is_null() {
            return;
        }

        let sel_alloc = sel_registerName(b"alloc\0".as_ptr() as _);
        let sel_init_with_data = sel_registerName(b"initWithData:\0".as_ptr() as _);
        let img_alloc = send_id(cls_img, sel_alloc);
        let ns_img = send_init_data(img_alloc, sel_init_with_data, ns_data);
        if ns_img.is_null() {
            return;
        }

        let sel_shared = sel_registerName(b"sharedApplication\0".as_ptr() as _);
        let ns_app = send_id(cls_app, sel_shared);
        if !ns_app.is_null() {
            let sel_set_icon = sel_registerName(b"setApplicationIconImage:\0".as_ptr() as _);
            send_void_id(ns_app, sel_set_icon, ns_img);
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
            apply_macos_application_icon();
            let sel_activate = sel_registerName(b"activateIgnoringOtherApps:\0".as_ptr() as _);
            send_void_bool(ns_app, sel_activate, 1);
        }
    }
}

/// Bring a standalone NSWindow to the front of all other application windows (including Chrome/browsers)
#[cfg(target_os = "macos")]
fn raise_standalone_nswindow_to_front(ns_win: *mut std::ffi::c_void) {
    use std::ffi::c_void;
    if ns_win.is_null() {
        return;
    }
    unsafe {
        let send_void: unsafe extern "C" fn(*mut c_void, *mut c_void) =
            std::mem::transmute(objc_msgSend as *const ());
        let send_void_ptr: unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void) =
            std::mem::transmute(objc_msgSend as *const ());
        let send_bool: unsafe extern "C" fn(*mut c_void, *mut c_void) -> i8 =
            std::mem::transmute(objc_msgSend as *const ());

        set_macos_regular_policy_and_activate();

        let sel_is_mini = sel_registerName(b"isMiniaturized\0".as_ptr() as _);
        if send_bool(ns_win, sel_is_mini) != 0 {
            let sel_demini = sel_registerName(b"deminiaturize:\0".as_ptr() as _);
            send_void_ptr(ns_win, sel_demini, std::ptr::null_mut());
        }

        let sel_order_front = sel_registerName(b"orderFrontRegardless\0".as_ptr() as _);
        send_void(ns_win, sel_order_front);

        let sel_key_front = sel_registerName(b"makeKeyAndOrderFront:\0".as_ptr() as _);
        send_void_ptr(ns_win, sel_key_front, std::ptr::null_mut());

        let sel_make_main = sel_registerName(b"makeMainWindow\0".as_ptr() as _);
        send_void(ns_win, sel_make_main);

        let sel_make_key = sel_registerName(b"makeKeyWindow\0".as_ptr() as _);
        send_void(ns_win, sel_make_key);

        focus_webview_first_responder(ns_win);
    }
}

/// Raise all active detached windows to the front when user switches back via Command+Tab or Dock click
fn bring_all_detached_windows_to_front(app_handle: &AppHandle, unminimize_all: bool) -> bool {
    let mut found = false;
    for (label, win) in app_handle.webview_windows() {
        if label.starts_with("detached_") {
            found = true;
            if unminimize_all {
                let _ = win.unminimize();
            }
            let _ = win.show();
            #[cfg(target_os = "macos")]
            if let Ok(ns_win) = win.ns_window() {
                if !ns_win.is_null() {
                    unsafe {
                        use std::ffi::c_void;
                        let send_bool: unsafe extern "C" fn(*mut c_void, *mut c_void) -> i8 =
                            std::mem::transmute(objc_msgSend as *const ());
                        let sel_is_mini = sel_registerName(b"isMiniaturized\0".as_ptr() as _);
                        let is_mini = send_bool(ns_win, sel_is_mini) != 0;
                        if unminimize_all || !is_mini {
                            raise_standalone_nswindow_to_front(ns_win);
                        }
                    }
                }
            }
            #[cfg(not(target_os = "macos"))]
            {
                let _ = win.set_focus();
            }
            let _ = win.eval("window.dispatchEvent(new Event('focus'))");
        }
    }
    #[cfg(target_os = "macos")]
    if found {
        let _ = app_handle.set_activation_policy(tauri::ActivationPolicy::Regular);
    }
    found
}

/// Synchronize macOS Dock & Cmd+Tab visibility based on live `detached_*` windows
#[cfg(target_os = "macos")]
fn sync_macos_activation_policy(app_handle: &AppHandle, excluding_label: Option<&str>) {
    let count = app_handle
        .webview_windows()
        .keys()
        .filter(|k| {
            k.starts_with("detached_")
                && excluding_label.map(|ex| *k != ex).unwrap_or(true)
        })
        .count() as u32;
    DETACHED_WINDOW_COUNT.store(count, Ordering::Relaxed);
    if count > 0 || IS_DETACHED_WINDOW.load(Ordering::Relaxed) {
        let _ = app_handle.set_activation_policy(tauri::ActivationPolicy::Regular);
        set_macos_regular_policy_and_activate();
    } else {
        let _ = app_handle.set_activation_policy(tauri::ActivationPolicy::Accessory);
        ensure_macos_accessory_policy();
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
fn write_macos_clipboard_text(text: &str) {
    use std::io::Write;
    if let Ok(mut child) = Command::new("pbcopy")
        .stdin(std::process::Stdio::piped())
        .spawn()
    {
        if let Some(stdin) = child.stdin.as_mut() {
            let _ = stdin.write_all(text.as_bytes());
        }
        let _ = child.wait();
    }
}

#[cfg(target_os = "macos")]
fn resolve_preferred_autostart_executable() -> Result<PathBuf, String> {
    let installed_app_bin = PathBuf::from("/Applications/OmniBox.app/Contents/MacOS/omni-box");
    if installed_app_bin.exists() {
        return Ok(installed_app_bin);
    }
    std::env::current_exe().map_err(|e| e.to_string())
}

#[cfg(target_os = "macos")]
fn apply_macos_launch_at_login(enabled: bool) -> Result<(), String> {
    let home = dirs::home_dir().ok_or("Cannot resolve home directory")?;
    let agents_dir = home.join("Library").join("LaunchAgents");
    let plist_path = agents_dir.join("com.omnibox.launcher.plist");

    if enabled {
        let exe_path = resolve_preferred_autostart_executable()?;
        let exe_str = exe_path.to_string_lossy();
        // Avoid registering a cargo `target/debug` binary into LaunchAgents if `/Applications/OmniBox.app` is not yet installed
        if exe_str.contains("/target/debug/") {
            return Ok(());
        }
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
        <string>--silent</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>ProcessType</key>
    <string>Interactive</string>
</dict>
</plist>
"#,
            exe_str
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

pub struct TrayMenuHandles {
    pub toggle_main_item: tauri::menu::MenuItem<tauri::Wry>,
    pub chk_launch_at_login: tauri::menu::CheckMenuItem<tauri::Wry>,
    pub chk_hide_on_blur: tauri::menu::CheckMenuItem<tauri::Wry>,
    pub chk_super_panel: tauri::menu::CheckMenuItem<tauri::Wry>,
}

pub struct AppState {
    pub db: Mutex<AppDatabase>,
    pub items: Mutex<Vec<SearchableItem>>,
    pub sys_apps: Mutex<Vec<SearchableItem>>,
    pub icon_cache_dir: PathBuf,
    pub global_hotkey: Arc<Mutex<String>>,
    pub tray_handles: Mutex<Option<TrayMenuHandles>>,
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
            "plugin:ip-studio",
            "IP 与网络工作台 (IP & Network Studio)",
            "查询本机 Wi-Fi/有线实际出口 IP、公网出口 IP 归属地/时区/ASN 与 CIDR 子网划分计算器",
            "plugin",
            "Globe",
            None,
            vec![
                "ip",
                "network",
                "cidr",
                "subnet",
                "mask",
                "geoip",
                "公网ip",
                "局域网",
                "子网掩码",
                "网络地址划分",
            ]
            .into_iter()
            .map(String::from)
            .collect(),
            "ip-studio",
        ),
        SearchableItem::new(
            "plugin:unit-converter",
            "换算与格式工作台 (Unit & Format Studio)",
            "长度/面积/体积/质量/温度/时间/速度/数据存储全量同屏换算，以及时间戳/颜色/Base64/财务大写转换",
            "plugin",
            "ArrowLeftRight",
            None,
            vec![
                "unit",
                "convert",
                "converter",
                "format",
                "base64",
                "timestamp",
                "color",
                "rmb",
                "换算",
                "单位换算",
                "格式转换",
                "长度",
                "面积",
                "体积",
                "重量",
                "质量",
                "温度",
                "存储",
                "时间戳",
                "颜色",
                "大写金额",
                "进制",
                "dwhs",
                "gszh",
            ]
            .into_iter()
            .map(String::from)
            .collect(),
            "unit-converter",
        ),
        SearchableItem::new(
            "plugin:ai-studio",
            "AI 助手与指令工作台 (AI Studio)",
            "支持多模型多轮对话、代码解释排障、沉浸式中英互译、Shell/SQL/正则生成与自定义 Prompt 角色",
            "plugin",
            "Sparkles",
            None,
            vec![
                "ai",
                "chat",
                "gpt",
                "gemini",
                "claude",
                "deepseek",
                "translate",
                "prompt",
                "助手",
                "对话",
                "问答",
                "翻译",
                "润色",
                "解释代码",
            ]
            .into_iter()
            .map(String::from)
            .collect(),
            "ai-studio",
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
        sync_macos_activation_policy(&app_handle, Some(&label));
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
        let send_void_usize: unsafe extern "C" fn(*mut c_void, *mut c_void, usize) =
            std::mem::transmute(objc_msgSend as *const ());
        let send_void_isize: unsafe extern "C" fn(*mut c_void, *mut c_void, isize) =
            std::mem::transmute(objc_msgSend as *const ());
        let send_void_bool: unsafe extern "C" fn(*mut c_void, *mut c_void, i8) =
            std::mem::transmute(objc_msgSend as *const ());
        let send_set_frame: unsafe extern "C" fn(*mut c_void, *mut c_void, CGRect, i8) =
            std::mem::transmute(objc_msgSend as *const ());

        // 0. Upgrade ISA to OmniBoxStandaloneWindow so canBecomeMainWindow == YES (required for Cmd+Tab & Dock!)
        let standalone_cls = get_or_register_standalone_window_class();
        if !standalone_cls.is_null() && object_getClass(ns_win) != standalone_cls {
            let _ = object_setClass(ns_win, standalone_cls);
        }

        // 1. Crucial: Never hide or close when clicking other applications!
        let sel_hides = sel_registerName(b"setHidesOnDeactivate:\0".as_ptr() as _);
        send_void_bool(ns_win, sel_hides, 0);

        // 2. Participate in macOS Managed Window Cycle (Cmd+Tab, Cmd+~, Mission Control, Stage Manager)
        //    NSWindowCollectionBehaviorManaged (1 << 2)
        //    | NSWindowCollectionBehaviorParticipatesInCycle (1 << 5)
        //    | NSWindowCollectionBehaviorFullScreenAuxiliary (1 << 8)
        let behavior: usize = (1 << 2) | (1 << 5) | (1 << 8);
        let sel_behavior = sel_registerName(b"setCollectionBehavior:\0".as_ptr() as _);
        send_void_usize(ns_win, sel_behavior, behavior);

        // 3. Default to normal window level (0 = NSNormalWindowLevel) so it does NOT stay in front of
        //    other apps when user clicks other apps (unless user explicitly clicks Pin 📌)
        let sel_level = sel_registerName(b"setLevel:\0".as_ptr() as _);
        send_void_isize(ns_win, sel_level, 0);

        // 4. Center on the current active mouse display
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

        // 5. Activate application in Dock / Cmd+Tab and bring standalone window to front
        raise_standalone_nswindow_to_front(ns_win);
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

    // 0. If an independent window for this exact plugin is ALREADY open, reuse & bring it to the front!
    let prefix = format!("detached_{}_", safe_id);
    if let Some((existing_label, existing_win)) = app_handle
        .webview_windows()
        .into_iter()
        .find(|(k, _)| k.starts_with(&prefix))
    {
        #[cfg(target_os = "macos")]
        {
            let _ = app_handle.set_activation_policy(tauri::ActivationPolicy::Regular);
            DETACHED_WINDOW_COUNT.store(
                app_handle
                    .webview_windows()
                    .keys()
                    .filter(|k| k.starts_with("detached_"))
                    .count() as u32,
                Ordering::Relaxed,
            );
        }
        let handle_for_reuse = app_handle.clone();
        let _ = app_handle.run_on_main_thread(move || {
            if let Some(main_win) = handle_for_reuse.get_webview_window("main") {
                let _ = main_win
                    .eval("window.__omniboxResetState && window.__omniboxResetState()");
                #[cfg(target_os = "macos")]
                if let Ok(ns_main) = main_win.ns_window() {
                    hide_nswindow_native(ns_main);
                }
                let _ = main_win.hide();
            }
            let _ = existing_win.unminimize();
            let _ = existing_win.show();
            #[cfg(target_os = "macos")]
            if let Ok(ns_win) = existing_win.ns_window() {
                raise_standalone_nswindow_to_front(ns_win);
            }
            #[cfg(not(target_os = "macos"))]
            {
                let _ = existing_win.set_focus();
            }
            let _ = existing_win.eval("window.dispatchEvent(new Event('focus'))");
        });
        return Ok(existing_label);
    }

    let label = format!("detached_{}_{}", safe_id, now_unix_ms() % 1_000_000);
    let mut url_str = format!("index.html?standalone={}", plugin_id);
    if let Some(eid) = ext_id {
        if !eid.is_empty() {
            url_str.push_str(&format!("&ext={}", eid));
        }
    }

    #[cfg(target_os = "macos")]
    {
        DETACHED_WINDOW_COUNT.fetch_add(1, Ordering::Relaxed);
        let _ = app_handle.set_activation_policy(tauri::ActivationPolicy::Regular);
    }

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

    // Track native window destruction so Dock icon policy always stays in sync
    #[cfg(target_os = "macos")]
    {
        let handle_for_destroy = app_handle.clone();
        let label_for_destroy = label.clone();
        win.on_window_event(move |event| {
            if matches!(event, tauri::WindowEvent::Destroyed) {
                sync_macos_activation_policy(&handle_for_destroy, Some(&label_for_destroy));
            }
        });
    }

    // 2. Configure standalone window & Dock icon FIRST, then hide main overlay, then re-assert focus!
    let handle_for_main = app_handle.clone();
    let win_clone = win.clone();
    let _ = app_handle.run_on_main_thread(move || {
        #[cfg(target_os = "macos")]
        if let Ok(ns_win) = win_clone.ns_window() {
            configure_standalone_nswindow(ns_win, 1040.0, 680.0);
        }

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
            raise_standalone_nswindow_to_front(ns_win);
        }
    });

    Ok(label)
}

#[tauri::command]
fn update_global_hotkey(hotkey: String, state: State<'_, AppState>) -> Result<(), String> {
    if let Ok(mut hk) = state.global_hotkey.lock() {
        *hk = hotkey.clone();
    }
    if let Ok(guard) = state.tray_handles.lock() {
        if let Some(handles) = guard.as_ref() {
            let _ = handles.toggle_main_item.set_text(format!(
                "🔍 呼出 / 隐藏 OmniBox 主面板 ({})",
                hotkey
            ));
        }
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
    state: State<'_, AppState>,
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

            if let Ok(guard) = state.tray_handles.lock() {
                if let Some(handles) = guard.as_ref() {
                    let _ = handles.chk_launch_at_login.set_checked(launch_at_login);
                    let _ = handles.chk_hide_on_blur.set_checked(hide_on_blur);
                    let _ = handles.chk_super_panel.set_checked(super_panel_enabled);
                }
            }
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
        let mut prev_app_active = is_macos_app_active();
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

            // Detect Command+Tab or Dock activation (`[NSApp isActive]` transitioning false -> true)
            // and automatically raise all standalone detached windows above Chrome / background apps!
            if tick_counter % 3 == 0 {
                let cur_app_active = is_macos_app_active();
                if !prev_app_active
                    && cur_app_active
                    && DETACHED_WINDOW_COUNT.load(Ordering::Relaxed) > 0
                {
                    let handle_for_activate = app_handle.clone();
                    let _ = app_handle.run_on_main_thread(move || {
                        let main_visible = handle_for_activate
                            .get_webview_window("main")
                            .and_then(|w| w.is_visible().ok())
                            .unwrap_or(false);
                        if !main_visible {
                            let _ = bring_all_detached_windows_to_front(&handle_for_activate, false);
                        }
                    });
                }
                prev_app_active = cur_app_active;
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
                // 1. Check default route physical interface first (excluding VPN/tunnel interfaces like utun/ppp/ipsec)
                if let Ok(out) = Command::new("route").args(["-n", "get", "default"]).output() {
                    let text = String::from_utf8_lossy(&out.stdout);
                    for line in text.lines() {
                        if let Some(rest) = line.trim().strip_prefix("interface:") {
                            let iface = rest.trim();
                            let is_vpn_or_virtual = iface.starts_with("utun")
                                || iface.starts_with("tun")
                                || iface.starts_with("tap")
                                || iface.starts_with("ppp")
                                || iface.starts_with("ipsec")
                                || iface.starts_with("bridge")
                                || iface.starts_with("gif")
                                || iface.starts_with("stf");
                            if !iface.is_empty() && !is_vpn_or_virtual {
                                if let Ok(ip_out) =
                                    Command::new("ipconfig").args(["getifaddr", iface]).output()
                                {
                                    let ip =
                                        String::from_utf8_lossy(&ip_out.stdout).trim().to_string();
                                    if !ip.is_empty() && ip != "127.0.0.1" {
                                        return Ok(ip);
                                    }
                                }
                            }
                        }
                    }
                }
                // 2. Fallback to physical hardware interfaces (en0 Wi-Fi / en1..en8 Ethernet) so VPN utun IP is never picked
                for iface in ["en0", "en1", "en2", "en3", "en4", "en5", "en6", "en7", "en8"] {
                    if let Ok(out) = Command::new("ipconfig").args(["getifaddr", iface]).output() {
                        let ip = String::from_utf8_lossy(&out.stdout).trim().to_string();
                        if !ip.is_empty() && ip != "127.0.0.1" {
                            return Ok(ip);
                        }
                    }
                }
                // 3. Last resort fallback via UDP socket
                if let Ok(sock) = std::net::UdpSocket::bind("0.0.0.0:0") {
                    if sock.connect("8.8.8.8:80").is_ok() {
                        if let Ok(local_addr) = sock.local_addr() {
                            let ip_str = local_addr.ip().to_string();
                            if !ip_str.is_empty() && ip_str != "0.0.0.0" && ip_str != "127.0.0.1" {
                                return Ok(ip_str);
                            }
                        }
                    }
                }
                Ok("127.0.0.1".to_string())
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalNetworkInterface {
    pub name: String,
    pub display_name: String,
    pub kind: String,
    pub ipv4: String,
    pub ipv6: Option<String>,
    pub netmask: String,
    pub cidr_prefix: u8,
    pub cidr_notation: String,
    pub broadcast: Option<String>,
    pub mac: Option<String>,
    pub is_primary: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalNetworkOverview {
    pub primary_interface: Option<String>,
    pub primary_display_name: Option<String>,
    pub primary_ipv4: String,
    pub gateway: Option<String>,
    pub dns_servers: Vec<String>,
    pub hostname: String,
    pub interfaces: Vec<LocalNetworkInterface>,
}

fn parse_netmask_token(token: &str) -> (String, u8) {
    let clean = token.trim();
    if let Some(hex_str) = clean.strip_prefix("0x").or_else(|| clean.strip_prefix("0X")) {
        if let Ok(val) = u32::from_str_radix(hex_str, 16) {
            let b1 = (val >> 24) & 0xff;
            let b2 = (val >> 16) & 0xff;
            let b3 = (val >> 8) & 0xff;
            let b4 = val & 0xff;
            let prefix = val.count_ones() as u8;
            return (format!("{}.{}.{}.{}", b1, b2, b3, b4), prefix);
        }
    }
    let parts: Vec<u8> = clean
        .split('.')
        .filter_map(|p| p.parse::<u8>().ok())
        .collect();
    if parts.len() == 4 {
        let val = ((parts[0] as u32) << 24)
            | ((parts[1] as u32) << 16)
            | ((parts[2] as u32) << 8)
            | (parts[3] as u32);
        return (clean.to_string(), val.count_ones() as u8);
    }
    ("255.255.255.0".to_string(), 24)
}

#[tauri::command]
async fn get_local_network_overview() -> Result<LocalNetworkOverview, String> {
    tauri::async_runtime::spawn_blocking(move || -> Result<LocalNetworkOverview, String> {
        let mut default_iface: Option<String> = None;
        let mut default_gateway: Option<String> = None;

        // 1. Parse `route -n get default` for primary active interface and gateway
        if let Ok(out) = Command::new("route").args(["-n", "get", "default"]).output() {
            let text = String::from_utf8_lossy(&out.stdout);
            for line in text.lines() {
                let trimmed = line.trim();
                if let Some(rest) = trimmed.strip_prefix("interface:") {
                    let iface = rest.trim().to_string();
                    if !iface.is_empty() {
                        default_iface = Some(iface);
                    }
                } else if let Some(rest) = trimmed.strip_prefix("gateway:") {
                    let gw = rest.trim().to_string();
                    if !gw.is_empty() {
                        default_gateway = Some(gw);
                    }
                }
            }
        }

        // 2. Parse `networksetup -listallhardwareports` to map `en0` -> `Wi-Fi`, `en4` -> `Ethernet Adapter`
        let mut hw_port_map: std::collections::HashMap<String, String> =
            std::collections::HashMap::new();
        if let Ok(out) = Command::new("networksetup")
            .arg("-listallhardwareports")
            .output()
        {
            let text = String::from_utf8_lossy(&out.stdout);
            let mut cur_port: Option<String> = None;
            for line in text.lines() {
                let trimmed = line.trim();
                if let Some(rest) = trimmed.strip_prefix("Hardware Port:") {
                    cur_port = Some(rest.trim().to_string());
                } else if let Some(rest) = trimmed.strip_prefix("Device:") {
                    let dev = rest.trim().to_string();
                    if let Some(port) = cur_port.take() {
                        hw_port_map.insert(dev, port);
                    }
                }
            }
        }

        // 3. Parse `ifconfig` to collect all active IPv4 interfaces
        let mut interfaces: Vec<LocalNetworkInterface> = Vec::new();
        if let Ok(out) = Command::new("ifconfig").output() {
            let text = String::from_utf8_lossy(&out.stdout);
            let mut cur_name: Option<String> = None;
            let mut cur_mac: Option<String> = None;
            let mut cur_ipv4: Option<String> = None;
            let mut cur_netmask: String = "255.255.255.0".to_string();
            let mut cur_prefix: u8 = 24;
            let mut cur_broadcast: Option<String> = None;
            let mut cur_ipv6: Option<String> = None;

            let flush_iface = |name_opt: &mut Option<String>,
                               mac_opt: &mut Option<String>,
                               ipv4_opt: &mut Option<String>,
                               netmask: &mut String,
                               prefix: &mut u8,
                               broadcast_opt: &mut Option<String>,
                               ipv6_opt: &mut Option<String>,
                               list: &mut Vec<LocalNetworkInterface>| {
                if let (Some(name), Some(ipv4)) = (name_opt.take(), ipv4_opt.take()) {
                    if name != "lo0" && ipv4 != "127.0.0.1" {
                        let raw_hw = hw_port_map.get(&name).cloned().unwrap_or_default();
                        let lower_hw = raw_hw.to_lowercase();
                        let (kind, display_name) = if lower_hw.contains("wi-fi")
                            || lower_hw.contains("airport")
                        {
                            ("wifi".to_string(), format!("Wi-Fi 无线网络 ({})", name))
                        } else if lower_hw.contains("ethernet")
                            || lower_hw.contains("lan")
                            || lower_hw.contains("thunderbolt") && !lower_hw.contains("bridge")
                        {
                            (
                                "ethernet".to_string(),
                                format!("{} ({})", raw_hw, name),
                            )
                        } else if name.starts_with("utun")
                            || name.starts_with("ppp")
                            || name.starts_with("ipsec")
                            || name.starts_with("tun")
                            || name.starts_with("tap")
                        {
                            (
                                "vpn".to_string(),
                                format!("VPN / 代理隧道接口 ({})", name),
                            )
                        } else if name.starts_with("bridge") || lower_hw.contains("bridge") {
                            (
                                "bridge".to_string(),
                                format!("虚拟网桥 / 共享网络 ({})", name),
                            )
                        } else if !raw_hw.is_empty() {
                            ("ethernet".to_string(), format!("{} ({})", raw_hw, name))
                        } else {
                            ("other".to_string(), format!("网络接口 ({})", name))
                        };

                        // Never mark VPN (`utun`/`ppp`/`tun`) or virtual bridge as the primary LAN egress interface
                        let is_primary = default_iface.as_deref() == Some(name.as_str())
                            && kind != "vpn"
                            && kind != "bridge";
                        let cidr_notation = format!("{}/{}", ipv4, *prefix);
                        list.push(LocalNetworkInterface {
                            name,
                            display_name,
                            kind,
                            ipv4,
                            ipv6: ipv6_opt.take(),
                            netmask: netmask.clone(),
                            cidr_prefix: *prefix,
                            cidr_notation,
                            broadcast: broadcast_opt.take(),
                            mac: mac_opt.take(),
                            is_primary,
                        });
                    }
                }
                *mac_opt = None;
                *ipv4_opt = None;
                *netmask = "255.255.255.0".to_string();
                *prefix = 24;
                *broadcast_opt = None;
                *ipv6_opt = None;
            };

            for line in text.lines() {
                if !line.starts_with('\t') && !line.starts_with(' ') && line.contains(':') {
                    flush_iface(
                        &mut cur_name,
                        &mut cur_mac,
                        &mut cur_ipv4,
                        &mut cur_netmask,
                        &mut cur_prefix,
                        &mut cur_broadcast,
                        &mut cur_ipv6,
                        &mut interfaces,
                    );
                    if let Some((iface_token, _)) = line.split_once(':') {
                        cur_name = Some(iface_token.trim().to_string());
                    }
                } else {
                    let trimmed = line.trim();
                    if let Some(rest) = trimmed.strip_prefix("ether ") {
                        cur_mac = Some(rest.trim().to_string());
                    } else if let Some(rest) = trimmed.strip_prefix("inet ") {
                        let tokens: Vec<&str> = rest.split_whitespace().collect();
                        if !tokens.is_empty() {
                            cur_ipv4 = Some(tokens[0].to_string());
                            for i in 0..tokens.len() {
                                if tokens[i] == "netmask" && i + 1 < tokens.len() {
                                    let (nm, pf) = parse_netmask_token(tokens[i + 1]);
                                    cur_netmask = nm;
                                    cur_prefix = pf;
                                } else if tokens[i] == "broadcast" && i + 1 < tokens.len() {
                                    cur_broadcast = Some(tokens[i + 1].to_string());
                                }
                            }
                        }
                    } else if let Some(rest) = trimmed.strip_prefix("inet6 ") {
                        let tokens: Vec<&str> = rest.split_whitespace().collect();
                        if !tokens.is_empty() {
                            let raw_v6 = tokens[0].split('%').next().unwrap_or(tokens[0]);
                            if cur_ipv6.is_none() || !raw_v6.starts_with("fe80:") {
                                cur_ipv6 = Some(raw_v6.to_string());
                            }
                        }
                    }
                }
            }
            flush_iface(
                &mut cur_name,
                &mut cur_mac,
                &mut cur_ipv4,
                &mut cur_netmask,
                &mut cur_prefix,
                &mut cur_broadcast,
                &mut cur_ipv6,
                &mut interfaces,
            );
        }

        // If default_iface is a VPN `utun` without a matching entry or if no interface was marked primary yet,
        // mark the first physical interface (`wifi` or `ethernet`) as primary fallback
        if !interfaces.iter().any(|i| i.is_primary) {
            if let Some(first_phys) = interfaces
                .iter_mut()
                .find(|i| i.kind == "wifi" || i.kind == "ethernet")
            {
                first_phys.is_primary = true;
                if default_iface.is_none() {
                    default_iface = Some(first_phys.name.clone());
                }
            } else if let Some(first_any) = interfaces.first_mut() {
                first_any.is_primary = true;
            }
        }

        // Sort so primary interface comes first, then Wi-Fi / Ethernet, then VPN / Bridge
        interfaces.sort_by_key(|i| {
            let prio = if i.is_primary {
                0
            } else if i.kind == "wifi" {
                1
            } else if i.kind == "ethernet" {
                2
            } else if i.kind == "vpn" {
                3
            } else {
                4
            };
            (prio, i.name.clone())
        });

        let primary_item = interfaces.first().cloned();
        let primary_ipv4 = primary_item
            .as_ref()
            .map(|i| i.ipv4.clone())
            .unwrap_or_else(|| "127.0.0.1".to_string());
        let primary_display_name = primary_item.as_ref().map(|i| i.display_name.clone());
        let primary_interface = primary_item
            .as_ref()
            .map(|i| i.name.clone())
            .or(default_iface);

        // 4. Parse DNS servers from `scutil --dns`
        let mut dns_servers: Vec<String> = Vec::new();
        if let Ok(out) = Command::new("scutil").arg("--dns").output() {
            let text = String::from_utf8_lossy(&out.stdout);
            for line in text.lines() {
                let trimmed = line.trim();
                if trimmed.starts_with("nameserver[") {
                    if let Some((_, rhs)) = trimmed.split_once(':') {
                        let ip = rhs.trim().to_string();
                        if !ip.is_empty() && !dns_servers.contains(&ip) {
                            dns_servers.push(ip);
                            if dns_servers.len() >= 4 {
                                break;
                            }
                        }
                    }
                }
            }
        }

        // 5. Get local hostname
        let mut hostname = "Mac".to_string();
        if let Ok(out) = Command::new("scutil").args(["--get", "LocalHostName"]).output() {
            let h = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !h.is_empty() {
                hostname = h;
            }
        }

        Ok(LocalNetworkOverview {
            primary_interface,
            primary_display_name,
            primary_ipv4,
            gateway: default_gateway,
            dns_servers,
            hostname,
            interfaces,
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn fetch_url_json_via_curl(url: String) -> Result<String, String> {
    let allowed_hosts = [
        "https://get.geojs.io/",
        "http://ip-api.com/",
        "https://ipwho.is/",
        "https://ipinfo.io/",
        "https://myip.ipip.net/",
        "https://api.ipify.org",
    ];
    if !allowed_hosts.iter().any(|prefix| url.starts_with(prefix)) {
        return Err("不允许的请求域名".to_string());
    }

    tauri::async_runtime::spawn_blocking(move || -> Result<String, String> {
        let out = Command::new("/usr/bin/curl")
            .args(["-s", "-L", "-m", "4", "-A", "OmniBox/2.0", &url])
            .output()
            .map_err(|e| e.to_string())?;
        if !out.status.success() {
            return Err("请求超时或网络不可达".to_string());
        }
        let body = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if body.is_empty() {
            return Err("返回内容为空".to_string());
        }
        Ok(body)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemIntegrationStatus {
    pub installed_in_applications: bool,
    pub applications_bundle_path: String,
    pub current_executable_path: String,
    pub is_running_from_applications: bool,
    pub launch_agent_plist_path: String,
    pub launch_agent_configured: bool,
    pub tray_active: bool,
}

#[tauri::command]
fn get_system_integration_status() -> Result<SystemIntegrationStatus, String> {
    let app_bundle = PathBuf::from("/Applications/OmniBox.app");
    let installed_in_applications = app_bundle.exists();
    let cur_exe = std::env::current_exe()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();
    let is_running_from_applications = cur_exe.starts_with("/Applications/OmniBox.app");

    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    let plist_path = home
        .join("Library")
        .join("LaunchAgents")
        .join("com.omnibox.launcher.plist");
    let launch_agent_configured = plist_path.exists();

    Ok(SystemIntegrationStatus {
        installed_in_applications,
        applications_bundle_path: "/Applications/OmniBox.app".to_string(),
        current_executable_path: cur_exe,
        is_running_from_applications,
        launch_agent_plist_path: plist_path.to_string_lossy().to_string(),
        launch_agent_configured,
        tray_active: true,
    })
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiChatMessageInput {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiChatCompletionResponse {
    pub content: String,
    pub reasoning_content: Option<String>,
    pub model: String,
    pub latency_ms: u64,
    pub total_tokens: Option<u64>,
}

fn normalize_openai_endpoint(raw_base: &str, default_host: &str, suffix_path: &str) -> String {
    let base = if raw_base.trim().is_empty() {
        default_host.trim_end_matches('/').to_string()
    } else {
        raw_base.trim().trim_end_matches('/').to_string()
    };

    if suffix_path == "/chat/completions" {
        if base.ends_with("/chat/completions") {
            return base;
        }
        if base.ends_with("/v1") || base.ends_with("/v1beta/openai") {
            return format!("{}/chat/completions", base);
        }
        return format!("{}/v1/chat/completions", base);
    }

    if suffix_path == "/models" {
        if base.ends_with("/models") {
            return base;
        }
        if base.ends_with("/chat/completions") {
            return format!(
                "{}/models",
                base.trim_end_matches("/chat/completions")
            );
        }
        if base.ends_with("/v1") || base.ends_with("/v1beta/openai") {
            return format!("{}/models", base);
        }
        return format!("{}/v1/models", base);
    }

    format!("{}{}", base, suffix_path)
}

fn parse_ai_response_body(raw_body: &str, fallback_model: &str) -> Result<(String, Option<String>, String, Option<u64>), String> {
    let trimmed = raw_body.trim();
    if trimmed.is_empty() {
        return Err("AI 接口返回内容为空，请检查 API Endpoint 与密钥是否正确".to_string());
    }

    // Case 1: Server returned SSE stream (`data: {...}`) even when `stream: false` was requested
    if trimmed.starts_with("data:") || trimmed.contains("\ndata:") {
        let mut combined_content = String::new();
        let mut combined_reasoning = String::new();
        let mut detected_model = fallback_model.to_string();
        let mut stream_tokens: Option<u64> = None;

        for line in trimmed.lines() {
            let l = line.trim();
            if let Some(rest) = l.strip_prefix("data:") {
                let data_str = rest.trim();
                if data_str == "[DONE]" || data_str.is_empty() {
                    continue;
                }
                if let Ok(val) = serde_json::from_str::<serde_json::Value>(data_str) {
                    if let Some(err_msg) = val["error"]["message"].as_str() {
                        return Err(format!("AI 接口错误: {}", err_msg));
                    }
                    if let Some(m) = val["model"].as_str() {
                        if !m.is_empty() {
                            detected_model = m.to_string();
                        }
                    }
                    if let Some(tok) = val["usage"]["total_tokens"].as_u64() {
                        stream_tokens = Some(tok);
                    }
                    if let Some(choices) = val["choices"].as_array() {
                        if let Some(first) = choices.first() {
                            if let Some(c) = first["delta"]["content"]
                                .as_str()
                                .or_else(|| first["message"]["content"].as_str())
                            {
                                combined_content.push_str(c);
                            }
                            if let Some(r) = first["delta"]["reasoning_content"]
                                .as_str()
                                .or_else(|| first["delta"]["reasoning"].as_str())
                                .or_else(|| first["message"]["reasoning_content"].as_str())
                            {
                                combined_reasoning.push_str(r);
                            }
                        }
                    }
                }
            }
        }

        if !combined_content.is_empty() || !combined_reasoning.is_empty() {
            let r_opt = if combined_reasoning.trim().is_empty() {
                None
            } else {
                Some(combined_reasoning)
            };
            return Ok((combined_content, r_opt, detected_model, stream_tokens));
        }
    }

    // Case 2: Standard JSON response (OpenAI / Gemini / Anthropic Claude / Ollama)
    let json_val: serde_json::Value = serde_json::from_str(trimmed).map_err(|_| {
        let preview: String = trimmed.chars().take(240).collect();
        format!("AI 接口返回非标准 JSON 响应: {}", preview)
    })?;

    if let Some(err_obj) = json_val.get("error") {
        let msg = err_obj["message"]
            .as_str()
            .or_else(|| err_obj.as_str())
            .unwrap_or("未知接口错误");
        return Err(format!("AI 接口报错: {}", msg));
    }

    let detected_model = json_val["model"]
        .as_str()
        .or_else(|| json_val["modelVersion"].as_str())
        .unwrap_or(fallback_model)
        .to_string();

    let total_tokens = json_val["usage"]["total_tokens"]
        .as_u64()
        .or_else(|| json_val["usageMetadata"]["totalTokenCount"].as_u64());

    // 2a. OpenAI / DeepSeek / Sub2API / OneAPI format (`choices[0].message.content`)
    if let Some(choices) = json_val["choices"].as_array() {
        if let Some(first) = choices.first() {
            let content = first["message"]["content"]
                .as_str()
                .or_else(|| first["text"].as_str())
                .unwrap_or("")
                .to_string();
            let reasoning = first["message"]["reasoning_content"]
                .as_str()
                .or_else(|| first["message"]["reasoning"].as_str())
                .map(|s| s.to_string())
                .filter(|s| !s.trim().is_empty());
            if !content.is_empty() || reasoning.is_some() {
                return Ok((content, reasoning, detected_model, total_tokens));
            }
        }
    }

    // 2b. Google Gemini native format (`candidates[0].content.parts`)
    if let Some(candidates) = json_val["candidates"].as_array() {
        if let Some(first) = candidates.first() {
            if let Some(parts) = first["content"]["parts"].as_array() {
                let mut text_buf = String::new();
                for p in parts {
                    if let Some(t) = p["text"].as_str() {
                        text_buf.push_str(t);
                    }
                }
                if !text_buf.is_empty() {
                    return Ok((text_buf, None, detected_model, total_tokens));
                }
            }
        }
    }

    // 2c. Anthropic Claude native format (`content[0].text`)
    if let Some(content_arr) = json_val["content"].as_array() {
        let mut text_buf = String::new();
        let mut thinking_buf = String::new();
        for item in content_arr {
            if item["type"].as_str() == Some("thinking") {
                if let Some(t) = item["thinking"].as_str() {
                    thinking_buf.push_str(t);
                }
            } else if let Some(t) = item["text"].as_str() {
                text_buf.push_str(t);
            }
        }
        if !text_buf.is_empty() || !thinking_buf.is_empty() {
            let r_opt = if thinking_buf.trim().is_empty() {
                None
            } else {
                Some(thinking_buf)
            };
            return Ok((text_buf, r_opt, detected_model, total_tokens));
        }
    }

    // 2d. Ollama `/api/chat` fallback (`message.content`)
    if let Some(c) = json_val["message"]["content"].as_str() {
        return Ok((c.to_string(), None, detected_model, total_tokens));
    }

    Err(format!(
        "未能从响应中解析出有效内容: {}",
        trimmed.chars().take(200).collect::<String>()
    ))
}

#[tauri::command]
async fn ai_chat_completion(
    provider: String,
    base_url: String,
    api_key: String,
    model: String,
    messages: Vec<AiChatMessageInput>,
    temperature: Option<f64>,
    proxy_url: Option<String>,
    stream_id: Option<String>,
    app_handle: AppHandle,
) -> Result<AiChatCompletionResponse, String> {
    tauri::async_runtime::spawn_blocking(move || -> Result<AiChatCompletionResponse, String> {
        use std::io::{BufRead, BufReader, Write};
        use tauri::Emitter;
        let start = Instant::now();
        let clean_key = api_key.trim().to_string();
        let clean_base = base_url.trim().trim_end_matches('/').to_string();
        let clean_model = if model.trim().is_empty() {
            "gpt-4o-mini".to_string()
        } else {
            model.trim().to_string()
        };
        let temp = temperature.unwrap_or(0.7);

        let is_native_gemini = provider == "Google Gemini"
            && (clean_base.is_empty() || clean_base.contains("googleapis.com"))
            && !clean_base.ends_with("/v1");

        let is_native_claude = provider == "Anthropic Claude"
            && (clean_base.is_empty() || clean_base.contains("anthropic.com"))
            && !clean_base.ends_with("/v1");

        let mut curl_args: Vec<String> = vec![
            "-s".to_string(),
            "-S".to_string(),
            "-L".to_string(),
            "-N".to_string(),
            "--connect-timeout".to_string(),
            "15".to_string(),
            "--speed-time".to_string(),
            "45".to_string(),
            "--speed-limit".to_string(),
            "1".to_string(),
            "--max-time".to_string(),
            "240".to_string(),
            "-H".to_string(),
            "Content-Type: application/json".to_string(),
            "-H".to_string(),
            "Accept: text/event-stream, application/json".to_string(),
            "-A".to_string(),
            "OmniBox-AI-Client/2.0".to_string(),
        ];

        if let Some(px) = proxy_url {
            let p_trim = px.trim();
            if !p_trim.is_empty() {
                curl_args.push("-x".to_string());
                curl_args.push(p_trim.to_string());
            }
        }

        let (target_url, payload_str) = if is_native_gemini {
            let host = if clean_base.is_empty() {
                "https://generativelanguage.googleapis.com"
            } else {
                clean_base.as_str()
            };
            let url = format!(
                "{}/v1beta/models/{}:generateContent?key={}",
                host, clean_model, clean_key
            );
            let mut sys_text = String::new();
            let mut contents = Vec::new();
            for m in &messages {
                if m.role == "system" {
                    if !sys_text.is_empty() {
                        sys_text.push('\n');
                    }
                    sys_text.push_str(&m.content);
                } else {
                    let g_role = if m.role == "assistant" { "model" } else { "user" };
                    contents.push(serde_json::json!({
                        "role": g_role,
                        "parts": [{ "text": m.content }]
                    }));
                }
            }
            let mut body_json = serde_json::json!({
                "contents": contents,
                "generationConfig": {
                    "temperature": temp
                }
            });
            if !sys_text.is_empty() {
                body_json["systemInstruction"] = serde_json::json!({
                    "parts": [{ "text": sys_text }]
                });
            }
            (url, body_json.to_string())
        } else if is_native_claude {
            let host = if clean_base.is_empty() {
                "https://api.anthropic.com"
            } else {
                clean_base.as_str()
            };
            let url = format!("{}/v1/messages", host.trim_end_matches("/v1"));
            curl_args.push("-H".to_string());
            curl_args.push(format!("x-api-key: {}", clean_key));
            curl_args.push("-H".to_string());
            curl_args.push("anthropic-version: 2023-06-01".to_string());

            let mut sys_text = String::new();
            let mut claude_msgs = Vec::new();
            for m in &messages {
                if m.role == "system" {
                    if !sys_text.is_empty() {
                        sys_text.push('\n');
                    }
                    sys_text.push_str(&m.content);
                } else {
                    claude_msgs.push(serde_json::json!({
                        "role": if m.role == "assistant" { "assistant" } else { "user" },
                        "content": m.content
                    }));
                }
            }
            let mut body_json = serde_json::json!({
                "model": clean_model,
                "max_tokens": 4096,
                "temperature": temp,
                "messages": claude_msgs
            });
            if !sys_text.is_empty() {
                body_json["system"] = serde_json::Value::String(sys_text);
            }
            (url, body_json.to_string())
        } else {
            // OpenAI / DeepSeek / Ollama / Sub2API / OneAPI / Any OpenAI-compatible gateway:
            // Always use `"stream": true` so gateways like Sub2API (`gpt-6.1-sol`) immediately flush SSE chunks
            // instead of blocking with 0 bytes until timeout!
            let default_host = match provider.as_str() {
                "DeepSeek" => "https://api.deepseek.com/v1",
                "Ollama 本地模型" => "http://127.0.0.1:11434/v1",
                "Google Gemini" => "https://generativelanguage.googleapis.com/v1beta/openai",
                _ => "https://api.openai.com/v1",
            };
            let url = normalize_openai_endpoint(&clean_base, default_host, "/chat/completions");
            if !clean_key.is_empty() {
                curl_args.push("-H".to_string());
                curl_args.push(format!("Authorization: Bearer {}", clean_key));
            }
            let oai_msgs: Vec<serde_json::Value> = messages
                .iter()
                .map(|m| {
                    serde_json::json!({
                        "role": m.role,
                        "content": m.content
                    })
                })
                .collect();
            let body_json = serde_json::json!({
                "model": clean_model,
                "messages": oai_msgs,
                "temperature": temp,
                "stream": true
            });
            (url, body_json.to_string())
        };

        curl_args.push("--data-binary".to_string());
        curl_args.push("@-".to_string());
        curl_args.push(target_url);

        let mut child = Command::new("/usr/bin/curl")
            .args(&curl_args)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| format!("无法启动网络请求组件: {}", e))?;

        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(payload_str.as_bytes());
        }

        let mut raw_stdout = String::new();
        let mut live_content = String::new();
        let mut live_reasoning = String::new();
        let mut live_model = clean_model.clone();
        let mut last_emit = Instant::now();

        if let Some(stdout) = child.stdout.take() {
            let reader = BufReader::new(stdout);
            for line_res in reader.lines() {
                if let Ok(line) = line_res {
                    raw_stdout.push_str(&line);
                    raw_stdout.push('\n');

                    let trimmed_line = line.trim();
                    if let Some(rest) = trimmed_line.strip_prefix("data:") {
                        let data_str = rest.trim();
                        if data_str != "[DONE]" && !data_str.is_empty() {
                            if let Ok(val) = serde_json::from_str::<serde_json::Value>(data_str) {
                                if let Some(m) = val["model"].as_str() {
                                    if !m.is_empty() {
                                        live_model = m.to_string();
                                    }
                                }
                                let mut chunk_added = false;
                                if let Some(choices) = val["choices"].as_array() {
                                    if let Some(first) = choices.first() {
                                        if let Some(c) = first["delta"]["content"]
                                            .as_str()
                                            .or_else(|| first["message"]["content"].as_str())
                                        {
                                            if !c.is_empty() {
                                                live_content.push_str(c);
                                                chunk_added = true;
                                            }
                                        }
                                        if let Some(r) = first["delta"]["reasoning_content"]
                                            .as_str()
                                            .or_else(|| first["delta"]["reasoning"].as_str())
                                            .or_else(|| first["message"]["reasoning_content"].as_str())
                                        {
                                            if !r.is_empty() {
                                                live_reasoning.push_str(r);
                                                chunk_added = true;
                                            }
                                        }
                                    }
                                }
                                if chunk_added {
                                    if let Some(ref sid) = stream_id {
                                        if last_emit.elapsed().as_millis() >= 35 {
                                            last_emit = Instant::now();
                                            let _ = app_handle.emit(
                                                "ai-stream-chunk",
                                                serde_json::json!({
                                                    "streamId": sid,
                                                    "content": live_content,
                                                    "reasoningContent": if live_reasoning.is_empty() { None } else { Some(&live_reasoning) },
                                                    "model": live_model
                                                }),
                                            );
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        let out = child
            .wait_with_output()
            .map_err(|e| format!("等待 AI 响应失败: {}", e))?;

        // Even if curl exited with non-zero (e.g. partial stream interrupted), if we already received valid SSE content, return it gracefully!
        if !out.status.success() && live_content.is_empty() && live_reasoning.is_empty() {
            let err_str = String::from_utf8_lossy(&out.stderr).trim().to_string();
            return Err(format!("网络连接异常: {}", err_str));
        }

        let (content, reasoning_content, used_model, total_tokens) =
            parse_ai_response_body(&raw_stdout, &clean_model)?;

        let latency_ms = start.elapsed().as_millis() as u64;
        Ok(AiChatCompletionResponse {
            content,
            reasoning_content,
            model: used_model,
            latency_ms,
            total_tokens,
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn ai_fetch_models(
    provider: String,
    base_url: String,
    api_key: String,
    proxy_url: Option<String>,
) -> Result<Vec<String>, String> {
    tauri::async_runtime::spawn_blocking(move || -> Result<Vec<String>, String> {
        let clean_key = api_key.trim().to_string();
        let clean_base = base_url.trim().trim_end_matches('/').to_string();

        let is_native_gemini = provider == "Google Gemini"
            && (clean_base.is_empty() || clean_base.contains("googleapis.com"))
            && !clean_base.ends_with("/v1");

        let mut curl_args: Vec<String> = vec![
            "-s".to_string(),
            "-S".to_string(),
            "-L".to_string(),
            "--max-time".to_string(),
            "15".to_string(),
            "-H".to_string(),
            "Accept: application/json".to_string(),
            "-A".to_string(),
            "OmniBox-AI-Client/2.0".to_string(),
        ];

        if let Some(px) = proxy_url {
            let p_trim = px.trim();
            if !p_trim.is_empty() {
                curl_args.push("-x".to_string());
                curl_args.push(p_trim.to_string());
            }
        }

        let target_url = if is_native_gemini {
            let host = if clean_base.is_empty() {
                "https://generativelanguage.googleapis.com"
            } else {
                clean_base.as_str()
            };
            format!("{}/v1beta/models?key={}", host, clean_key)
        } else {
            let default_host = match provider.as_str() {
                "DeepSeek" => "https://api.deepseek.com/v1",
                "Ollama 本地模型" => "http://127.0.0.1:11434/v1",
                "Anthropic Claude" => "https://api.anthropic.com/v1",
                _ => "https://api.openai.com/v1",
            };
            if !clean_key.is_empty() {
                curl_args.push("-H".to_string());
                curl_args.push(format!("Authorization: Bearer {}", clean_key));
                if provider == "Anthropic Claude" && clean_base.contains("anthropic.com") {
                    curl_args.push("-H".to_string());
                    curl_args.push(format!("x-api-key: {}", clean_key));
                    curl_args.push("-H".to_string());
                    curl_args.push("anthropic-version: 2023-06-01".to_string());
                }
            }
            normalize_openai_endpoint(&clean_base, default_host, "/models")
        };

        curl_args.push(target_url);

        let out = Command::new("/usr/bin/curl")
            .args(&curl_args)
            .output()
            .map_err(|e| format!("请求模型列表失败: {}", e))?;

        if !out.status.success() {
            let err_str = String::from_utf8_lossy(&out.stderr).trim().to_string();
            return Err(format!("无法连接至 Endpoint: {}", err_str));
        }

        let body = String::from_utf8_lossy(&out.stdout).trim().to_string();
        let json_val: serde_json::Value =
            serde_json::from_str(&body).map_err(|_| "返回格式非标准 JSON".to_string())?;

        if let Some(err_msg) = json_val["error"]["message"].as_str() {
            return Err(format!("服务端返回错误: {}", err_msg));
        }

        let mut models: Vec<String> = Vec::new();
        if let Some(data_arr) = json_val["data"].as_array() {
            for item in data_arr {
                if let Some(id) = item["id"].as_str() {
                    models.push(id.to_string());
                }
            }
        } else if let Some(models_arr) = json_val["models"].as_array() {
            for item in models_arr {
                if let Some(name) = item["name"].as_str().or_else(|| item["id"].as_str()) {
                    models.push(name.trim_start_matches("models/").to_string());
                }
            }
        }

        if models.is_empty() {
            return Err("接口已连通，但未返回可用模型列表".to_string());
        }
        Ok(models)
    })
    .await
    .map_err(|e| e.to_string())?
}

fn summon_and_open_plugin_in_main(app_handle: &AppHandle, plugin_id: &str) {
    if let Some(win) = app_handle.get_webview_window("main") {
        let _ = win.unminimize();
        #[cfg(target_os = "macos")]
        {
            summon_macos_overlay_on_current_space_and_screen(&win);
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = win.show();
            let _ = win.set_focus();
        }
        let script = format!(
            "window.__omniboxOpenPlugin ? window.__omniboxOpenPlugin('{}') : null",
            plugin_id
        );
        let _ = win.eval(&script);
    }
}

fn update_saved_user_setting_bool(app_handle: &AppHandle, field: &str, value: bool) {
    if let Some(state) = app_handle.try_state::<AppState>() {
        if let Ok(db) = state.db.lock() {
            let mut root = db
                .kv_get("system", "user-settings")
                .ok()
                .flatten()
                .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
                .unwrap_or_else(|| serde_json::json!({}));
            if let Some(obj) = root.as_object_mut() {
                obj.insert(field.to_string(), serde_json::Value::Bool(value));
                if let Ok(serialized) = serde_json::to_string(&root) {
                    let _ = db.kv_put("system", "user-settings", &serialized);
                }
            }
        }
    }
    if let Some(win) = app_handle.get_webview_window("main") {
        let payload = serde_json::json!({
            field: value
        });
        let _ = win.eval(&format!(
            "window.__omniboxOnNativeSettingChanged && window.__omniboxOnNativeSettingChanged({})",
            payload
        ));
    }
}

fn setup_system_tray(
    app: &tauri::App,
    initial_hotkey: &str,
    initial_launch_at_login: bool,
    initial_hide_on_blur: bool,
    initial_super_panel: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    use tauri::menu::{
        CheckMenuItemBuilder, MenuBuilder, MenuItemBuilder, PredefinedMenuItem, SubmenuBuilder,
    };
    use tauri::tray::TrayIconBuilder;

    let toggle_main_item = MenuItemBuilder::with_id(
        "tray_toggle_main",
        format!("🔍 呼出 / 隐藏 OmniBox 主面板 ({})", initial_hotkey),
    )
    .build(app)?;

    let super_panel_item =
        MenuItemBuilder::with_id("tray_open_super_panel", "✨ 呼出超级面板 (Super Panel)")
            .build(app)?;

    // Submenu 1: Open Built-in Workbenches in Main Window
    let sub_workbenches = SubmenuBuilder::new(app, "⚡ 快捷打开内置工作台")
        .item(
            &MenuItemBuilder::with_id("open_plugin:ai-studio", "🤖 AI 助手与指令工作台 (AI Studio)")
                .build(app)?,
        )
        .item(
            &MenuItemBuilder::with_id("open_plugin:calc-pad", "🧮 计算稿纸 (Calc Pad)").build(app)?,
        )
        .item(
            &MenuItemBuilder::with_id("open_plugin:ip-studio", "🌐 IP 与网络工作台 (IP Studio)")
                .build(app)?,
        )
        .item(
            &MenuItemBuilder::with_id(
                "open_plugin:unit-converter",
                "⚖️ 换算与格式工作台 (Unit & Format)",
            )
            .build(app)?,
        )
        .item(
            &MenuItemBuilder::with_id("open_plugin:json-studio", "🧩 JSON 助手 (JSON Studio)")
                .build(app)?,
        )
        .item(
            &MenuItemBuilder::with_id("open_plugin:diff-viewer", "🔀 文本对比 (Diff Viewer)")
                .build(app)?,
        )
        .item(
            &MenuItemBuilder::with_id(
                "open_plugin:authenticator",
                "🛡️ 2FA 验证码管理 (Authenticator)",
            )
            .build(app)?,
        )
        .item(
            &MenuItemBuilder::with_id("open_plugin:password-vault", "🔐 密码本 (Password Vault)")
                .build(app)?,
        )
        .item(
            &MenuItemBuilder::with_id("open_plugin:utools-hub", "🧱 uTools 插件中心 (Plugin Hub)")
                .build(app)?,
        )
        .build()?;

    // Submenu 2: Spawn Independent Detached Windows
    let sub_detached = SubmenuBuilder::new(app, "🪟 以独立窗口打开工具")
        .item(
            &MenuItemBuilder::with_id("detach_plugin:ai-studio", "🤖 独立窗口：AI 助手与指令工作台")
                .build(app)?,
        )
        .item(
            &MenuItemBuilder::with_id("detach_plugin:calc-pad", "🧮 独立窗口：计算稿纸")
                .build(app)?,
        )
        .item(
            &MenuItemBuilder::with_id("detach_plugin:ip-studio", "🌐 独立窗口：IP 与网络工作台")
                .build(app)?,
        )
        .item(
            &MenuItemBuilder::with_id(
                "detach_plugin:unit-converter",
                "⚖️ 独立窗口：换算与格式工作台",
            )
            .build(app)?,
        )
        .item(
            &MenuItemBuilder::with_id("detach_plugin:json-studio", "🧩 独立窗口：JSON 助手")
                .build(app)?,
        )
        .item(
            &MenuItemBuilder::with_id("detach_plugin:diff-viewer", "🔀 独立窗口：文本对比")
                .build(app)?,
        )
        .item(
            &MenuItemBuilder::with_id("detach_plugin:password-vault", "🔐 独立窗口：密码本")
                .build(app)?,
        )
        .build()?;

    // Submenu 3: Quick System Commands
    let sub_sys_cmds = SubmenuBuilder::new(app, "🛠️ 本机快捷系统指令")
        .item(
            &MenuItemBuilder::with_id("sys_cmd:sys:copy-local-ip", "📋 复制本机局域网出口 IP")
                .build(app)?,
        )
        .item(
            &MenuItemBuilder::with_id(
                "sys_cmd:sys:toggle-dark-mode",
                "🌓 切换 macOS 系统深浅色模式",
            )
            .build(app)?,
        )
        .item(
            &MenuItemBuilder::with_id(
                "sys_cmd:sys:show-hidden-files",
                "👁️ 切换 Finder 显示隐藏文件",
            )
            .build(app)?,
        )
        .item(&MenuItemBuilder::with_id("sys_cmd:sys:empty-trash", "🗑️ 清空废纸篓").build(app)?)
        .item(&MenuItemBuilder::with_id("sys_cmd:sys:lock-screen", "🔒 立即锁定屏幕").build(app)?)
        .build()?;

    // Live CheckMenuItems
    let chk_launch_at_login = CheckMenuItemBuilder::with_id(
        "tray_chk_launch_at_login",
        "开机自动静默启动 (Launch at Login)",
    )
    .checked(initial_launch_at_login)
    .build(app)?;

    let chk_hide_on_blur =
        CheckMenuItemBuilder::with_id("tray_chk_hide_on_blur", "失去焦点时自动隐藏主窗口")
            .checked(initial_hide_on_blur)
            .build(app)?;

    let chk_super_panel =
        CheckMenuItemBuilder::with_id("tray_chk_super_panel", "启用鼠标长按呼出超级面板")
            .checked(initial_super_panel)
            .build(app)?;

    let rescan_item =
        MenuItemBuilder::with_id("tray_rescan_apps", "🔄 重新扫描本机应用索引").build(app)?;
    let settings_item =
        MenuItemBuilder::with_id("tray_open_settings", "⚙️ 个人中心与偏好设置... (⌘,)")
            .build(app)?;
    let quit_item =
        MenuItemBuilder::with_id("tray_quit", "🚪 完全退出 OmniBox (⌘Q)").build(app)?;

    let sep1 = PredefinedMenuItem::separator(app)?;
    let sep2 = PredefinedMenuItem::separator(app)?;
    let sep3 = PredefinedMenuItem::separator(app)?;
    let sep4 = PredefinedMenuItem::separator(app)?;

    let tray_menu = MenuBuilder::new(app)
        .items(&[
            &toggle_main_item,
            &super_panel_item,
            &sep1,
            &sub_workbenches,
            &sub_detached,
            &sub_sys_cmds,
            &sep2,
            &chk_launch_at_login,
            &chk_hide_on_blur,
            &chk_super_panel,
            &sep3,
            &rescan_item,
            &settings_item,
            &sep4,
            &quit_item,
        ])
        .build()?;

    // Save handles into AppState so preferences page can sync with Tray CheckMenuItems
    if let Some(state) = app.try_state::<AppState>() {
        if let Ok(mut guard) = state.tray_handles.lock() {
            *guard = Some(TrayMenuHandles {
                toggle_main_item: toggle_main_item.clone(),
                chk_launch_at_login: chk_launch_at_login.clone(),
                chk_hide_on_blur: chk_hide_on_blur.clone(),
                chk_super_panel: chk_super_panel.clone(),
            });
        }
    }

    let tray_icon_img = tauri::image::Image::from_bytes(include_bytes!("../icons/64x64.png"))
        .ok()
        .or_else(|| app.default_window_icon().cloned());

    let mut builder = TrayIconBuilder::with_id("omnibox-status-tray")
        .menu(&tray_menu)
        .show_menu_on_left_click(true)
        .tooltip("OmniBox · 极速桌面工作台 (Option + Space / ⌘⌘)");

    if let Some(img) = tray_icon_img {
        builder = builder.icon(img);
    }

    builder
        .on_menu_event(move |app_handle, event| {
            let id = event.id().as_ref();
            match id {
                "tray_toggle_main" => {
                    toggle_launcher_window(app_handle);
                }
                "tray_open_super_panel" => {
                    if let Some(win) = app_handle.get_webview_window("main") {
                        #[cfg(target_os = "macos")]
                        let clip = read_macos_clipboard_text();
                        #[cfg(not(target_os = "macos"))]
                        let clip = String::new();
                        let payload = serde_json::json!({ "clipboardText": clip });
                        let _ = win.unminimize();
                        #[cfg(target_os = "macos")]
                        summon_macos_overlay_on_current_space_and_screen(&win);
                        let _ = win.eval(&format!(
                            "window.__omniboxOpenSuperPanel && window.__omniboxOpenSuperPanel({})",
                            payload
                        ));
                    }
                }
                "tray_open_settings" => {
                    summon_and_open_plugin_in_main(app_handle, "user-center");
                }
                "tray_rescan_apps" => {
                    let handle_clone = app_handle.clone();
                    tauri::async_runtime::spawn(async move {
                        if let Some(state) = handle_clone.try_state::<AppState>() {
                            let cache_dir = state.icon_cache_dir.clone();
                            let fresh = scan_system_applications(&cache_dir);
                            let combined = build_core_items(&fresh);
                            if let Ok(mut s) = state.sys_apps.lock() {
                                *s = fresh;
                            }
                            if let Ok(mut it) = state.items.lock() {
                                *it = combined;
                            }
                        }
                        if let Some(win) = handle_clone.get_webview_window("main") {
                            let _ = win.eval(
                                "window.__omniboxOnTrayToast && window.__omniboxOnTrayToast('已重新扫描本机应用索引')",
                            );
                        }
                    });
                }
                "tray_chk_launch_at_login" => {
                    let next_val = if let Some(state) = app_handle.try_state::<AppState>() {
                        state
                            .tray_handles
                            .lock()
                            .ok()
                            .and_then(|g| g.as_ref().and_then(|h| h.chk_launch_at_login.is_checked().ok()))
                            .unwrap_or(true)
                    } else {
                        true
                    };
                    #[cfg(target_os = "macos")]
                    {
                        let _ = apply_macos_launch_at_login(next_val);
                    }
                    update_saved_user_setting_bool(app_handle, "launchAtLogin", next_val);
                }
                "tray_chk_hide_on_blur" => {
                    let next_val = if let Some(state) = app_handle.try_state::<AppState>() {
                        state
                            .tray_handles
                            .lock()
                            .ok()
                            .and_then(|g| g.as_ref().and_then(|h| h.chk_hide_on_blur.is_checked().ok()))
                            .unwrap_or(true)
                    } else {
                        true
                    };
                    #[cfg(target_os = "macos")]
                    HIDE_ON_BLUR_ENABLED.store(next_val, Ordering::Relaxed);
                    update_saved_user_setting_bool(app_handle, "hideOnBlur", next_val);
                }
                "tray_chk_super_panel" => {
                    let next_val = if let Some(state) = app_handle.try_state::<AppState>() {
                        state
                            .tray_handles
                            .lock()
                            .ok()
                            .and_then(|g| g.as_ref().and_then(|h| h.chk_super_panel.is_checked().ok()))
                            .unwrap_or(true)
                    } else {
                        true
                    };
                    #[cfg(target_os = "macos")]
                    SUPER_PANEL_ENABLED.store(next_val, Ordering::Relaxed);
                    update_saved_user_setting_bool(app_handle, "enableSuperPanel", next_val);
                }
                "tray_quit" => {
                    ALLOW_APP_EXIT.store(true, Ordering::Relaxed);
                    app_handle.exit(0);
                }
                other if other.starts_with("open_plugin:") => {
                    let plugin_id = other.trim_start_matches("open_plugin:");
                    summon_and_open_plugin_in_main(app_handle, plugin_id);
                }
                other if other.starts_with("detach_plugin:") => {
                    let plugin_id = other.trim_start_matches("detach_plugin:").to_string();
                    let title = match plugin_id.as_str() {
                        "ai-studio" => "AI 助手与指令工作台 (AI Studio)",
                        "calc-pad" => "计算稿纸 (Calc Pad)",
                        "ip-studio" => "IP 与网络工作台 (IP Studio)",
                        "unit-converter" => "换算与格式工作台 (Unit & Format)",
                        "json-studio" => "JSON 助手 (JSON Studio)",
                        "diff-viewer" => "文本对比 (Diff Viewer)",
                        "password-vault" => "密码本 (Password Vault)",
                        _ => "OmniBox 独立工具窗口",
                    }
                    .to_string();
                    let handle_clone = app_handle.clone();
                    tauri::async_runtime::spawn(async move {
                        let _ = spawn_detached_plugin_window(plugin_id, None, title, handle_clone)
                            .await;
                    });
                }
                other if other.starts_with("sys_cmd:") => {
                    let cmd_id = other.trim_start_matches("sys_cmd:").to_string();
                    if let Ok(res_text) = execute_system_command(cmd_id.clone()) {
                        if cmd_id == "sys:copy-local-ip" {
                            #[cfg(target_os = "macos")]
                            write_macos_clipboard_text(&res_text);
                            if let Some(win) = app_handle.get_webview_window("main") {
                                let _ = win.eval(&format!(
                                    "window.__omniboxOnTrayToast && window.__omniboxOnTrayToast('已复制局域网出口 IP: {}')",
                                    res_text
                                ));
                            }
                        }
                    }
                }
                _ => {}
            }
        })
        .build(app)?;

    Ok(())
}

#[cfg(target_os = "macos")]
fn enforce_single_instance(is_silent_launch: bool) {
    let my_pid = std::process::id().to_string();
    if let Ok(out) = Command::new("pgrep").args(["-x", "omni-box"]).output() {
        let text = String::from_utf8_lossy(&out.stdout);
        let other_pids: Vec<&str> = text
            .lines()
            .map(|l| l.trim())
            .filter(|p| !p.is_empty() && *p != my_pid)
            .collect();

        if !other_pids.is_empty() {
            if is_silent_launch {
                // Another instance is already running; exit silent autostart cleanly
                std::process::exit(0);
            } else {
                // User explicitly launched OmniBox; terminate stale/previous instances so only 1 instance runs
                for pid in other_pids {
                    let _ = Command::new("kill").args(["-9", pid]).output();
                }
            }
        }
    }
}

fn main() {
    let is_silent_launch = std::env::args().any(|a| {
        a == "--silent" || a == "--minimized" || a == "--autostart" || a == "-s"
    });

    #[cfg(target_os = "macos")]
    enforce_single_instance(is_silent_launch);

    let app_data_dir = dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("com.omnibox.launcher");

    let db_path = app_data_dir.join("omni_data.db");
    let icon_cache_dir = app_data_dir.join("app_icons_cache");

    let db = AppDatabase::init(db_path).expect("Failed to initialize OmniBox SQLite database");
    let sys_apps = scan_system_applications(&icon_cache_dir);
    let initial_items = build_core_items(&sys_apps);

    // Load saved settings from SQLite if present
    let saved_settings_json = db
        .kv_get("system", "user-settings")
        .ok()
        .flatten()
        .and_then(|json_str| serde_json::from_str::<serde_json::Value>(&json_str).ok());

    let initial_hotkey = saved_settings_json
        .as_ref()
        .and_then(|v| v["globalHotkey"].as_str().map(String::from))
        .unwrap_or_else(|| "Option + Space".to_string());

    let initial_launch_at_login = saved_settings_json
        .as_ref()
        .and_then(|v| v["launchAtLogin"].as_bool())
        .unwrap_or(true);

    let initial_hide_on_blur = saved_settings_json
        .as_ref()
        .and_then(|v| v["hideOnBlur"].as_bool())
        .unwrap_or(true);

    let initial_super_panel = saved_settings_json
        .as_ref()
        .and_then(|v| v["enableSuperPanel"].as_bool())
        .unwrap_or(true);

    #[cfg(target_os = "macos")]
    {
        HIDE_ON_BLUR_ENABLED.store(initial_hide_on_blur, Ordering::Relaxed);
        SUPER_PANEL_ENABLED.store(initial_super_panel, Ordering::Relaxed);
        let _ = apply_macos_launch_at_login(initial_launch_at_login);
    }

    let hotkey_arc = Arc::new(Mutex::new(initial_hotkey.clone()));
    let hotkey_for_setup = hotkey_arc.clone();

    let app = tauri::Builder::default()
        .manage(AppState {
            db: Mutex::new(db),
            items: Mutex::new(initial_items),
            sys_apps: Mutex::new(sys_apps),
            icon_cache_dir,
            global_hotkey: hotkey_arc,
            tray_handles: Mutex::new(None),
        })
        .setup(move |app| {
            let _ = setup_system_tray(
                app,
                &initial_hotkey,
                initial_launch_at_login,
                initial_hide_on_blur,
                initial_super_panel,
            );

            #[cfg(target_os = "macos")]
            {
                let _ = app.set_activation_policy(tauri::ActivationPolicy::Accessory);
                ensure_macos_accessory_policy();
                apply_macos_application_icon();
                if let Some(win) = app.get_webview_window("main") {
                    if let Ok(ns_win) = win.ns_window() {
                        configure_nswindow_overlay_props(ns_win);
                        place_nswindow_on_active_mouse_screen(ns_win, true);
                        if is_silent_launch {
                            hide_nswindow_native(ns_win);
                            let _ = win.hide();
                        }
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
            read_clipboard_image_base64,
            get_local_network_overview,
            fetch_url_json_via_curl,
            get_system_integration_status,
            ai_chat_completion,
            ai_fetch_models
        ])
        .build(tauri::generate_context!())
        .expect("error while building OmniBox Tauri application");

    app.run(|app_handle, event| {
        #[cfg(target_os = "macos")]
        match event {
            tauri::RunEvent::Ready => {
                let _ = app_handle.set_activation_policy(tauri::ActivationPolicy::Accessory);
                ensure_macos_accessory_policy();
                apply_macos_application_icon();
                if let Some(win) = app_handle.get_webview_window("main") {
                    if let Ok(ns_win) = win.ns_window() {
                        configure_nswindow_overlay_props(ns_win);
                    }
                }
            }
            tauri::RunEvent::Reopen { .. } => {
                let brought_detached = bring_all_detached_windows_to_front(app_handle, true);
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
                if !ALLOW_APP_EXIT.load(Ordering::Relaxed) {
                    api.prevent_exit();
                }
            }
            _ => {}
        }
    });
}
