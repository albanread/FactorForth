<< "E:\\NewFactor" add-vocab-root >>
USE: forth.all
USE: io
USE: kernel
USE: words
USE: sequences
USE: prettyprint
USE: namespaces
USE: vocabs.parser

"Search path vocabs with @ word:" print
manifest get search-vocabs>> [
    name>> dup write
    " -> " write
    lookup-vocab vocab-words
    [ name>> "@" = ] filter
    [ name>> ] map
    dup empty? [ drop "none" print ] [ . ] if
] each
