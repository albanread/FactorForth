//! Diagnose why the IDE doesn't show output.  IoMode::Gui's
//! on_write closure should fire for every byte; tests confirm
//! IoMode::Test works, so we test the Gui path here.

#![cfg(target_os = "windows")]

use std::sync::Arc;
use std::sync::Mutex;

use newfactor::compiler::{compile_in_context, CompileContext};
use newfactor::session::{IoMode, Session, SessionOpts};

#[test]
#[ignore]
fn diag_gui_mode_with_flush_after_eval() {
    // Mimic the IDE binary exactly: line-buffered on_write +
    // flush callback that pushes any partial line.
    let lines = Arc::new(Mutex::new(Vec::<String>::new()));
    let line_buf = Arc::new(Mutex::new(String::new()));

    let buf_for_writer = line_buf.clone();
    let lines_for_writer = lines.clone();
    let on_write: Box<dyn FnMut(u8) + Send> = Box::new(move |b: u8| {
        let mut buf = buf_for_writer.lock().unwrap();
        if b == b'\n' {
            let line = std::mem::take(&mut *buf);
            drop(buf);
            lines_for_writer.lock().unwrap().push(line);
        } else {
            buf.push(b as char);
        }
    });
    let buf_for_flush = line_buf.clone();
    let lines_for_flush = lines.clone();
    let on_flush: Box<dyn FnMut() + Send> = Box::new(move || {
        let mut buf = buf_for_flush.lock().unwrap();
        if !buf.is_empty() {
            let line = std::mem::take(&mut *buf);
            drop(buf);
            lines_for_flush.lock().unwrap().push(line);
        }
    });

    let opts = SessionOpts::defaults_for_crate(IoMode::Gui {
        on_write, on_flush,
    });
    let session = Session::new(opts).expect("Session::new");

    let mut ctx = CompileContext::new();
    let ir = compile_in_context("10 . cr", &mut ctx).expect("compile");
    eprintln!("IR:\n{ir}");
    let res = session.eval(&ir);
    eprintln!("eval result: {res:?}");

    let collected = lines.lock().unwrap().clone();
    eprintln!("lines: {:?}", collected);
    assert!(!collected.is_empty(), "expected at least one line");
    assert!(collected.iter().any(|l| l.contains("10")),
        "expected '10' in some line; got {:?}", collected);
}

#[test]
#[ignore]
fn diag_gui_mode_writes_each_byte() {
    let bytes = Arc::new(Mutex::new(Vec::<u8>::new()));
    let bytes_for_writer = bytes.clone();
    let on_write: Box<dyn FnMut(u8) + Send> = Box::new(move |b: u8| {
        bytes_for_writer.lock().unwrap().push(b);
    });

    let on_flush: Box<dyn FnMut() + Send> = Box::new(|| {});
    let opts = SessionOpts::defaults_for_crate(IoMode::Gui { on_write, on_flush });
    let session = Session::new(opts).expect("Session::new");

    let mut ctx = CompileContext::new();
    let ir = compile_in_context("42 . cr", &mut ctx).expect("compile");
    eprintln!("IR being eval'd:\n{ir}");
    session.eval(&ir).expect("eval");

    let collected = bytes.lock().unwrap().clone();
    eprintln!("on_write received {} bytes: {:?}",
        collected.len(),
        String::from_utf8_lossy(&collected));

    assert!(!collected.is_empty(),
        "on_write should have been called at least once");
    assert!(collected.iter().any(|&b| b == b'4'),
        "expected '4' in output");
}
