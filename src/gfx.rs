//! gfx — the Factor-callable graphics FFI.
//!
//! These `rt_gpane_*` functions are what the `forth.wf64-gfx` vocab
//! (baked into `factorforth.image`) reaches through to draw.  They
//! were originally defined in WF64's `runtime.rs`; when we cut the
//! `wf64` crate dependency and forked the window shell into our own
//! `igui` crate, these came with us — rewritten to call `igui::*`
//! directly.  So the graphics path is now ours end to end: Factor →
//! these exports → `igui`'s batch/executor → Direct2D.  No WF64, no
//! JASM, no NewGC.
//!
//! Model: Forth opens a graphical MDI child (`gpane-open`), calls
//! `gpane-begin id`, issues any number of draw primitives, then
//! `gpane-present`.  Colours are packed `0xRRGGBB` in one Forth cell;
//! coordinates are signed cells (pixels).  Draw primitives push onto
//! the worker thread's current batch (thread-local); the paint
//! happens on the GUI thread after `present` submits the batch.
//!
//! The `build.rs` export list must carry every `rt_gpane_*` name so
//! it lands in the exe's export table for Factor's `GetProcAddress`.

/// Open a graphical MDI child sized `width x height` with the given
/// UTF-8 title (`title_addr..title_addr+title_len`).  Returns the
/// child id (positive) on success, 0 on failure.
#[no_mangle]
pub extern "C" fn rt_gpane_open(
    width: u64,
    height: u64,
    title_addr: u64,
    title_len: u64,
) -> u64 {
    #[cfg(windows)]
    {
        let title = unsafe {
            std::slice::from_raw_parts(title_addr as *const u8, title_len as usize)
        };
        let title = std::str::from_utf8(title).unwrap_or("∴ gpane");
        match igui::window::open_child_sized(title, width as i32, height as i32) {
            Some(id) => id as u64,
            None => 0,
        }
    }
    #[cfg(not(windows))]
    {
        let _ = (width, height, title_addr, title_len);
        0
    }
}

/// Begin a draw batch targeting `child_id`.  Replaces any in-progress
/// batch on this thread.  Pair with `rt_gpane_present`.
#[no_mangle]
pub extern "C" fn rt_gpane_begin(child_id: u64) -> u64 {
    #[cfg(windows)]
    {
        igui::batch::begin(child_id as i64);
    }
    let _ = child_id;
    0
}

/// Submit the current batch to the GUI thread; paints next frame.
/// No-op if no batch is in progress.
#[no_mangle]
pub extern "C" fn rt_gpane_present() -> u64 {
    #[cfg(windows)]
    {
        if let Some(batch) = igui::batch::finish() {
            igui::batch::submit(batch);
        }
    }
    0
}

/// Decode a 24-bit packed RGB cell (0xRRGGBB) into float RGBA.
/// Alpha is always 1.0.
#[cfg(windows)]
fn rgb_to_rgba(packed: u64) -> igui::batch::Rgba {
    let r = ((packed >> 16) & 0xFF) as f32 / 255.0;
    let g = ((packed >> 8) & 0xFF) as f32 / 255.0;
    let b = (packed & 0xFF) as f32 / 255.0;
    igui::batch::Rgba { r, g, b, a: 1.0 }
}

/// Clear the pane with `rgb` (packed 0xRRGGBB).
#[no_mangle]
pub extern "C" fn rt_gpane_clear(rgb: u64) -> u64 {
    #[cfg(windows)]
    {
        igui::batch::push(igui::batch::SurfaceCmd::Clear { color: rgb_to_rgba(rgb) });
    }
    let _ = rgb;
    0
}

