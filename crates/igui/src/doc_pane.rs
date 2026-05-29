//! doc_pane — an MDI child that renders a Markdown document.
//!
//! A read-only, scrollable pane type (peer to the editor / REPL / log),
//! drawing through the shared `docpane` render core — the same bytes
//! the standalone `doc-crate.exe` test harness renders, so the in-window
//! manual and the snapshots never drift.
//!
//! Structure mirrors `text_view`: its own window class + wndproc, state
//! in `GWLP_USERDATA`, created via `WM_MDICREATE` on the GUI thread.
//! Unlike the terminal grid, it owns a Markdown string, lays it out with
//! `docpane::layout`, and paints with `docpane::render::draw_document`.
//!
//! Factory note: the pane creates its `ID2D1HwndRenderTarget` from
//! *docpane's* D2D factory (`render::factory()`), because the Mermaid
//! path geometry and the target it's drawn on must share one factory.

#![cfg(windows)]

use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{HANDLE, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Direct2D::Common::{
    D2D1_ALPHA_MODE_IGNORE, D2D1_PIXEL_FORMAT, D2D_SIZE_U,
};
use windows::Win32::Graphics::Direct2D::{
    ID2D1HwndRenderTarget, D2D1_FEATURE_LEVEL_DEFAULT, D2D1_HWND_RENDER_TARGET_PROPERTIES,
    D2D1_PRESENT_OPTIONS_NONE, D2D1_RENDER_TARGET_PROPERTIES, D2D1_RENDER_TARGET_TYPE_DEFAULT,
    D2D1_RENDER_TARGET_USAGE_NONE,
};
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM;
use windows::Win32::Graphics::Gdi::{BeginPaint, EndPaint, InvalidateRect, PAINTSTRUCT};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::WindowsAndMessaging::{
    DefMDIChildProcW, GetClientRect, GetWindowLongPtrW, LoadCursorW, RegisterClassExW,
    SendMessageW, SetWindowLongPtrW, CREATESTRUCTW, CW_USEDEFAULT, GWLP_USERDATA, IDC_ARROW,
    MDICREATESTRUCTW, WM_MDICREATE, WM_MOUSEWHEEL, WM_NCCREATE, WM_NCDESTROY, WM_PAINT, WM_SIZE,
    WNDCLASSEXW, WNDCLASS_STYLES, WS_OVERLAPPEDWINDOW, WS_VISIBLE,
};

use docpane::{layout as dlayout, parser, render, theme};

use super::{registry, window};

const DOC_CLASS: PCWSTR = w!("Factor4thDocPane");
/// DIPs scrolled per wheel notch.
const WHEEL_STEP: f32 = 48.0;

// ─── Per-window state (lives on the GUI thread, in GWLP_USERDATA) ────
struct DocWindowState {
    hwnd: HWND,
    child_id: i64,
    target: Option<ID2D1HwndRenderTarget>,
    /// The Markdown source for this pane.
    md: String,
    /// Cached layout + the content width (DIPs) it was laid out at, so
    /// we only re-layout when the pane is resized.
    layout: Option<dlayout::Layout>,
    laid_out_w: f32,
    scroll_y: f32,
    max_scroll: f32,
    client_w: u32,
    client_h: u32,
    dpi: u32,
}

impl DocWindowState {
    fn new(hwnd: HWND, child_id: i64, md: String) -> Self {
        let dpi = unsafe { GetDpiForWindow(hwnd) };
        let dpi = if dpi == 0 { 96 } else { dpi };
        Self {
            hwnd,
            child_id,
            target: None,
            md,
            layout: None,
            laid_out_w: -1.0,
            scroll_y: 0.0,
            max_scroll: 0.0,
            client_w: 0,
            client_h: 0,
            dpi,
        }
    }

    /// 96 / dpi — multiply pixel sizes by this to get DIPs.  The render
    /// target is created at the monitor DPI, so all layout/draw maths is
    /// in DIPs and D2D scales to pixels.
    fn dip_scale(&self) -> f32 {
        if self.dpi == 0 {
            1.0
        } else {
            96.0 / (self.dpi as f32)
        }
    }

    fn invalidate(&self) {
        let _ = unsafe { InvalidateRect(Some(self.hwnd), None, false) };
    }

    fn ensure_target(&mut self, w: u32, h: u32) {
        if let Some(t) = self.target.as_ref() {
            let cur = unsafe { t.GetPixelSize() };
            if cur.width != w || cur.height != h {
                let _ = unsafe { t.Resize(&D2D_SIZE_U { width: w, height: h }) };
            }
            return;
        }
        let dpi = self.dpi as f32;
        let target = unsafe {
            render::factory().CreateHwndRenderTarget(
                &D2D1_RENDER_TARGET_PROPERTIES {
                    r#type: D2D1_RENDER_TARGET_TYPE_DEFAULT,
                    pixelFormat: D2D1_PIXEL_FORMAT {
                        format: DXGI_FORMAT_B8G8R8A8_UNORM,
                        alphaMode: D2D1_ALPHA_MODE_IGNORE,
                    },
                    dpiX: dpi,
                    dpiY: dpi,
                    usage: D2D1_RENDER_TARGET_USAGE_NONE,
                    minLevel: D2D1_FEATURE_LEVEL_DEFAULT,
                },
                &D2D1_HWND_RENDER_TARGET_PROPERTIES {
                    hwnd: self.hwnd,
                    pixelSize: D2D_SIZE_U { width: w, height: h },
                    presentOptions: D2D1_PRESENT_OPTIONS_NONE,
                },
            )
        };
        match target {
            Ok(t) => self.target = Some(t),
            Err(e) => eprintln!("[doc-pane] CreateHwndRenderTarget failed: {e}"),
        }
    }

    /// (Re)lay out the document for the current content width and update
    /// the scroll bound.  No-op if the width is unchanged.
    fn relayout(&mut self, content_w: f32, viewport_h: f32) {
        if self.layout.is_some() && (content_w - self.laid_out_w).abs() < 0.5 {
            self.update_max_scroll(viewport_h);
            return;
        }
        let blocks = parser::parse(&self.md);
        let ly = dlayout::layout(&blocks, theme::H_PAD, content_w, 0.0, render::measure_text);
        self.laid_out_w = content_w;
        self.layout = Some(ly);
        self.update_max_scroll(viewport_h);
    }

    fn update_max_scroll(&mut self, viewport_h: f32) {
        let total = self.layout.as_ref().map(|l| l.total_h).unwrap_or(0.0);
        self.max_scroll = (total - viewport_h).max(0.0);
        if self.scroll_y > self.max_scroll {
            self.scroll_y = self.max_scroll;
        }
    }

    fn scroll_by(&mut self, dips: f32) {
        let prev = self.scroll_y;
        self.scroll_y = (self.scroll_y + dips).clamp(0.0, self.max_scroll);
        if (self.scroll_y - prev).abs() > 0.01 {
            self.invalidate();
        }
    }

    fn paint(&mut self) {
        let mut rect = RECT::default();
        if unsafe { GetClientRect(self.hwnd, &mut rect) }.is_err() {
            return;
        }
        let w = (rect.right - rect.left) as u32;
        let h = (rect.bottom - rect.top) as u32;
        if w == 0 || h == 0 {
            return;
        }
        self.client_w = w;
        self.client_h = h;
        self.ensure_target(w, h);

        let scale = self.dip_scale();
        let viewport_w = w as f32 * scale;
        let viewport_h = h as f32 * scale;
        let content_w = (viewport_w - 2.0 * theme::H_PAD).max(1.0);
        self.relayout(content_w, viewport_h);

        let target = match self.target.clone() {
            Some(t) => t,
            None => return,
        };
        let layout = match self.layout.as_ref() {
            Some(l) => l,
            None => return,
        };

        unsafe {
            target.BeginDraw();
            let bg = theme::hex(theme::BG);
            target.Clear(Some(std::ptr::addr_of!(bg)));
            // &ID2D1HwndRenderTarget deref-coerces to &ID2D1RenderTarget.
            if let Err(e) = render::draw_document(&target, layout, self.scroll_y, viewport_h) {
                eprintln!("[doc-pane] draw_document: {e}");
            }
            let _ = target.EndDraw(None, None);
        }
    }
}

// ─── Class registration ─────────────────────────────────────────────
pub fn register_class() -> Result<(), super::IGuiError> {
    // The render core's factories must exist before the first paint.
    if let Err(e) = render::init() {
        eprintln!("[doc-pane] render::init failed: {e}");
    }
    let h_instance = unsafe { GetModuleHandleW(None) }
        .map_err(|e| super::IGuiError::Win32(format!("GetModuleHandleW (doc): {e}")))?
        .into();
    let cursor = unsafe { LoadCursorW(None, IDC_ARROW) }
        .map_err(|e| super::IGuiError::Win32(format!("LoadCursorW (doc): {e}")))?;
    let cls = WNDCLASSEXW {
        cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
        style: WNDCLASS_STYLES(0),
        lpfnWndProc: Some(doc_wnd_proc),
        cbClsExtra: 0,
        cbWndExtra: 0,
        hInstance: h_instance,
        hIcon: Default::default(),
        hCursor: cursor,
        hbrBackground: windows::Win32::Graphics::Gdi::HBRUSH(std::ptr::null_mut()),
        lpszMenuName: PCWSTR::null(),
        lpszClassName: DOC_CLASS,
        hIconSm: Default::default(),
    };
    let _ = unsafe { RegisterClassExW(&cls) };
    Ok(())
}

struct DocBootstrap {
    child_id: i64,
    md: String,
}

/// Language/host-thread entry: open a doc-pane showing `md`.  Marshals
/// to the GUI thread (where state alloc + WM_MDICREATE are safe).
pub fn open(title: &str, md: &str) -> Option<i64> {
    window::open_doc_child(title, md)
}

/// GUI-thread half of [`open`]: allocate the child id, box the bootstrap
/// (carrying the Markdown), issue WM_MDICREATE.
pub(super) fn create_on_gui_thread(mdi: HWND, title_utf16: &[u16], md: &str) -> Option<i64> {
    let child_id = registry::allocate_child_id();
    let bootstrap = Box::into_raw(Box::new(DocBootstrap {
        child_id,
        md: md.to_owned(),
    }));

    let h_module = match unsafe { GetModuleHandleW(None) } {
        Ok(h) => HANDLE(h.0),
        Err(e) => {
            eprintln!("[doc-pane] GetModuleHandleW: {e}");
            let _ = unsafe { Box::from_raw(bootstrap) };
            return None;
        }
    };

    let create = MDICREATESTRUCTW {
        szClass: DOC_CLASS,
        szTitle: PCWSTR::from_raw(title_utf16.as_ptr()),
        hOwner: h_module,
        x: CW_USEDEFAULT,
        y: CW_USEDEFAULT,
        cx: CW_USEDEFAULT,
        cy: CW_USEDEFAULT,
        style: WS_VISIBLE | WS_OVERLAPPEDWINDOW,
        lParam: LPARAM(bootstrap as isize),
    };

    let result = unsafe {
        SendMessageW(
            mdi,
            WM_MDICREATE,
            Some(WPARAM(0)),
            Some(LPARAM(&create as *const _ as isize)),
        )
    };
    if result.0 == 0 {
        eprintln!("[doc-pane] WM_MDICREATE returned 0");
        let _ = unsafe { Box::from_raw(bootstrap) };
        return None;
    }
    Some(child_id)
}

// ─── Window proc ────────────────────────────────────────────────────
unsafe extern "system" fn doc_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if msg == WM_NCCREATE {
        let create = lparam.0 as *const CREATESTRUCTW;
        let mdi_create = unsafe { (*create).lpCreateParams as *const MDICREATESTRUCTW };
        if mdi_create.is_null() {
            return unsafe { DefMDIChildProcW(hwnd, msg, wparam, lparam) };
        }
        let bootstrap_ptr = unsafe { (*mdi_create).lParam.0 as *mut DocBootstrap };
        if bootstrap_ptr.is_null() {
            return unsafe { DefMDIChildProcW(hwnd, msg, wparam, lparam) };
        }
        let bootstrap = unsafe { Box::from_raw(bootstrap_ptr) };
        let child_id = bootstrap.child_id;
        let win_state = Box::new(DocWindowState::new(hwnd, child_id, bootstrap.md));
        let raw = Box::into_raw(win_state) as isize;
        unsafe { SetWindowLongPtrW(hwnd, GWLP_USERDATA, raw) };
        registry::register(child_id, hwnd, hwnd);
        return unsafe { DefMDIChildProcW(hwnd, msg, wparam, lparam) };
    }

    let state_ptr = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *mut DocWindowState;
    if state_ptr.is_null() {
        return unsafe { DefMDIChildProcW(hwnd, msg, wparam, lparam) };
    }
    let state = unsafe { &mut *state_ptr };

    match msg {
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            let _ = unsafe { BeginPaint(hwnd, &mut ps) };
            state.paint();
            let _ = unsafe { EndPaint(hwnd, &ps) };
            LRESULT(0)
        }
        WM_SIZE => {
            let w = (lparam.0 & 0xFFFF) as u32;
            let h = ((lparam.0 >> 16) & 0xFFFF) as u32;
            state.client_w = w;
            state.client_h = h;
            if let Some(t) = state.target.as_ref() {
                let _ = unsafe { t.Resize(&D2D_SIZE_U { width: w, height: h }) };
            }
            state.invalidate();
            unsafe { DefMDIChildProcW(hwnd, msg, wparam, lparam) }
        }
        WM_MOUSEWHEEL => {
            // High word of wparam is the signed wheel delta (±120/notch).
            let delta = ((wparam.0 >> 16) & 0xFFFF) as i16 as f32;
            // Wheel up (positive) scrolls toward the top → smaller scroll_y.
            state.scroll_by(-(delta / 120.0) * WHEEL_STEP);
            LRESULT(0)
        }
        WM_NCDESTROY => {
            registry::unregister(state.child_id);
            let _ = unsafe { Box::from_raw(state_ptr) };
            unsafe { SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0) };
            unsafe { DefMDIChildProcW(hwnd, msg, wparam, lparam) }
        }
        _ => unsafe { DefMDIChildProcW(hwnd, msg, wparam, lparam) },
    }
}
