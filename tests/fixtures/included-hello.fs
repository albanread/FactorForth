\ tests/fixtures/included-hello.fs
\ A tiny Forth fixture that exercises INCLUDED.
\ Contents loaded by tests/session_file_access.rs.

." hello from included" cr

\ Words defined here CAN be invoked by other code IN THE SAME FILE,
\ since the included file is compiled as a single NewFactor unit.
\ (NewFactor's resolver runs at compile time — words defined in
\ an included file are not visible to a SEPARATE compilation
\ outside the file.  That's a limitation worth knowing about.)
: included-word  42 ;
." word produces: " included-word . cr