/// Fill a rectangle at (x, y) with size (w × h).
#[no_mangle]
pub extern "C" fn rt_gpane_fill_rect(x: u64, y: u64, w: u64, h: u64, rgb: u64) -> u64 {
    #[cfg(windows)]
    {
        let (x, y, w, h) = (x as i64 as f32, y as i64 as f32, w as i64 as f32, h as i64 as f32);
        igui::batch::push(igui::batch::SurfaceCmd::FillRect {
            rect: igui::batch::Rect { x0: x, y0: y, x1: x + w, y1: y + h },
            corner_radius: 0.0,
            color: rgb_to_rgba(rgb),
        });
    }
    let _ = (x, y, w, h, rgb);
    0
}

/// Stroke a rectangle.  `thick` is the line thickness in pixels.
#[no_mangle]
pub extern "C" fn rt_gpane_stroke_rect(x: u64, y: u64, w: u64, h: u64, thick: u64, rgb: u64) -> u64 {
    #[cfg(windows)]
    {
        let (x, y, w, h) = (x as i64 as f32, y as i64 as f32, w as i64 as f32, h as i64 as f32);
        let thick = thick as i64 as f32;
        igui::batch::push(igui::batch::SurfaceCmd::StrokeRect {
            rect: igui::batch::Rect { x0: x, y0: y, x1: x + w, y1: y + h },
            corner_radius: 0.0,
            half_thickness: thick / 2.0,
            color: rgb_to_rgba(rgb),
        });
    }
    let _ = (x, y, w, h, thick, rgb);
    0
}

/// Draw a line from (x0,y0) to (x1,y1) with thickness `thick`.
#[no_mangle]
pub extern "C" fn rt_gpane_line(x0: u64, y0: u64, x1: u64, y1: u64, thick: u64, rgb: u64) -> u64 {
    #[cfg(windows)]
    {
        let x0 = x0 as i64 as f32; let y0 = y0 as i64 as f32;
        let x1 = x1 as i64 as f32; let y1 = y1 as i64 as f32;
        let thick = thick as i64 as f32;
        igui::batch::push(igui::batch::SurfaceCmd::DrawLine {
            p0: igui::batch::Point { x: x0, y: y0 },
            p1: igui::batch::Point { x: x1, y: y1 },
            half_thickness: thick / 2.0,
            color: rgb_to_rgba(rgb),
        });
    }
    let _ = (x0, y0, x1, y1, thick, rgb);
    0
}

/// Fill a circle centered at (cx,cy) with radius `r`.
#[no_mangle]
pub extern "C" fn rt_gpane_fill_circle(cx: u64, cy: u64, r: u64, rgb: u64) -> u64 {
    #[cfg(windows)]
    {
        let cx = cx as i64 as f32; let cy = cy as i64 as f32; let r = r as i64 as f32;
        igui::batch::push(igui::batch::SurfaceCmd::FillCircle {
            center: igui::batch::Point { x: cx, y: cy },
            radius: r,
            color: rgb_to_rgba(rgb),
        });
    }
    let _ = (cx, cy, r, rgb);
    0
}

// ─── Forth-side event API ────────────────────────────────────────────
//
// `gpane-next-event ( child_id timeout-ms -- p4 p3 p2 p1 kind )` pulls
// the next event matching `child_id` (or a global like FrameClose)
// from the iGui mailbox.  Non-matching infrastructure events are
// stashed and picked up by the worker's normal drain.  `timeout-ms < 0`
// blocks indefinitely; on timeout the kind is EV_NONE and all params
// are 0 (same shape, so Forth's stack effect stays predictable).
//
// Event-kind tags mirror IGuiEvent variants; the Factor-side constants
// in the forth.wf64-gfx vocab use these values.
pub const EV_NONE:        i64 = 0;
pub const EV_KEY:         i64 = 1;
pub const EV_CHAR:        i64 = 2;
pub const EV_MOUSE:       i64 = 3;
pub const EV_FOCUS:       i64 = 4;
pub const EV_RESIZE:      i64 = 5;
pub const EV_CLOSE:       i64 = 6;
pub const EV_FRAME_CLOSE: i64 = 7;
pub const EV_TICK:        i64 = 13;

