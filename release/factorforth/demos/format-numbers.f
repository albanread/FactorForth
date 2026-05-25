\ Pictured numeric output — the ANS DSL <# # #S sign hold #>
\ is a tiny string-builder.  Each word manipulates an implicit
\ accumulator; #> closes it and yields (c-addr u).

\ Simple cases via the n>$ convenience.
1234 n>$ type cr               \ → 1234
-7   n>$ type cr               \ → -7
0    n>$ type cr               \ → 0

\ DSL form, with the standard signed-decimal incantation.
." decimal: "
-42 dup abs <# #S swap sign #> type cr

\ Hex with a "0x" prefix — HOLD pushes the literal characters,
\ then sign + #> close the session.
." hex:     "
255 hex
  dup abs <# #S
    120 hold              \ 'x' = 0x78
    48  hold              \ '0' = 0x30
  swap sign
  #> type cr
decimal

\ Binary, no sign, no prefix.
." binary:  "
binary
13 <# #S #> type cr
decimal
