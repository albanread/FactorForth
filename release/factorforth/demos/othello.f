\ othello.f — a text Othello demo on CoreProtocols.
\
\ Loads the standard library (core + collections) and the Othello
\ engine, then plays the four standard opening moves and prints the
\ board after each.  Run it from the IDE:  S" demos/othello.f" INCLUDED
\
\ Everything below the libraries is ordinary ANS Forth: the board is
\ a CoreProtocols `grid`, and the move engine is plain words on top.

S" lib/core.f"        INCLUDED
S" lib/collections.f" INCLUDED
S" lib/othello.f"     INCLUDED

othello-new
." Opening position:" cr
show-board cr

." Black plays (2,3):" cr
2 3 black play
show-board cr

." White plays (2,2):" cr
2 2 white play
show-board cr

." Black plays (2,4):" cr
2 4 black play
show-board cr
