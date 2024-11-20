use anyhow::{anyhow, Context as _, Error, Result};
use rquickjs::{Context, Exception, Function, Object, Runtime};
use std::{env::args, path::PathBuf};
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

#[cfg(test)]
mod test {
    use crate::{LatexConverter, RenderMode, SyntaxHighlighter};

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
