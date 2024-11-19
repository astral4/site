use anyhow::{anyhow, Context as _, Error, Result};
use rquickjs::{Context, Exception, Function, Object, Runtime};
use syntect::{
    highlighting::{Theme, ThemeSet},
    html::highlighted_html_for_string,
    parsing::SyntaxSet,
};

const KATEX_SRC: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/katex/katex.min.js"));

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
                .context("failed to find `katex` namespace")?
                .get::<_, Function<'_>>("renderToString")
                .context("failed to find `katex.renderToString()`")?
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
            .remove("base16-ocean.light") // we call `BTreeMap::remove()` instead of `BTreeMap::get()` to obtain an owned `Theme`
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
    }

    #[should_panic = "conversion should fail on invalid LaTeX"]
    #[test]
    fn invalid_latex() {
        let converter = LatexConverter::new().expect("engine initialization should succeed");

        converter
            .latex_to_html("\\frac{", RenderMode::Inline)
            .expect("conversion should fail on invalid LaTeX");
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
