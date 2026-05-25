<< "E:\\NewFactor" add-vocab-root >>
USE: forth.all
USE: io
USE: kernel
USE: words
USE: vocabs
USE: sequences
USE: prettyprint

! Find all words named "@" in all loaded vocabs
loaded-vocabs [
    vocab-words [
        name>> "@" =
    ] filter
    dup empty? not [
        first vocabulary>> "Vocab: " write write " word: @" print
    ] [ drop ] if
] each
