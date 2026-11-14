//! Windows-specific platform safeguards:
//! - Fall back to Slint's software renderer when the system lacks OpenGL 2.0+
//!   shader support (e.g. Microsoft GDI Generic 1.1 in VMs or RDP sessions).
//! - Clear Slint's tracked Alt modifier if Win32 modal menu/resize loops
//!   (such as `Alt+Space -> Size -> Enter`) consume the `VK_MENU` release event.

use std::ffi::{c_int, c_void};
use std::ptr::null_mut;

use i_slint_backend_winit::winit::event::WindowEvent as WinitWindowEvent;
use i_slint_backend_winit::{EventResult, WinitWindowAccessor};
use slint::ComponentHandle;

type Hwnd = *mut c_void;
type Hdc = *mut c_void;
type Hglrc = *mut c_void;

const WS_POPUP: u32 = 0x8000_0000;
const PFD_DOUBLEBUFFER: u32 = 0x0000_0001;
const PFD_DRAW_TO_WINDOW: u32 = 0x0000_0004;
const PFD_SUPPORT_OPENGL: u32 = 0x0000_0020;
const PFD_TYPE_RGBA: u8 = 0;
const PFD_MAIN_PLANE: u8 = 0;
const VK_MENU: c_int = 0x12;
const WM_KEYFIRST: u32 = 0x0100;
const WM_KEYLAST: u32 = 0x0109;
const PM_NOREMOVE: u32 = 0x0000;

#[repr(C)]
struct Point {
    x: i32,
    y: i32,
}

#[repr(C)]
struct Msg {
    hwnd: Hwnd,
    message: u32,
    w_param: usize,
    l_param: isize,
    time: u32,
    pt: Point,
}

#[repr(C)]
struct PixelFormatDescriptor {
    n_size: u16,
    n_version: u16,
    dw_flags: u32,
    i_pixel_type: u8,
    c_color_bits: u8,
    c_red_bits: u8,
    c_red_shift: u8,
    c_green_bits: u8,
    c_green_shift: u8,
    c_blue_bits: u8,
    c_blue_shift: u8,
    c_alpha_bits: u8,
    c_alpha_shift: u8,
    c_accum_bits: u8,
    c_accum_red_bits: u8,
    c_accum_green_bits: u8,
    c_accum_blue_bits: u8,
    c_accum_alpha_bits: u8,
    c_depth_bits: u8,
    c_stencil_bits: u8,
    c_aux_buffers: u8,
    i_layer_type: u8,
    b_reserved: u8,
    dw_layer_mask: u32,
    dw_visible_mask: u32,
    dw_damage_mask: u32,
}

#[link(name = "user32")]
unsafe extern "system" {
    fn CreateWindowExW(
        ex_style: u32,
        class_name: *const u16,
        window_name: *const u16,
        style: u32,
        x: c_int,
        y: c_int,
        width: c_int,
        height: c_int,
        parent: Hwnd,
        menu: *mut c_void,
        instance: *mut c_void,
        param: *mut c_void,
    ) -> Hwnd;
    fn DestroyWindow(hwnd: Hwnd) -> c_int;
    fn GetDC(hwnd: Hwnd) -> Hdc;
    fn ReleaseDC(hwnd: Hwnd, hdc: Hdc) -> c_int;
    fn GetAsyncKeyState(vkey: c_int) -> i16;
    fn PeekMessageW(
        msg: *mut Msg,
        hwnd: Hwnd,
        msg_filter_min: u32,
        msg_filter_max: u32,
        remove_msg: u32,
    ) -> c_int;
}

#[link(name = "gdi32")]
unsafe extern "system" {
    fn ChoosePixelFormat(hdc: Hdc, ppfd: *const PixelFormatDescriptor) -> c_int;
    fn SetPixelFormat(hdc: Hdc, format: c_int, ppfd: *const PixelFormatDescriptor) -> c_int;
}

#[link(name = "opengl32")]
unsafe extern "system" {
    fn wglCreateContext(hdc: Hdc) -> Hglrc;
    fn wglDeleteContext(hglrc: Hglrc) -> c_int;
    fn wglMakeCurrent(hdc: Hdc, hglrc: Hglrc) -> c_int;
    fn wglGetProcAddress(name: *const u8) -> *const c_void;
}

