# `site/katex-dl`

`katex-dl` is a crate for downloading distributions of the JavaScript library [KaTeX](https://katex.org/). This is done for vendoring purposes and is used by [my static site generator](../ssg/).

Every KaTeX font is distributed as WOFF2, WOFF, and TTF. Since an overwhelming majority of visitors use [browsers supporting WOFF2](https://caniuse.com/woff2), this crate only downloads fonts in the WOFF2 format. Additionally, the output KaTeX CSS file is modified to only specify WOFF2 font sources.
