use anyhow::{anyhow, Context as _, Error, Result};
use ego_tree::{tree, Tree};
use gray_matter::{engine::YAML, Matter};
use image::{codecs::avif::AvifEncoder, GenericImageView, ImageEncoder, ImageReader};
use jiff::civil::Date;
use markup5ever::{namespace_url, ns, Attribute, LocalName, QualName};
use pulldown_cmark::{Event, Options, Parser, TextMergeStream};
use rquickjs::{Context, Exception, Function, Object, Runtime};
use same_file::Handle;
use scraper::{
    node::{Doctype, Element, Node, Text},
    Html,
};
use serde::{
    de::{Error as DeError, Unexpected},
    Deserialize, Deserializer,
};
use std::{
    env::args,
    fs::{copy, read_to_string, File},
    io::BufWriter,
    path::Path,
};
use syntect::{
    highlighting::{Theme, ThemeSet},
    html::highlighted_html_for_string,
    parsing::SyntaxSet,
};
use toml_edit::de::from_str as toml_from_str;

const KATEX_SRC: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../katex/katex.js"));

const OUTPUT_SITE_CSS_FILE: &str = "/site.css";

#[derive(Deserialize)]
pub struct Config {
    // path to directory of all articles
    pub articles_dir: Box<Path>,
    // path to directory of all webpage body files;
    // meant for non-article pages like the site index and the "about" page
    pub body_dir: Box<Path>,
    // path to site-wide CSS file
    pub site_css_file: Box<Path>,
    // path to directory for generated site output
    pub output_dir: Box<Path>,
}

impl Config {
    /// Reads a config file from a path provided by command-line arguments.
    ///
    /// # Errors
    /// This function returns an error if:
    /// - not enough command-line arguments are provided
    /// - too many command-line arguments are provided
    /// - a config parameter interpreted as a directory path does not point to a directory
    /// - a config parameter interpreted as a file path does not point to a file
    /// - the output directory path and another path in the config point to the same location
    pub fn from_env() -> Result<Self> {
        let mut args = args();

        let config_path = args
            .next()
            .ok_or_else(|| anyhow!("configuration file path was not provided"))?;

        if args.next().is_some() {
            return Err(anyhow!("too many input arguments were provided"));
        }

        let config: Self = toml_from_str(
            &read_to_string(&config_path)
                .with_context(|| format!("failed to read configuration from {config_path}"))?,
        )
        .context("failed to parse configuration file")?;

        config
            .check_paths()
            .context("configuration file is invalid")?;

        Ok(config)
    }

    fn check_paths(&self) -> Result<()> {
        if !self.articles_dir.is_dir() {
            Err(anyhow!(
                "`articles_dir`: {:?} does not point to a directory",
                self.articles_dir
            ))
        } else if !self.body_dir.is_dir() {
            Err(anyhow!(
                "`body_dir`: {:?} does not point to a directory",
                self.body_dir
            ))
        } else if !self.site_css_file.is_file() {
            Err(anyhow!(
                "`site_css_file`: {:?} does not point to a file",
                self.site_css_file
            ))
        } else {
            let output_dir = get_handle(&self.output_dir)?;
            if output_dir == get_handle(&self.articles_dir)? {
                Err(anyhow!(
                    "`output_dir` and `articles_dir` point to the same location"
                ))
            } else if output_dir == get_handle(&self.body_dir)? {
                Err(anyhow!(
                    "`output_dir` and `body_dir` point to the same location"
                ))
            } else {
                Ok(())
            }
        }
    }
}

fn get_handle<P: AsRef<Path>>(path: P) -> Result<Handle> {
    Handle::from_path(&path)
        .with_context(|| format!("failed to open directory at {:?}", path.as_ref()))
}

#[cfg_attr(test, derive(Debug, PartialEq))]
#[derive(Deserialize)]
pub struct Frontmatter {
    pub title: String,
    pub slug: String,
    #[serde(deserialize_with = "deserialize_date")]
    pub created: Date,
    #[serde(default, deserialize_with = "deserialize_optional_date")]
    pub updated: Option<Date>,
}

