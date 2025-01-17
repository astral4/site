# `site/ssg`

## Introduction

This directory contains the source code of `ssg`, the static site generator for my website.

### Design philosophy

- Do the reasonable thing.
- Prefer [semantic HTML](https://web.dev/learn/html/semantic-html).
- Report useful error messages.

Tasks with lots of room for customization are left up to the user. This includes favicon generation—see [here](https://evilmartians.com/chronicles/how-to-favicon-in-2021-six-files-that-fit-most-needs), [here](https://dev.to/masakudamatsu/favicon-nightmare-how-to-maintain-sanity-3al7), and [here](https://css-tricks.com/how-to-favicon-in-2021/) for various conflicting practices over the years—and `robots.txt` generation. There might be zero or many best approaches, or an approach that is unlikely to remain best in the long term; `ssg` defers for simplicity.

## Installation

1. Make sure Rust and Cargo are [installed](https://www.rust-lang.org/tools/install)
2. Run `cargo install --bin ssg` if the current working directory is `site/`, or simply `cargo install` if the current working directory is `ssg/`
3. You're done!

## How to use this tool

`ssg` is a command line program that accepts exactly one argument: the path to a config file.

```
ssg path/to/config.toml
```

### The configuration file schema

The config file must be in [TOML](https://toml.io/en/) and is expected to have the following fields:

- `output_dir` (string)
  - path to a directory where website files will be written to
  - cannot point to an existing directory
- `site_css_file` (string)
  - path to a file containing CSS to be applied to the entire website
- `head_template_html_file` (string)
  - path to a file containing HTML to be inserted in the `<head>` of every page
  - example uses: custom `<meta>` tags; favicon `<link>` tags
- `body_template_html_file` (string)
  - path to a file containing HTML to be inserted in the `<body>` of every page
  - must contain a `<main>` element for slotting page content
- `fragments`
  - an array of tables; each table must have the following fields:
    - `title` (string)
      - string to be used as the output page's title
    - `path` (string)
      - path to a file containing HTML to be inserted within the contents of `body_template_html_file`
      - the file name determines the output path (e.g. `foo/bar/index.html` maps to `<output dir>/index.html` and `/path/to/about-me.html` maps to `<output dir>/about-me/index.html`)
  - every `path` must point to a different location
  - example uses: non-article pages; pages with custom HTML
- `articles_dir` (string)
  - path to a directory containing Markdown files
  - files are converted to HTML and inserted within the contents of `body_template_html_file`
  - pages are written to `<output dir>/writing/`
- `code_theme` (string)
  - name of theme for code syntax highlighting in articles
  - supported values: `"base16-ocean.dark"`; `"base16-eighties.dark"`; `"base16-mocha.dark"`; `"base16-ocean.light"`; `"InspiredGitHub"`; `"Solarized (dark)"`; `"Solarized (light)"`

Example of a valid config file:

```toml
output_dir = "my-cool-website/"
site_css_file = "my-cool-styles.css"
head_template_html_file = "abc/xyz.html"
body_template_html_file = "/layout.html"
fragments = [
    { title = "Welcome", path = "/files/index.html" },
    { title = "About me", path = "path/to/about.html" },
]
articles_dir = "my-cool-articles/"
code_theme = "base16-mocha.dark"
```

>[!NOTE]
>Relative paths within the config file are interpreted as relative to the config file's path, **not** relative to the current working directory when the program runs.

### The Markdown frontmatter schema

Markdown files within `articles_dir` must have [YAML](https://yaml.org)-style frontmatter with the following fields:

- `title` (string)
  - string to be used as the output page's title
- `slug` (string)
  - string used to determine the output page's path (e.g. `all-about-animals` will make have the program write to `<output dir>/writing/all-about-animals/`)
  - cannot contain `/`, `\`, or `:`
  - must be unique across all Markdown files
- `created` (string)
  - date in `YYYY-MM-DD` format displayed with the page title
  - indicates when the article was created
- `updated` (string; optional)
  - date in `YYYY-MM-DD` format displayed with the page title
  - indicates when the article was last updated/edited
  - cannot chronologically precede `created`

Example of valid frontmatter:

```yaml
---
title: "Check out this computer program!"
slug: "my-favorite-program"
created: "2036-08-12"
---
```

### Emitted classes

Generated pages contain some elements with specific HTML `class` names. They are listed here in case you want to target them in CSS.

- `__article-heading`
  - `<hgroup>` element at the beginning of article pages
  - contains the title heading and date string 
- `__article-list`
  - `<ol>` element on the article archive page
  - child `<li>` entries contain article creation dates, titles, and links
- `__article-date`
  - `<p>` element within `__article-list`
  - contains the article creation date

## Features

### Templates

`head_template_html_file` and `body_template_html_file` let you insert snippets of HTML into every generated page, making site-wide layouts and themes possible.

### CSS processing

`ssg` converts the CSS in `site_css_file` to styling rules compatible with a set of baseline browser versions, so you can use the latest CSS features without worrying about browser compatibility. Output CSS is also minified to save disk space and bandwidth.

### Font loading optimization

`ssg` parses the CSS in `site_css_file` and inlines [`@font-face` declarations](https://developer.mozilla.org/en-US/docs/Web/CSS/@font-face) in the HTML of every page. Pages also include `<link>` elements for preloading fonts based on font URLs detected in the CSS.

[Inlining](https://web.dev/learn/performance/optimize-web-fonts#inline_font-face_declarations) and [preloading](https://web.dev/learn/performance/optimize-web-fonts#preload) improve page loading and rendering performance. The combination of these two strategies also prevents [FOUT](https://en.wikipedia.org/wiki/Flash_of_unstyled_content).

### Flexible Markdown file organization

`ssg` recursively searches for files with the `.md` extension within `articles_dir`. This allows you to freely structure your articles. For example, you might put articles inside directories by year, organize articles by title in alphabetical order, or maintain a flat structure with one directory containing all files. `ssg` will process everything as long as it is contained in a single parent directory (`articles_dir`).

### Markdown extensions

`ssg` parses and processes some syntax extensions to the original Markdown specification: [tables](https://www.markdownguide.org/extended-syntax/#tables), [fenced code blocks](https://www.markdownguide.org/extended-syntax/#fenced-code-blocks), [strikethrough text](https://www.markdownguide.org/extended-syntax/#strikethrough), and math expressions.

### Smart punctuation

Straight single and double quotes in articles are automatically converted to their curly counterparts. This is done in the name of typographical correctness for [apostrophes](https://practicaltypography.com/apostrophes.html) and [quotation marks](https://practicaltypography.com/straight-and-curly-quotes.html).

### Image conversion

Images referenced in articles are converted to AVIF, a modern lossy image format with [broad support in web browsers](https://caniuse.com/avif). Compared to older formats like WebP and JPEG, AVIF offers better compression quality at equivalent file sizes. Existing AVIF images are simply copied to the output destination.

To opt out of conversion, use raw HTML (i.e. `<img>`) to include images. `ssg` will completely ignore images declared this way in articles, but this also means you are responsible for copying the image file to the output destination.

### Syntax highlighting

Multi-line code blocks in articles are converted to styled HTML. A variety of languages and themes are supported. For example, Markdown that looks like this...

````
```rs
fn main() {
    println!("Hello world!");
    let mut x = 2;
    for i in 0..10 {
        x += i;
    }
    println!("{x}");
}
```
````

...is rendered like this:

```rs
fn main() {
    println!("Hello world!");
    let mut x = 2;
    for i in 0..10 {
        x += i;
    }
    println!("{x}");
}
```

### LaTeX support

`ssg` supports math expressions. Inline expressions should be surrounded by single dollar signs (`$`); display expressions should be surrounded by double dollar signs (`$$`). For example, Markdown that looks like this...

```
$$\int\tfrac{x}{\sqrt{x^2+5}}~dx=\sqrt{x^2+5}+C$$
```

...is rendered like this:

$$\int\tfrac{x}{\sqrt{x^2+5}}~dx=\sqrt{x^2+5}+C$$

### Article archive

Articles are written to `<output dir>/writing/`. `ssg` also generates a page at `<output dir>/writing/index.html` containing a list of all articles. The articles are sorted by creation date in reverse chronological order, then by title in reverse lexicographical order.
