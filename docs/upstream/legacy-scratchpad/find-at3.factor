<< "E:\\NewFactor" add-vocab-root >>
USE: forth.all
USE: io
USE: kernel
USE: words
USE: vocabs
USE: sequences
USE: prettyprint

! List all NewFactor vocabs that have a word named "@"
"Looking for @ in all forth.* vocabs:" print
{
    "forth.variables"
    "forth.core"
    "forth.numeric"
    "forth.doubles"
    "forth.strings"
    "forth.structures"
    "forth.wf-ext"
    "forth.dictionary"
    "forth.all"
} [
    dup write " -> " write
    lookup-vocab vocab-words
    [ name>> "@" = ] filter
    [ name>> ] map .
] each