impl Frontmatter {
    /// Parses YAML-style frontmatter from the text content of an article in Markdown format.
    ///
    /// # Errors
    /// This function returns an error if:
    /// - no frontmatter is found in the text
    /// - frontmatter cannot be parsed due to invalid syntax, missing fields, invalid field values, etc.
    /// - the parsed last-updated date is before the parsed creation date
    pub fn from_text(input: &str) -> Result<Self> {
        let matter: Frontmatter = Matter::<YAML>::new()
            .parse(input)
            .data
            .ok_or_else(|| anyhow!("article frontmatter not found"))?
            .deserialize()
            .context("failed to parse article frontmatter")?;

        if matter.updated.is_some_and(|date| date < matter.created) {
            Err(anyhow!(
                "last-updated date precedes creation date of article"
            ))
        } else {
            Ok(matter)
        }
    }
}

fn deserialize_date<'de, D>(deserializer: D) -> Result<Date, D::Error>
where
    D: Deserializer<'de>,
{
    let raw: String = Deserialize::deserialize(deserializer)?;

    raw.parse().map_err(|_| {
        DeError::invalid_value(Unexpected::Str(&raw), &"Expected a date in string form")
    })
}

fn deserialize_optional_date<'de, D>(deserializer: D) -> Result<Option<Date>, D::Error>
where
    D: Deserializer<'de>,
{
    let raw: Option<String> = Deserialize::deserialize(deserializer)?;

    match raw {
        Some(raw) => Ok(Some(raw.parse().map_err(|_| {
            DeError::invalid_value(Unexpected::Str(&raw), &"Expected a date in string form")
        })?)),
        None => Ok(None),
    }
}

/// Parses the input string as Markdown, returning an iterator of Markdown parsing events.
/// The parser recognizes the following extensions to the CommonMark standard:
/// - strikethroughs
/// - YAML-style frontmatter
/// - math markup
pub fn parse_markdown(text: &str) -> impl Iterator<Item = Event<'_>> {
    TextMergeStream::new(Parser::new_ext(
        text,
        Options::ENABLE_STRIKETHROUGH
            | Options::ENABLE_YAML_STYLE_METADATA_BLOCKS
            | Options::ENABLE_MATH,
    ))
}

pub struct LatexConverter {
    context: Context,
}

#[derive(Clone, Copy)]
pub enum RenderMode {
    Inline,
    Display,
}

impl LatexConverter {
    /// Initializes a utility to convert LaTeX source code into HTML.
    /// The current implementation works by running the KaTeX library in a QuickJS runtime via the `rquickjs` crate.
    ///
    /// # Errors
    /// This function returns an error if:
    /// - initializating the JavaScript runtime fails
    /// - evaluating the KaTeX source code fails
    pub fn new() -> Result<Self> {
        let runtime = Runtime::new().context("failed to initialize JS runtime")?;
        let context = Context::full(&runtime).context("failed to initialize JS runtime context")?;

        // When using KaTeX normally (i.e. in a browser or a runtime like Node.js),
        // importing the library makes the JavaScript runtime evaluate the KaTeX source code.
        // Essentially, we perform the same process here,
        // and items exported by KaTeX will be in a object named `katex` with global context.
        context
            .with(|ctx| {
                ctx.eval::<(), _>(KATEX_SRC)
                    .context("failed to evaluate `katex` source code")
            })
            .context("failed to initialize `katex`")?;

        Ok(Self { context })
    }

    /// Converts a string of LaTeX into a string of HTML.
    /// The output HTML uses CSS classes from KaTeX.
    /// The CSS file that comes with KaTeX distributions contains rules for these classes;
    /// it should be used for math to display properly.
    ///
    /// # Errors
    /// This function returns an error if
    /// - the rendering settings cannot be initialized
    /// - the `katex.renderToString()` function cannot be found
    /// - the `katex.renderToString()` function fails to run (e.g. due to invalid LaTeX)
    pub fn latex_to_html(&self, src: &str, mode: RenderMode) -> Result<String> {
        self.context.with(|ctx| {
            // `katex.renderToString()` accepts an object of options.
            // The `displayMode` option controls whether the input string will be rendered in display or inline mode.
            // Source: https://katex.org/docs/options
            let settings =
                Object::new(ctx.clone()).context("failed to initialize `katex` settings")?;
            settings
                .set(
                    "displayMode",
                    match mode {
                        RenderMode::Inline => false,
                        RenderMode::Display => true,
                    },
                )
                .context("failed to initialize `katex` settings")?;

            // To call `katex.renderToString()`, we have to get the function from global context.
            ctx.globals()
                .get::<_, Object<'_>>("katex")
                .context("failed to find the namespace `katex`")?
                .get::<_, Function<'_>>("renderToString")
                .context("failed to find the function `katex.renderToString()`")?
                .call((src, settings))
                .map_err(|e| {
                    let mut err = Error::new(e);
                    // Add exceptions raised by QuickJS to the error chain
                    if let Some(msg) = ctx.catch().as_exception().and_then(Exception::message) {
                        err = err.context(msg);
                    }
                    err.context("failed to run `katex.renderToString()`")
                })
        })
    }
}

