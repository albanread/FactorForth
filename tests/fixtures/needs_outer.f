\ Fixture: pulls in a sibling by a path relative to ITS OWN directory
\ (not the process CWD), proving NEEDS resolves nested includes
\ relative to the including file.

NEEDS needs_probe.f

: outer-word ( -- n ) probe-word 1 + ;

." [outer-loaded]" cr
