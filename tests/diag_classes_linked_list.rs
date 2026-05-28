//! Verify the linked-list worked example from classes.md actually
//! runs end-to-end.  If the doc shows it, it has to work.

#![cfg(target_os = "windows")]

use std::sync::{Arc, Mutex};
use newfactor::compiler::{compile_in_context, CompileContext};
use newfactor::session::{IoMode, Session, SessionOpts};

fn fresh() -> (Session, Arc<Mutex<Vec<u8>>>, CompileContext) {
    let out = Arc::new(Mutex::new(Vec::<u8>::new()));
    let opts = SessionOpts::defaults_for_crate(IoMode::Test {
        input: vec![], output: out.clone(),
    });
    let s = Session::new(opts).expect("Session::new");
    (s, out, CompileContext::new())
}

fn captured(out: &Arc<Mutex<Vec<u8>>>) -> String {
    String::from_utf8_lossy(&out.lock().unwrap()).to_string()
}

#[test]
#[ignore]
fn linked_list_length_and_sum() {
    let (s, out, mut ctx) = fresh();
    let src = r#"
        CLASS: nil-node ;
        CLASS: cons-node
            SLOT: head
            SLOT: tail
        ;
        <nil-node> VALUE nil

        : prepend ( elt list -- list' )
            swap <cons-node> ;

        GENERIC: list-length ( l -- n )
        METHOD: list-length ( n:nil-node -- n )
            drop 0 ;
        METHOD: list-length ( c:cons-node -- n )
            cons-node>tail list-length  1+ ;

        GENERIC: list-sum ( l -- n )
        METHOD: list-sum ( n:nil-node -- n )
            drop 0 ;
        METHOD: list-sum ( c:cons-node -- n )
            dup cons-node>tail list-sum
            swap cons-node>head + ;

        nil 1 prepend 2 prepend 3 prepend     \ list: 3 → 2 → 1 → nil
        dup list-length .                      \ 3
        list-sum .                             \ 6
    "#;
    let ir = compile_in_context(src, &mut ctx);
    let ir = match ir {
        Ok(ir) => ir,
        Err(e) => { eprintln!("compile err: {e}"); panic!("compile: {e}"); }
    };
    eprintln!("IR:\n{ir}");
    s.eval(&ir).expect("eval");
    let cap = captured(&out);
    eprintln!("captured: {cap:?}");
    assert!(cap.contains("3"), "list-length should be 3: {cap}");
    assert!(cap.contains("6"), "list-sum should be 6: {cap}");
}
