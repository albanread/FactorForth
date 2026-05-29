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
\ Beyond the four core generics, two kinds of enrichment live here:
\   - type-specific extras (vec2: dot, normalize, perp; complex: c*,
\     conj, phase, recip, c/), and
\   - DERIVED protocol words (vneg) written once over the generics, so
\     they serve every protocol type for free.
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

\ normalize — the unit vector pointing the same way (v / |v|).  Built
\ on the protocol: one vmag, one vscale.  (Undefined for the zero
\ vector, as usual.)
: normalize ( v -- u )
    dup vmag 1e swap f/ vscale ;

\ perp — rotate 90° left: (x, y) -> (-y, x).
: perp ( v -- v' )
    LET ( v:vec2 as x y ) -> ( a b ) = -y, x END
    <vec2> ;

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

\ phase — the argument (angle from the positive real axis), atan2(im, re).
: phase ( z -- angle )
    LET ( z:complex as re im ) -> ( a ) = atan2(im, re) END ;

\ recip — the multiplicative inverse: conj(z) / |z|^2.
: recip ( z -- z' )
    LET ( z:complex as re im ) -> ( rr ri ) =
        re / (re^2 + im^2),
        -im / (re^2 + im^2)
    END
    <complex> ;

\ c/ — complex division a / b = a * conj(b) / |b|^2.
: c/ ( a b -- c )
    LET ( a:complex as ar ai, b:complex as br bi ) -> ( qr qi ) =
        (ar * br + ai * bi) / (br^2 + bi^2),
        (ai * br - ar * bi) / (br^2 + bi^2)
    END
    <complex> ;

METHOD: show ( z:complex -- )
    dup complex>re . ." + " complex>im . ." i" ;

\ ── Derived protocol words ───────────────────────────────────────
\
\ Written ONCE, over the generics above — so they work for EVERY type
\ that implements the protocol: vec2, complex, and whatever you add
\ tomorrow.  This is the point of a protocol: the algorithm names the
\ behaviour (v+, v-, vscale, vmag), never the concrete class, so a new
\ type joins the family the moment it answers those words.

\ vneg — the additive inverse, v scaled by -1.
: vneg ( v -- c )
    -1e vscale ;

\ vdist — the distance between two values: the magnitude of a - b.
: vdist ( a b -- n )
    v- vmag ;

\ vlerp — linear interpolation a -> b by t:  a + (b - a)*t.
\   >r          ( a b )        save t
\   over v-     ( a b-a )      b - a, keeping a
\   r> vscale   ( a (b-a)*t )  scale by t
\   v+          ( a+(b-a)*t )  add the base back
: vlerp ( a b t -- c )
    >r over v- r> vscale v+ ;

\ vmid — the midpoint, lerp at t = 0.5.
: vmid ( a b -- c )
    0.5e vlerp ;
