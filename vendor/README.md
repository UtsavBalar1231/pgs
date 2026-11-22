# Vendored libgit2

PGS builds against the published `libgit2-sys` 0.18.8+1.9.7 bindings and libgit2
1.9.7 sources, with a backport for [libgit2 #7284]. The source archive has
SHA-256 `7f7c568b25d7489bc3fb2988ed69ab111d2944d2f5fec3d5c987fe545ea97b50`.
This copy contains the files used by the Cargo build and their license notices.
The crate uses its original `Cargo.toml` from the published `Cargo.toml.orig`.

The backport changes `libgit2/src/libgit2/ignore.c` and `iterator.c`. It keeps
nested negations for rule lookup and preserves ignored ancestor directories
during traversal. [Upstream PR #7339] covers the same bug.

Once `libgit2-sys` ships the upstream fix, remove the Cargo override and this
vendored crate, update `Cargo.lock`, and run
`cargo test --locked --test test_scan scan_nested_ignore_exception_lists_only_reincluded_files`.

[libgit2 #7284]: https://github.com/libgit2/libgit2/issues/7284
[Upstream PR #7339]: https://github.com/libgit2/libgit2/pull/7339
