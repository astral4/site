use anyhow::{anyhow, Context as _, Error, Result};
use gray_matter::{engine::YAML, Matter};
use image::{codecs::avif::AvifEncoder, GenericImageView, ImageEncoder, ImageReader};
use jiff::civil::Date;
use rquickjs::{Context, Exception, Function, Object, Runtime};
use serde::{
    de::{Error as DeError, Unexpected},
    Deserialize, Deserializer,
};
use std::{
    env::args,
    fs::{copy, File},
    io::BufWriter,
    path::{Path, PathBuf},
};
use syntect::{
    highlighting::{Theme, ThemeSet},
    html::highlighted_html_for_string,
    parsing::SyntaxSet,
};

const KATEX_SRC: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../katex/katex.js"));

pub struct Input {
    pub content_dir: PathBuf,
    pub output_dir: PathBuf,
}

/// Reads the articles directory and output directory from command-line arguments.
/// # Errors
/// This function returns an error if:
/// - not enough arguments were provided
/// - too many arguments were provided
/// - an argument parsed as a directory path does not point to a directory
pub fn read_input() -> Result<Input> {
    let mut args = args();

    let content_dir: PathBuf = args
        .next()
        .ok_or_else(|| anyhow!("articles directory path was not provided"))?
        .into();

    if !content_dir.is_dir() {
        return Err(anyhow!(
            "articles directory path does not point to a directory"
        ));
    }

    let output_dir: PathBuf = args
        .next()
        .ok_or_else(|| anyhow!("output directory path was not provided"))?
        .into();

    if !output_dir.is_dir() {
        return Err(anyhow!(
            "output directory path does not point to a directory"
        ));
    }

    if args.next().is_some() {
        return Err(anyhow!("too many input arguments were supplied"));
    }

    Ok(Input {
        content_dir,
        output_dir,
    })
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
    /// # Errors
    /// This function returns an error if:
    /// - No frontmatter was found in the text
    /// - Frontmatter could not be parsed due to invalid syntax, missing fields, invalid field values, etc.
    /// - The parsed last-updated date is before the parsed creation date
    pub fn from_text(input: &str) -> Result<Self> {
        let matter: Frontmatter = Matter::<YAML>::new()
            .parse(input)
            .data
            .ok_or(anyhow!("article frontmatter not found"))?
            .deserialize()
            .context("failed to parse article frontmatter")?;

        if let Some(date) = matter.updated {
            println!("{input}: {}", matter.created - date);
        }

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

pub struct LatexConverter {
    context: Context,
}

#[derive(Clone, Copy)]
pub enum RenderMode {
    Inline,
    Display,
}

impl LatexConverter {
    /// # Errors
    /// This function returns an error for:
    /// - failed initialization of the underlying JavaScript runtime from `rquickjs`
    /// - failed evaluation of the embedded `katex` source code
    pub fn new() -> Result<Self> {
        let runtime = Runtime::new().context("failed to initialize JS runtime")?;
        let context = Context::full(&runtime).context("failed to initialize JS runtime context")?;

        context
            .with(|ctx| {
                ctx.eval::<(), _>(KATEX_SRC)
                    .context("failed to evaluate `katex` source code")
            })
            .context("failed to initialize `katex`")?;

        Ok(Self { context })
    }

    /// # Errors
    /// This function returns an error if
    /// - the rendering settings could not be initialized
    /// - the `katex.renderToString()` function could not be found
    /// - the `katex.renderToString()` function failed to run (e.g. due to invalid LaTeX)
    pub fn latex_to_html(&self, src: &str, mode: RenderMode) -> Result<String> {
        self.context.with(|ctx| {
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

            ctx.globals()
                .get::<_, Object<'_>>("katex")
                .context("failed to find the namespace `katex`")?
                .get::<_, Function<'_>>("renderToString")
                .context("failed to find the function `katex.renderToString()`")?
                .call((src, settings))
                .map_err(|e| {
                    let mut err = Error::new(e);
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
    #[must_use]
    /// # Panics
    /// This function panics if the default theme set of `syntect` does not contain "base16-ocean.light".
    pub fn new() -> Self {
        let syntaxes = SyntaxSet::load_defaults_newlines();
        let theme = ThemeSet::load_defaults()
            .themes
            .remove("base16-ocean.light") // to obtain an owned `Theme`, we call `BTreeMap::remove()` instead of `BTreeMap::get()`
            .expect("default theme set should include \"base16-ocean.light\"");

        Self { syntaxes, theme }
    }

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

/// # Errors
/// This function returns an error if:
/// - the input image path is empty
/// - the input image path is not relative
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
    if !Path::new(image_path).is_relative() {
        return Err(anyhow!("image source is not a relative file path"));
    }

    let input_path = input_article_dir.join(image_path);
    let output_path = output_article_dir.join(image_path).with_extension("avif");

    let image = ImageReader::open(&input_path)
        .with_context(|| format!("failed to open file at {input_path:?}"))?
        .decode()
        .with_context(|| format!("failed to read image from {input_path:?}"))?;

    let (width, height) = image.dimensions();

    if input_path.extension().is_some_and(|ext| ext == "avif") {
        copy(&input_path, &output_path).with_context(|| {
            format!("failed to copy file from {input_path:?} to {output_path:?}")
        })?;
    } else {
        let writer = BufWriter::new(
            File::create(&output_path)
                .with_context(|| format!("failed to create file at {output_path:?}"))?,
        );
        AvifEncoder::new_with_speed_quality(writer, 1, 80)
            .write_image(image.as_bytes(), width, height, image.color().into())
            .with_context(|| format!("failed to write image to {output_path:?}"))?;
    }

    let html = if let Some(id) = id {
        format!("<img src=\"{image_path}\" alt=\"{alt_text}\" width=\"{width}\" height=\"{height}\" decoding=\"async\" loading=\"lazy\" id=\"{id}\">")
    } else {
        format!("<img src=\"{image_path}\" alt=\"{alt_text}\" width=\"{width}\" height=\"{height}\" decoding=\"async\" loading=\"lazy\">")
    };

    Ok(html)
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
