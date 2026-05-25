<< "E:\\NewFactor" add-vocab-root >>
USE: forth.all
USE: io
USE: kernel
USE: words
USE: vocabs
USE: sequences
USE: prettyprint

! Search ALL loaded vocabs for @
"All vocabs with @ word:" print
loaded-vocab-names [
    lookup-vocab vocab-words
    [ name>> "@" = ] filter
    dup empty? not [
        first vocabulary>> "  " write write nl
    ] [ drop ] if
] each
