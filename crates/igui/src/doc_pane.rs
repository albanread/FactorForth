//! doc_pane — an MDI child that browses a folder of Markdown documents.
//!
//! A read-only pane type (peer to the editor / REPL / log) that renders
//! through the shared `docpane` render core.  It behaves like a small
//! documentation browser: a sidebar table-of-contents on the left, the
//! current document on the right, click a sidebar entry or an in-page
//! link to navigate, scroll with the wheel.
//!
//! The render core renders one document from text; the *browser* chrome
//! (folder scan, sidebar, navigation) lives here in the host.  The
//! sidebar is itself a rendered Markdown TOC, so both panes go through
//! the same `draw_document`, and navigation is unified: every clickable
//! region — sidebar entry or content link — is a `HitRegion` with an
//! href, resolved relative to the current document.
//!
//! Factory note: the pane creates its render target from *docpane's* D2D
//! factory (`render::factory()`), so Mermaid geometry and the target
//! share one factory.

#![cfg(windows)]

use std::path::{Path, PathBuf};

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
    MDICREATESTRUCTW, WM_LBUTTONDOWN, WM_MDICREATE, WM_MOUSEWHEEL, WM_NCCREATE, WM_NCDESTROY,
    WM_PAINT, WM_SIZE, WNDCLASSEXW, WNDCLASS_STYLES, WS_OVERLAPPEDWINDOW, WS_VISIBLE,
};

use docpane::layout::Layout;
use docpane::{layout as dlayout, parser, render, theme};

use super::{registry, window};

const DOC_CLASS: PCWSTR = w!("Factor4thDocPane");
/// Sidebar width in DIPs.
const SIDEBAR_W: f32 = 220.0;
/// DIPs scrolled per wheel notch.
const WHEEL_STEP: f32 = 48.0;

struct DocFile {
    name: String,
    path: PathBuf,
}

/// Flat scan of a folder for `.md` files: index/readme first, then
/// alphabetical (case-insensitive).
fn scan(dir: &Path) -> Vec<DocFile> {
    let mut files: Vec<DocFile> = Vec::new();
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            let p = e.path();
            if p.extension().and_then(|x| x.to_str()) == Some("md") {
                let name = p
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("untitled")
                    .to_string();
                files.push(DocFile { name, path: p });
            }
        }
    }
    files.sort_by(|a, b| {
        let ap = a.name.eq_ignore_ascii_case("index") || a.name.eq_ignore_ascii_case("readme");
        let bp = b.name.eq_ignore_ascii_case("index") || b.name.eq_ignore_ascii_case("readme");
        match (ap, bp) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
        }
    });
    files
}

/// Resolve an href relative to the current doc → a local file path, or
/// `None` for external / anchor-only / missing targets.
fn resolve_href(href: &str, current: &Path, docs_dir: &Path) -> Option<PathBuf> {
    if href.starts_with("http://") || href.starts_with("https://") {
        return None;
    }
    let href = href.split('#').next().unwrap_or("");
    if href.is_empty() {
        return None;
    }
    let base = current.parent().unwrap_or(docs_dir);
    let cand = base.join(href);
    if cand.exists() {
        return Some(cand);
    }
    let with = base.join(format!("{href}.md"));
    if with.exists() {
        return Some(with);
    }
    None
}

fn same_file(a: &Path, b: &Path) -> bool {
    a == b
        || matches!(
            (std::fs::canonicalize(a), std::fs::canonicalize(b)),
            (Ok(x), Ok(y)) if x == y
        )
}

// ─── Per-window state ────────────────────────────────────────────────
struct DocWindowState {
    hwnd: HWND,
    child_id: i64,
    target: Option<ID2D1HwndRenderTarget>,
    docs_dir: PathBuf,
    files: Vec<DocFile>,
    current: usize,
    /// Cached content layout + the width it was laid out at.
    content: Option<Layout>,
    laid_out_w: f32,
    /// Cached sidebar TOC layout (rebuilt only if the file set changes).
    sidebar: Option<Layout>,
    scroll_y: f32,
    max_scroll: f32,
    client_w: u32,
    client_h: u32,
    dpi: u32,
}

impl DocWindowState {
    fn new(hwnd: HWND, child_id: i64, path: &str) -> Self {
        let p = PathBuf::from(path);
        let (docs_dir, initial) = if p.is_dir() {
            (p.clone(), None)
        } else {
            (
                p.parent().map(|d| d.to_path_buf()).unwrap_or_else(|| PathBuf::from(".")),
                Some(p.clone()),
            )
        };
        let files = scan(&docs_dir);
        let current = initial
            .and_then(|ip| files.iter().position(|f| same_file(&f.path, &ip)))
            .unwrap_or(0);
        let dpi = unsafe { GetDpiForWindow(hwnd) };
        let dpi = if dpi == 0 { 96 } else { dpi };
        Self {
            hwnd,
            child_id,
            target: None,
            docs_dir,
            files,
            current,
            content: None,
            laid_out_w: -1.0,
            sidebar: None,
            scroll_y: 0.0,
            max_scroll: 0.0,
            client_w: 0,
            client_h: 0,
            dpi,
        }
    }

    fn dip_scale(&self) -> f32 {
        if self.dpi == 0 { 1.0 } else { 96.0 / (self.dpi as f32) }
    }

    fn invalidate(&self) {
        let _ = unsafe { InvalidateRect(Some(self.hwnd), None, false) };
    }

    fn current_path(&self) -> Option<PathBuf> {
        self.files.get(self.current).map(|f| f.path.clone())
    }