/// Decode an `IGuiEvent` into (kind, p1, p2, p3, p4).  Returns
/// `EV_NONE` for variants Forth doesn't surface (infra events).
#[cfg(windows)]
fn decode_event(ev: &igui::channels::IGuiEvent) -> (i64, i64, i64, i64, i64) {
    use igui::channels::IGuiEvent;
    match ev {
        IGuiEvent::Key { vkey, mods, repeat, down, .. } =>
            (EV_KEY, *vkey, *mods, if *down { 1 } else { 0 }, *repeat),
        IGuiEvent::Char { codepoint, mods, .. } => (EV_CHAR, *codepoint, *mods, 0, 0),
        IGuiEvent::Mouse { x, y, op, button, mods, .. } =>
            (EV_MOUSE, *x, *y, *op, *mods | (*button << 8)),
        IGuiEvent::Focus { gained, .. } => (EV_FOCUS, if *gained { 1 } else { 0 }, 0, 0, 0),
        IGuiEvent::Resize { width, height, .. } => (EV_RESIZE, *width, *height, 0, 0),
        IGuiEvent::Close { .. } => (EV_CLOSE, 0, 0, 0, 0),
        IGuiEvent::FrameClose => (EV_FRAME_CLOSE, 0, 0, 0, 0),
        IGuiEvent::Tick { time_ms, .. } => (EV_TICK, *time_ms, 0, 0, 0),
        IGuiEvent::DpiChange { .. }
        | IGuiEvent::ThemeChange
        | IGuiEvent::Menu { .. }
        | IGuiEvent::EvalBuffer { .. }
        | IGuiEvent::ForthRestart
        | IGuiEvent::ForthInterrupt
        | IGuiEvent::ReplSubmit { .. } => (EV_NONE, 0, 0, 0, 0),
    }
}

/// Block up to `timeout_ms` for the next event matching `child_id`
/// (or a global).  Writes the decoded event into the five `out_*`
/// slots.  Returns 1 if an event was returned, 0 on timeout.  On 0
/// the slots are zeroed so Forth always pops the same five cells.
#[no_mangle]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn rt_gpane_next_event_for(
    child_id: u64,
    timeout_ms: u64,
    out_kind: *mut i64,
    out_p1: *mut i64,
    out_p2: *mut i64,
    out_p3: *mut i64,
    out_p4: *mut i64,
) -> u64 {
    unsafe {
        if !out_kind.is_null() { *out_kind = 0; }
        if !out_p1.is_null()   { *out_p1   = 0; }
        if !out_p2.is_null()   { *out_p2   = 0; }
        if !out_p3.is_null()   { *out_p3   = 0; }
        if !out_p4.is_null()   { *out_p4   = 0; }
    }

    #[cfg(windows)]
    {
        let id = child_id as i64;
        let timeout = timeout_ms as i64;
        // Loop on EV_NONE: an infra event we don't surface gets
        // stashed back for the main drain, then we retry rather than
        // collapsing the caller's timeout to 0.
        loop {
            let Some(ev) = igui::channels::next_event_for(id, timeout) else {
                return 0;
            };
            let (kind, p1, p2, p3, p4) = decode_event(&ev);
            if kind == EV_NONE {
                igui::channels::stash_event(ev);
                continue;
            }
            unsafe {
                if !out_kind.is_null() { *out_kind = kind; }
                if !out_p1.is_null()   { *out_p1   = p1;   }
                if !out_p2.is_null()   { *out_p2   = p2;   }
                if !out_p3.is_null()   { *out_p3   = p3;   }
                if !out_p4.is_null()   { *out_p4   = p4;   }
            }
            return 1;
        }
    }
    #[cfg(not(windows))]
    {
        let _ = (child_id, timeout_ms, out_kind, out_p1, out_p2, out_p3, out_p4);
        0
    }
}