pub struct SyntaxHighlighter {
    syntaxes: SyntaxSet,
    theme: Theme,
}

impl SyntaxHighlighter {
    /// Initializes a utility to add syntax highlighting to code.
    /// The current implementation uses the `syntect` crate.
    ///
    /// # Panics
    /// This function panics if the default theme set of `syntect` does not contain "base16-ocean.light".
    #[must_use]
    pub fn new() -> Self {
        let syntaxes = SyntaxSet::load_defaults_newlines();

        // To obtain an owned `Theme`, we call `BTreeMap::remove()` instead of `BTreeMap::get()`.
        // This is fine because we don't care about the entire `ThemeSet`.
        // (Anyway, if we needed the entire `ThemeSet`, we could just call `ThemeSet::load_defaults()` again.)
        let theme = ThemeSet::load_defaults()
            .themes
            .remove("base16-ocean.light")
            .expect("default theme set should include \"base16-ocean.light\"");

        Self { syntaxes, theme }
    }

    /// Adds syntax highlighting to a string of code, outputting HTML with inline styles.
    /// If no language is specified or no syntax for the specified language is found,
    /// the input string is highlighted as plaintext.
    ///
    /// # Errors
    /// This function returns an error if `syntect` fails to highlight the provided text.
    pub fn highlight(&self, text: &str, language: Option<&str>) -> Result<String> {
        let syntax = match language {
            Some(lang) => self.syntaxes.find_syntax_by_token(lang).ok_or_else(|| {
                anyhow!("no syntax could be found for the provided language \"{lang}\"")
            })?,
            None => self.syntaxes.find_syntax_plain_text(),
        };

        highlighted_html_for_string(text, &self.syntaxes, syntax, &self.theme).map_err(Into::into)
    }
}

/// Processes an image link by converting the linked image to AVIF and saving it to an output path.
/// This function outputs a string containing an HTML <img> element
/// with `src`, `alt`, dimension, and rendering attributes.
///
/// # Errors
/// This function returns an error if:
/// - the input image path is empty
/// - the input image path is not normalized or relative
/// - the file at the input image path cannot be opened or read from
/// - the file at the output file path cannot be created or written to
pub fn process_image(
    input_article_dir: &Path,
    output_article_dir: &Path,
    image_path: &str,
    alt_text: &str,
    id: Option<&str>,
) -> Result<String> {
    if image_path.is_empty() {
        return Err(anyhow!("no source provided for image"));
    }
    if !Path::new(image_path).is_relative()
        || Path::new(image_path)
            .components()
            .any(|part| part.as_os_str() == "..")
    {
        return Err(anyhow!(
            "image source is not a normalized relative file path ({image_path})"
        ));
    }

    let input_path = input_article_dir.join(image_path);
    let output_path = output_article_dir.join(image_path).with_extension("avif");

    let image = ImageReader::open(&input_path)
        .with_context(|| format!("failed to open file at {input_path:?}"))?
        .decode()
        .with_context(|| format!("failed to read image from {input_path:?}"))?;

    let (width, height) = image.dimensions();

    // If the input image path ends with ".avif",
    // we assume it is already encoded in AVIF and simply copy it to the output destination.
    if input_path.extension().is_some_and(|ext| ext == "avif") {
        copy(&input_path, &output_path).with_context(|| {
            format!("failed to copy file from {input_path:?} to {output_path:?}")
        })?;
    } else {
        let writer = BufWriter::new(
            File::create(&output_path)
                .with_context(|| format!("failed to create file at {output_path:?}"))?,
        );
        // We use the slowest encoding speed for the best compression.
        AvifEncoder::new_with_speed_quality(writer, 1, 80)
            .write_image(image.as_bytes(), width, height, image.color().into())
            .with_context(|| format!("failed to write image to {output_path:?}"))?;
    }

    Ok(match id {
        Some(id) => format!("<img src=\"{image_path}\" alt=\"{alt_text}\" width=\"{width}\" height=\"{height}\" decoding=\"async\" loading=\"lazy\" id=\"{id}\">"),
        None => format!("<img src=\"{image_path}\" alt=\"{alt_text}\" width=\"{width}\" height=\"{height}\" decoding=\"async\" loading=\"lazy\">")
    })
}