fn windows_supports_opengl_shaders() -> bool {
    const STATIC_CLASS: [u16; 7] = [
        b'S' as u16,
        b'T' as u16,
        b'A' as u16,
        b'T' as u16,
        b'I' as u16,
        b'C' as u16,
        0,
    ];
    const EMPTY_TITLE: [u16; 1] = [0];

    let pfd = PixelFormatDescriptor {
        n_size: size_of::<PixelFormatDescriptor>() as u16,
        n_version: 1,
        dw_flags: PFD_DRAW_TO_WINDOW | PFD_SUPPORT_OPENGL | PFD_DOUBLEBUFFER,
        i_pixel_type: PFD_TYPE_RGBA,
        c_color_bits: 32,
        c_red_bits: 0,
        c_red_shift: 0,
        c_green_bits: 0,
        c_green_shift: 0,
        c_blue_bits: 0,
        c_blue_shift: 0,
        c_alpha_bits: 0,
        c_alpha_shift: 0,
        c_accum_bits: 0,
        c_accum_red_bits: 0,
        c_accum_green_bits: 0,
        c_accum_blue_bits: 0,
        c_accum_alpha_bits: 0,
        c_depth_bits: 24,
        c_stencil_bits: 8,
        c_aux_buffers: 0,
        i_layer_type: PFD_MAIN_PLANE,
        b_reserved: 0,
        dw_layer_mask: 0,
        dw_visible_mask: 0,
        dw_damage_mask: 0,
    };

    // Probe OpenGL 2.0+ shader support on an invisible dummy window so
    // systems with only GDI Generic OpenGL 1.1 select software rendering
    // instead of failing after the real window opens.
    unsafe {
        let hwnd = CreateWindowExW(
            0,
            STATIC_CLASS.as_ptr(),
            EMPTY_TITLE.as_ptr(),
            WS_POPUP,
            0,
            0,
            1,
            1,
            null_mut(),
            null_mut(),
            null_mut(),
            null_mut(),
        );
        if hwnd.is_null() {
            return false;
        }
        let hdc = GetDC(hwnd);
        let mut supported = false;
        if !hdc.is_null() {
            let format = ChoosePixelFormat(hdc, &pfd);
            if format != 0 && SetPixelFormat(hdc, format, &pfd) != 0 {
                let hglrc = wglCreateContext(hdc);
                if !hglrc.is_null() {
                    if wglMakeCurrent(hdc, hglrc) != 0 {
                        let proc_addr =
                            wglGetProcAddress(c"glCreateShader".as_ptr().cast()) as usize;
                        // wglGetProcAddress returns 0, 1, 2, 3, or -1 on failure.
                        supported = proc_addr > 3 && proc_addr != usize::MAX;
                        wglMakeCurrent(null_mut(), null_mut());
                    }
                    wglDeleteContext(hglrc);
                }
            }
            ReleaseDC(hwnd, hdc);
        }
        DestroyWindow(hwnd);
        supported
    }
}

/// Select the software renderer automatically on Windows hosts without OpenGL 2.0+ shaders.
pub(crate) fn init_backend_if_needed() -> Result<(), String> {
    if std::env::var_os("SLINT_BACKEND").is_some() || windows_supports_opengl_shaders() {
        return Ok(());
    }
    let backend = i_slint_backend_winit::Backend::builder()
        .with_renderer_name("software")
        .build()
        .map_err(|error| error.to_string())?;
    slint::platform::set_platform(Box::new(backend)).map_err(|error| error.to_string())
}

fn has_pending_keyboard_input() -> bool {
    let mut msg = std::mem::MaybeUninit::<Msg>::uninit();
    unsafe {
        PeekMessageW(
            msg.as_mut_ptr(),
            null_mut(),
            WM_KEYFIRST,
            WM_KEYLAST,
            PM_NOREMOVE,
        ) != 0
    }
}

/// Clear any stale Alt modifier state left behind when Windows modal system
/// menus or move/size loops consume `VK_MENU` release events.
pub(crate) fn install_alt_modifier_guard(app: &crate::SheetsApp) {
    app.window().on_winit_window_event(|window, event| {
        let should_sync = match event {
            WinitWindowEvent::ModifiersChanged(mods) => !mods.state().alt_key(),
            WinitWindowEvent::Resized(_)
            | WinitWindowEvent::Moved(_)
            | WinitWindowEvent::Focused(true)
            | WinitWindowEvent::MouseInput { .. } => !has_pending_keyboard_input(),
            _ => false,
        };
        if should_sync && unsafe { (GetAsyncKeyState(VK_MENU) as u16 & 0x8000) == 0 } {
            let _ = window.try_dispatch_event(slint::platform::WindowEvent::KeyReleased {
                text: slint::platform::Key::Alt.into(),
            });
            let _ = window.try_dispatch_event(slint::platform::WindowEvent::KeyReleased {
                text: slint::platform::Key::AltGr.into(),
            });
        }
        EventResult::Propagate
    });
}
