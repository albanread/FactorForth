! NewFactor functional test script.
USE: vocabs.loader
<< "E:\\NewFactor" add-vocab-root >>

USING: forth.all forth.core forth.variables forth.memory forth.preparser
       io kernel math namespaces prettyprint ;
FROM: forth.variables => @ ;
IN: scratchpad

"=== Basic arithmetic ===" print
1 2 + . nl
10 3 - . nl
4 5 * . nl

"=== variable (Factor source, var! for store) ===" print
variable counter
counter .                            ! address
counter @ .                          ! should print 0
42 counter var!
counter @ .                          ! should print 42
3 counter +!
counter @ .                          ! should print 45

"=== value / to ===" print
0 value score
score .                              ! should print 0
99 to score
score .                              ! should print 99

"=== constant ===" print
42 constant answer
answer .                             ! should print 42

"=== defer / is ===" print
defer myop
[ 2 * ] is myop
5 myop .                             ! should print 10

"=== >r r> ===" print
1 2 3 >r >r r> r> . . .             ! should print 2 1 3

"=== {: locals :} ===" print
{: add3 a b c -- result :}
    a b + c + ;
1 2 3 add3 .                         ! should print 6

{: quadratic a b c -- result :}
    a a * b b * + c c * + ;
3 4 0 quadratic .                    ! should print 25

{: zero-sum a b | z -- result :}
    a b + z + ;
10 20 zero-sum .                     ! should print 30

"=== memory / create ===" print
variable x
variable y
y x - .                              ! should print 8  (consecutive cells)
create my-pair  16 allot
10 my-pair var!
20 my-pair 8 + var!
my-pair @ .                          ! should print 10
my-pair 8 + @ .                      ! should print 20
3 cells .                            ! should print 24
cell .                               ! should print 8

"=== forth-load preparser ===" print
! demo.fth uses plain Forth syntax: ! for store, IF/THEN/ELSE, BEGIN/UNTIL etc.
! The preparser rewrites these before Factor's lexer sees the file.
"E:\\NewFactor\\demo.fth" forth-load

"=== Done ===" print