pub struct PageBuilder {
    body: Tree<Node>,
}

impl PageBuilder {
    /// Initializes the webpage HTML builder, parsing an input string as a HTML `<body>`.
    ///
    /// # Errors
    /// This function returns an error if the input string could not be successfully parsed as no-quirks HTML.
    pub fn new(body: &str) -> Result<Self> {
        let body = Html::parse_fragment(body);

        match body.errors.first() {
            Some(err) => Err(Error::msg(err.clone())
                .context("encountered errors when parsing page body as HTML")),
            None => Ok(Self { body: body.tree }),
        }
    }

    /// Consumes the webpage HTML builder, outputting a string containing a complete HTML document.
    /// The parameters determine the contents of various metadata tags in the HTML `<head>` element.
    #[must_use]
    pub fn build_page(self, title: &str, author: &str) -> String {
        let mut html = Html::new_document();
        let mut root = html.tree.root_mut();

        root.append(Node::Doctype(Doctype {
            name: "html".into(),
            public_id: "".into(),
            system_id: "".into(),
        }));

        let html_element = create_el_with_attrs("html", vec![("lang", "en")]);

        let mut html_element_node = root.append(html_element);

        html_element_node.append_subtree(tree! {
            create_el("head") => {
                create_el_with_attrs("meta", vec![("charset", "utf-8")]),
                create_el_with_attrs("meta", vec![("name", "viewport"), ("content", "width=device-width, initial-scale=1")]),
                create_el_with_attrs("meta", vec![("name", "author"), ("content", author)]),
                create_el("title") => { create_text(title) },
                create_el_with_attrs("link", vec![("rel", "stylesheet"), ("href", OUTPUT_SITE_CSS_FILE)])
            }
        });

        let mut body_element_node = html_element_node.append(create_el("body"));

        body_element_node.append_subtree(self.body);

        html.html()
    }
}

fn create_el(name: &str) -> Node {
    Node::Element(Element::new(create_name(name), vec![]))
}

fn create_el_with_attrs<'a, I>(name: &str, attrs: I) -> Node
where
    I: IntoIterator,
    I::IntoIter: Iterator<Item = (&'a str, &'a str)>,
{
    let attrs = attrs
        .into_iter()
        .map(|(key, value)| Attribute {
            name: create_name(key),
            value: value.into(),
        })
        .collect();

    Node::Element(Element::new(create_name(name), attrs))
}

fn create_name(name: &str) -> QualName {
    QualName {
        prefix: None,
        ns: ns!(html),
        local: LocalName::try_static(name)
            .expect("calls to this function should supply valid names"),
    }
}

fn create_text(text: &str) -> Node {
    Node::Text(Text { text: text.into() })
}

#[cfg(test)]
mod test {
    use crate::{Frontmatter, LatexConverter, RenderMode, SyntaxHighlighter};
    use jiff::civil::date;

