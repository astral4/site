# `site/neophf`

`neophf` is a crate for generating compile-time perfect hash maps. The design is very similar to [`phf`](https://crates.io/crates/phf), but this implementation is optimized for evaluation/query performance. I only implemented a small subset of `phf`'s API to keep the crate as simple as possible while still meeting the needs of this project.

For my prior work on this subject, see [`phf-sandbox`](https://github.com/astral4/phf-sandbox) and [`haph`](https://github.com/astral4/haph).
