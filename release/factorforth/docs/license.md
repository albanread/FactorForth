# License

FactorForth itself: **BSD-3-Clause**.

```
Copyright (c) 2025-2026, the FactorForth authors.

Redistribution and use in source and binary forms, with or
without modification, are permitted provided that the following
conditions are met:

1. Redistributions of source code must retain the above
   copyright notice, this list of conditions and the following
   disclaimer.

2. Redistributions in binary form must reproduce the above
   copyright notice, this list of conditions and the following
   disclaimer in the documentation and/or other materials
   provided with the distribution.

3. Neither the name of the copyright holder nor the names of
   its contributors may be used to endorse or promote products
   derived from this software without specific prior written
   permission.

THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND
CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF
MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR
CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT
NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES;
LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION)
HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN
CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR
OTHERWISE) ARISING IN ANY WAY OUT OF THE USE OF THIS SOFTWARE,
EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
```

## Third-party components

### Factor (factor.dll and factorforth.image)

```
Copyright (C) 2003-2025, Slava Pestov and contributors.
All rights reserved.

Redistribution and use in source and binary forms, with or
without modification, are permitted provided that the following
conditions are met:

1. Redistributions of source code must retain the above
   copyright notice, this list of conditions and the following
   disclaimer.

2. Redistributions in binary form must reproduce the above
   copyright notice, this list of conditions and the following
   disclaimer in the documentation and/or other materials
   provided with the distribution.

THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS "AS IS" AND
ANY EXPRESS OR IMPLIED WARRANTIES ARE DISCLAIMED.
```

Factor source: <https://github.com/factor/factor>.  We ship a
slightly patched build (`vm/factor.cpp` extended with
`nf_init_factor` / `nf_eval_string` embedding entry points);
the patch is in our source tree under `vm-build/`.

### iGui (Direct2D MDI front-end)

Shared with the WF64 project at `E:\WF64\`.  Same BSD-3-Clause
terms as FactorForth itself.

### DocCrate (the documentation browser)

```
Copyright (c) 2025, the DocCrate authors.
BSD-3-Clause.
```

Source: `E:\DocCrate\` (sibling repo).

### Windows API bindings

`windows` crate: Apache-2.0 / MIT dual licensed.
`libloading` crate: ISC.
`pulldown-cmark` (used by DocCrate): MIT.

## Trademark

"FactorForth" is just a project name.  Factor is a trademark
of Slava Pestov / the Factor community; FactorForth uses
Factor's VM under license but is not affiliated with the
Factor project.