    #[test]
    fn frontmatter() {
        const BAD_1: &str = "abc123";
        const BAD_2: &str = "---\ntitle: abc\n---";
        const BAD_3: &str = "---\ntitle: abc\nslug: def\ncreated: 123xyz\nupdated: 123xyz\n---";
        const BAD_4: &str = "---\ntitle: \nslug: \ncreated: 2000-01-01\n---";
        const BAD_5: &str = "---\ntitle: abc\nslug: def\ncreated: 2000-02-30\n---";
        const BAD_6: &str =
            "---\ntitle: abc\nslug: def\ncreated: 2000-01-01\nupdated: 1900-01-01\n---";
        const BAD_7: &str =
            "---\ntitle: abc\nslug: def\ncreated: 2000-01-01T00:00Z\nupdated: 2000-01-01T00:00-01:00\n---";

        const GOOD_1: &str = "---\ntitle: abc\nslug: def\ncreated: 2000-01-01\n---";
        const GOOD_2: &str =
            "---\ntitle: abc\nslug: def\ncreated: 2000-01-01\nupdated: 2000-01-01\n---";
        const GOOD_3: &str =
            "---\ntitle: abc\nslug: def\ncreated: 2000-01-01\nupdated: 2000-01-02\n---";
        const GOOD_4: &str =
            "---\ntitle: abc\nslug: def\ncreated: 2000-01-01T01:00\nupdated: 2000-01-01T00:00\n---";

        assert!(
            Frontmatter::from_text(BAD_1).is_err(),
            "parsing should fail if frontmatter is absent"
        );
        assert!(
            Frontmatter::from_text(BAD_2).is_err(),
            "parsing should fail if not all frontmatter fields are present"
        );
        assert!(
            Frontmatter::from_text(BAD_3).is_err(),
            "parsing should fail if date fields are invalid"
        );
        assert!(
            Frontmatter::from_text(BAD_4).is_err(),
            "parsing should fail if title or slug are empty"
        );
        assert!(
            Frontmatter::from_text(BAD_5).is_err(),
            "parsing should fail if a date is invalid"
        );
        assert!(
            Frontmatter::from_text(BAD_6).is_err(),
            "parsing should fail if the last-updated date precedes the creation date"
        );
        assert!(
            Frontmatter::from_text(BAD_7).is_err(),
            "timezone parsing is not supported"
        );
        assert_eq!(
            Frontmatter::from_text(GOOD_1).expect("parsing should succeed"),
            Frontmatter {
                title: String::from("abc"),
                slug: String::from("def"),
                created: date(2000, 1, 1),
                updated: None
            }
        );
        assert_eq!(
            Frontmatter::from_text(GOOD_2).expect("parsing should succeed"),
            Frontmatter {
                title: String::from("abc"),
                slug: String::from("def"),
                created: date(2000, 1, 1),
                updated: Some(date(2000, 1, 1))
            }
        );
        assert!(
            Frontmatter::from_text(GOOD_3).is_ok(),
            "parsing should succeed"
        );
        assert_eq!(
            Frontmatter::from_text(GOOD_4).expect("parsing should succeed due to ignoring dates"),
            Frontmatter {
                title: String::from("abc"),
                slug: String::from("def"),
                created: date(2000, 1, 1),
                updated: Some(date(2000, 1, 1))
            }
        );
    }

    #[test]
    fn latex_to_html() {
        let converter = LatexConverter::new().expect("engine initialization should succeed");

        let inline_html = converter
            .latex_to_html("2x+3y=4z", RenderMode::Inline)
            .expect("inline LaTeX conversion should succeed");

        let display_html = converter
            .latex_to_html("2x+3y=4z", RenderMode::Display)
            .expect("display LaTeX conversion should succeed");

        assert_ne!(
            inline_html, display_html,
            "inline LaTeX and display LaTeX should yield different outputs"
        );

        assert!(
            converter
                .latex_to_html("\\frac{", RenderMode::Inline)
                .is_err(),
            "conversion should fail on invalid LaTeX"
        );
    }

    #[test]
    fn syntax_highlighting() {
        let highlighter = SyntaxHighlighter::new();

        assert!(
            highlighter.highlight("abc123", None).is_ok(),
            "plaintext highlighting should succeed"
        );
        assert!(
            highlighter
                .highlight("const FOO: usize = 42;", Some("rs"))
                .is_ok(),
            "extension-based syntax detection and highlighting should succeed"
        );
        assert!(
            highlighter
                .highlight("const FOO: usize = 42;", Some("rust"))
                .is_ok(),
            "name-based syntax detection and highlighting should succeed"
        );
        assert!(
            highlighter
                .highlight("constant foo u0 = \"abc", Some("rust"))
                .is_ok(),
            "highlighting should succeed for invalid syntax"
        );
        assert!(
            highlighter.highlight("", Some("klingon")).is_err(),
            "syntax detection for non-existent language should fail"
        );
    }
}
