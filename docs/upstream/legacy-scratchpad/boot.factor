! NewFactor boot script.
!
! Run with:
!   E:\factor\factor.com E:\NewFactor\boot.factor
!
! << >> runs at parse time so the vocab root is registered before
! any USE: / USING: directives are resolved.

<< "E:\\NewFactor" add-vocab-root >>

USE: forth.all
USE: listener

listener
