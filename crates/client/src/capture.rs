//! Mouse capture for first-person look. Rune has no pointer lock, so while the game holds the
//! mouse the cursor is hidden and moved back to the window's centre after each motion, and the
//! motion becomes camera turn. Supported on X11, Windows and macOS; on Wayland (which forbids
//! moving the pointer) the game falls back to drag-to-look.

/// Whether this platform lets us move the pointer.
pub fn supported() -> bool {
    imp::supported()
}

/// Moves the OS pointer by (dx, dy) window points; `scale` is pixels per point.
pub fn warp_by(dx: f64, dy: f64, scale: f64) -> bool {
    imp::warp_by(dx, dy, scale)
}

#[cfg(target_os = "linux")]
mod imp {
    use std::ffi::c_void;
    use std::sync::OnceLock;

    struct X11 {
        _lib: libloading::Library,
        display: *mut c_void,
        warp: unsafe extern "C" fn(*mut c_void, u64, u64, i32, i32, u32, u32, i32, i32) -> i32,
        flush: unsafe extern "C" fn(*mut c_void) -> i32,
    }
    // The display connection is only used from the UI thread, one call at a time.
    unsafe impl Send for X11 {}
    unsafe impl Sync for X11 {}

    fn x11() -> Option<&'static X11> {
        static X: OnceLock<Option<X11>> = OnceLock::new();
        X.get_or_init(|| unsafe {
            let lib = libloading::Library::new("libX11.so.6").ok()?;
            let open: libloading::Symbol<unsafe extern "C" fn(*const i8) -> *mut c_void> = lib.get(b"XOpenDisplay\0").ok()?;
            let warp = *lib.get::<unsafe extern "C" fn(*mut c_void, u64, u64, i32, i32, u32, u32, i32, i32) -> i32>(b"XWarpPointer\0").ok()?;
            let flush = *lib.get::<unsafe extern "C" fn(*mut c_void) -> i32>(b"XFlush\0").ok()?;
            let display = open(std::ptr::null());
            if display.is_null() {
                return None;
            }
            Some(X11 { _lib: lib, display, warp, flush })
        })
        .as_ref()
    }

    pub fn supported() -> bool {
        let wayland = std::env::var_os("WAYLAND_DISPLAY").is_some() && std::env::var("WINIT_UNIX_BACKEND").map_or(true, |b| b != "x11");
        !wayland && std::env::var_os("DISPLAY").is_some() && x11().is_some()
    }

    pub fn warp_by(dx: f64, dy: f64, scale: f64) -> bool {
        let Some(x) = x11() else { return false };
        // With no destination window, X moves the pointer relative to where it is.
        unsafe {
            (x.warp)(x.display, 0, 0, 0, 0, 0, 0, (dx * scale).round() as i32, (dy * scale).round() as i32);
            (x.flush)(x.display);
        }
        true
    }
}

#[cfg(target_os = "windows")]
mod imp {
    #[repr(C)]
    struct Point {
        x: i32,
        y: i32,
    }
    #[link(name = "user32")]
    unsafe extern "system" {
        fn GetCursorPos(p: *mut Point) -> i32;
        fn SetCursorPos(x: i32, y: i32) -> i32;
    }

    pub fn supported() -> bool {
        true
    }

    pub fn warp_by(dx: f64, dy: f64, scale: f64) -> bool {
        let mut p = Point { x: 0, y: 0 };
        unsafe {
            if GetCursorPos(&mut p) == 0 {
                return false;
            }
            SetCursorPos(p.x + (dx * scale).round() as i32, p.y + (dy * scale).round() as i32) != 0
        }
    }
}

#[cfg(target_os = "macos")]
mod imp {
    use std::ffi::c_void;

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct CGPoint {
        x: f64,
        y: f64,
    }
    #[link(name = "CoreGraphics", kind = "framework")]
    unsafe extern "C" {
        fn CGEventCreate(source: *const c_void) -> *mut c_void;
        fn CGEventGetLocation(event: *mut c_void) -> CGPoint;
        fn CGWarpMouseCursorPosition(p: CGPoint) -> i32;
        fn CGAssociateMouseAndMouseCursorPosition(connected: u32) -> i32;
    }
    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" {
        fn CFRelease(cf: *const c_void);
    }

    pub fn supported() -> bool {
        true
    }

    pub fn warp_by(dx: f64, dy: f64, _scale: f64) -> bool {
        // Global display coordinates on macOS are in points, like Rune's.
        unsafe {
            let e = CGEventCreate(std::ptr::null());
            if e.is_null() {
                return false;
            }
            let p = CGEventGetLocation(e);
            CFRelease(e);
            CGWarpMouseCursorPosition(CGPoint { x: p.x + dx, y: p.y + dy });
            // Without this the system ignores mouse motion for a moment after each warp.
            CGAssociateMouseAndMouseCursorPosition(1);
        }
        true
    }
}

#[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
mod imp {
    pub fn supported() -> bool {
        false
    }
    pub fn warp_by(_: f64, _: f64, _: f64) -> bool {
        false
    }
}