    /// Build the sidebar TOC as a small Markdown doc — a heading plus a
    /// link per file (href = bare filename, resolved relative to the
    /// docs dir).  Rendered like any other document.
    fn sidebar_md(&self) -> String {
        let mut s = String::from("## Contents\n\n");
        for f in &self.files {
            let fname = f.path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            s.push_str(&format!("- [{}]({})\n", f.name, fname));
        }
        s
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

    fn relayout(&mut self, content_w: f32, viewport_h: f32) {
        if self.sidebar.is_none() {
            let md = self.sidebar_md();
            let blocks = parser::parse(&md);
            self.sidebar = Some(dlayout::layout(
                &blocks,
                theme::H_PAD,
                (SIDEBAR_W - 2.0 * theme::H_PAD).max(1.0),
                0.0,
                render::measure_text,
            ));
        }
        if self.content.is_none() || (content_w - self.laid_out_w).abs() > 0.5 {
            let md = self
                .current_path()
                .and_then(|p| std::fs::read_to_string(p).ok())
                .unwrap_or_else(|| "# (no document)\n".to_string());
            let blocks = parser::parse(&md);
            self.content = Some(dlayout::layout(
                &blocks,
                SIDEBAR_W + theme::H_PAD,
                content_w,
                0.0,
                render::measure_text,
            ));
            self.laid_out_w = content_w;
        }
        let total = self.content.as_ref().map(|l| l.total_h).unwrap_or(0.0);
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

    /// Map a client-pixel click to an href, if it landed on a sidebar
    /// entry or an in-page link.
    fn hit_test(&self, px_x: f32, px_y: f32) -> Option<String> {
        let scale = self.dip_scale();
        let dx = px_x * scale;
        let dy = px_y * scale;
        if let Some(sb) = &self.sidebar {
            for h in &sb.hits {
                if dx >= h.x0 && dx <= h.x1 && dy >= h.y0 && dy <= h.y1 {
                    return Some(h.href.clone());
                }
            }
        }
        if let Some(c) = &self.content {
            for h in &c.hits {
                let y0 = h.y0 - self.scroll_y;
                let y1 = h.y1 - self.scroll_y;
                if dx >= h.x0 && dx <= h.x1 && dy >= y0 && dy <= y1 {
                    return Some(h.href.clone());
                }
            }
        }
        None
    }

    fn navigate(&mut self, href: &str) {
        let cur = self.current_path().unwrap_or_else(|| self.docs_dir.clone());
        if let Some(target_path) = resolve_href(href, &cur, &self.docs_dir) {
            if let Some(idx) = self.files.iter().position(|f| same_file(&f.path, &target_path)) {
                if idx != self.current {
                    self.current = idx;
                    self.content = None; // force relayout of the new doc
                    self.scroll_y = 0.0;
                    self.invalidate();
                }
            }
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
        let content_w = (viewport_w - SIDEBAR_W - 2.0 * theme::H_PAD).max(1.0);
        self.relayout(content_w, viewport_h);

        let target = match self.target.clone() {
            Some(t) => t,
            None => return,
        };

        unsafe {
            target.BeginDraw();
            let bg = theme::hex(theme::BG);
            target.Clear(Some(std::ptr::addr_of!(bg)));

            // Sidebar strip + its TOC, then a divider, then the content.
            render::fill_rect(&target, 0.0, 0.0, SIDEBAR_W, viewport_h, theme::SIDEBAR_BG);
            if let Some(sb) = self.sidebar.as_ref() {
                let _ = render::draw_document(&target, sb, 0.0, viewport_h);
            }
            render::fill_rect(&target, SIDEBAR_W - 1.0, 0.0, 1.5, viewport_h, theme::BORDER);
            if let Some(c) = self.content.as_ref() {
                let _ = render::draw_document(&target, c, self.scroll_y, viewport_h);
            }
            let _ = target.EndDraw(None, None);
        }
    }
}

// ─── Class registration ─────────────────────────────────────────────
pub fn register_class() -> Result<(), super::IGuiError> {
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
    /// A docs folder (→ sidebar + first doc) or a single `.md` file
    /// (→ that file's folder as the docs set, opened at that file).
    path: String,
}

/// Open a doc-pane.  `path` is a docs folder or a `.md` file.  Marshals
/// to the GUI thread (state alloc + WM_MDICREATE).
pub fn open(title: &str, path: &str) -> Option<i64> {
    window::open_doc_child(title, path)
}

pub(super) fn create_on_gui_thread(mdi: HWND, title_utf16: &[u16], path: &str) -> Option<i64> {
    let child_id = registry::allocate_child_id();
    let bootstrap = Box::into_raw(Box::new(DocBootstrap {
        child_id,
        path: path.to_owned(),
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
        SendMessageW(mdi, WM_MDICREATE, Some(WPARAM(0)), Some(LPARAM(&create as *const _ as isize)))
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
        let win_state = Box::new(DocWindowState::new(hwnd, child_id, &bootstrap.path));
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
            let delta = ((wparam.0 >> 16) & 0xFFFF) as i16 as f32;
            state.scroll_by(-(delta / 120.0) * WHEEL_STEP);
            LRESULT(0)
        }
        WM_LBUTTONDOWN => {
            let x = (lparam.0 & 0xFFFF) as i16 as f32;
            let y = ((lparam.0 >> 16) & 0xFFFF) as i16 as f32;
            if let Some(href) = state.hit_test(x, y) {
                state.navigate(&href);
            }
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
