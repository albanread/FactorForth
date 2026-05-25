! forth.all — Load all NewFactor vocabularies.
!
! Usage at Factor listener:
!
!   "E:\\NewFactor" add-vocab-root
!   USE: forth.all
!
! After that, all Forth-compatible words are in scope.

USING: forth.fstack forth.core forth.memory forth.variables forth.numeric forth.doubles
       forth.strings forth.structures forth.wf-ext forth.dictionary forth.locals
       forth.preparser ;
IN: forth.all

! All words are re-exported by importing the vocabs above.
! Nothing additional defined here; this is an aggregation module.
