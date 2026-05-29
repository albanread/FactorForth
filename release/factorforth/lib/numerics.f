\ numerics.f — CoreProtocols, Layer 2: numeric value types.
\
\ Load after core.f.  Two value classes — vec2 (a 2-D vector) and
\ complex (a complex number) — sharing a small arithmetic protocol.
\
\ The method bodies are written with LET, the infix-algebra DSL, so they
\ read like the mathematics rather than the stack: `ax + bx, ay + by`
\ instead of `-rot swap vec2>y swap vec2>y +`.  Components are floats
\ (the graphics and analysis toys want real arithmetic).
\
\ See docs/coreprotocols.md (Layer 2) for the design.

\ ── The arithmetic protocol ──────────────────────────────────────
\
\ A handful of generics both types implement.  v+ / v- / vscale return
\ the SAME type as their input; vmag returns a scalar.  v+ / v- key on
\ BOTH arguments (multiple dispatch), so mixing types — a vec2 plus a
\ complex — simply finds no method rather than silently misbehaving.
GENERIC: v+     ( a b -- c )      \ component-wise add
GENERIC: v-     ( a b -- c )      \ component-wise subtract
GENERIC: vscale ( v k -- c )      \ multiply by a scalar k
GENERIC: vmag   ( v -- n )        \ magnitude / modulus

\ ── vec2 — a 2-D vector ──────────────────────────────────────────
CLASS: vec2 SLOT: x SLOT: y ;

METHOD: v+ ( a:vec2 b:vec2 -- c )
    LET ( a:vec2 as ax ay, b:vec2 as bx by ) -> ( sx sy ) =
        ax + bx, ay + by
    END
    <vec2> ;
METHOD: v- ( a:vec2 b:vec2 -- c )
    LET ( a:vec2 as ax ay, b:vec2 as bx by ) -> ( dx dy ) =
        ax - bx, ay - by
    END
    <vec2> ;
METHOD: vscale ( v:vec2 k -- c )
    LET ( v:vec2 as x y, k ) -> ( px py ) =
        x * k, y * k
    END
    <vec2> ;
METHOD: vmag ( v:vec2 -- n )
    LET ( v:vec2 as x y ) -> ( m ) = sqrt(x^2 + y^2) END ;

\ vec2-specific: the dot product (a scalar, so not part of the shared
\ same-type-in/out protocol).
: dot ( a b -- n )
    LET ( a:vec2 as ax ay, b:vec2 as bx by ) -> ( d ) = ax * bx + ay * by END ;

METHOD: show ( v:vec2 -- )
    ." (" dup vec2>x . ." , " vec2>y . ." )" ;

\ ── complex — a complex number ───────────────────────────────────
CLASS: complex SLOT: re SLOT: im ;

METHOD: v+ ( a:complex b:complex -- c )
    LET ( a:complex as ar ai, b:complex as br bi ) -> ( sr si ) =
        ar + br, ai + bi
    END
    <complex> ;
METHOD: v- ( a:complex b:complex -- c )
    LET ( a:complex as ar ai, b:complex as br bi ) -> ( dr di ) =
        ar - br, ai - bi
    END
    <complex> ;
METHOD: vscale ( z:complex k -- c )
    LET ( z:complex as re im, k ) -> ( pr pi ) =
        re * k, im * k
    END
    <complex> ;
METHOD: vmag ( z:complex -- n )
    LET ( z:complex as re im ) -> ( m ) = sqrt(re^2 + im^2) END ;

\ complex-specific: the full product and the conjugate.
: c* ( a b -- c )
    LET ( a:complex as ar ai, b:complex as br bi ) -> ( pr pi ) =
        ar * br - ai * bi,
        ar * bi + ai * br
    END
    <complex> ;
: conj ( z -- z' )
    LET ( z:complex as re im ) -> ( r2 i2 ) = re, -im END
    <complex> ;

METHOD: show ( z:complex -- )
    dup complex>re . ." + " complex>im . ." i" ;
