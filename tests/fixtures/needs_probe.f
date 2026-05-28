\ Fixture for the NEEDS include-once tests (tests/diag_needs.rs).
\ The top-level marker prints when this file is pulled in, so a test
\ can count how many times it loaded.  probe-word proves an included
\ definition is callable from code that follows the NEEDS.

: probe-word ( -- n ) 42 ;

." [probe-loaded]" cr
