# Third-party notices

## SQLite WebAssembly wrapper

This static application vendors `@sqlite.org/sqlite-wasm` version
`3.53.0-build1` from <https://github.com/sqlite/sqlite-wasm>.

The wrapper is licensed under Apache-2.0. The license text is included at
`vendor/sqlite/LICENSE.txt`. The underlying SQLite database engine is dedicated
to the public domain; see <https://www.sqlite.org/copyright.html>.

The vendored files are unmodified copies of the package's `dist/index.mjs` and
`dist/sqlite3.wasm` artifacts. They are deliberately checked in so publishing a
catalog snapshot does not compile SQLite or require a package registry.

Pinned SHA-256 digests:

- `index.mjs`: `f80870f0fa03a39a3338d17ed3fbea04808d344c88e724d90d5f37b9b7b83154`
- `sqlite3.wasm`: `02d7e48164395fa68f81c6ec33e9da5461be397dc57602ac0cd89b4bbba1d312`
