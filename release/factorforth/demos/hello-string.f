\ ANS strings the way they should have always worked: no PAD,
\ no clobbering, GC'd buffers.

s" hello, world" type cr

\ A fresh byte buffer.  ANS-style accessor takes an index:
\ `0 buf` is the address of byte 0.
80 cbuffer buf

\ Fill 80 bytes starting at buf[0] with ASCII space.
0 buf 80 bl fill
0 buf 10 type cr           \ ten spaces

\ Write 'H' 'i' into the buffer; type the prefix.
72  0 buf c!
105 1 buf c!
0 buf 2 type cr            \ → "Hi"
