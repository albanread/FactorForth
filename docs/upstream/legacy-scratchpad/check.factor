! NewFactor error check script.
<< "E:\\NewFactor" add-vocab-root >>

USE: forth.all
USE: prettyprint
USE: compiler.errors
USE: sequences
USE: io
USE: accessors
USE: assocs

"=== Compiler errors ===" print
compiler-errors get [
    [ drop . ] [ error>> . ] bi
    "---" print
] assoc-each
"=== Done ===" print
