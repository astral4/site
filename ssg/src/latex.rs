//! Utility for converting math markup in articles from LaTeX to HTML.

use anyhow::{Context as _, Error, Result};
use rquickjs::{Context, Exception, Function, Object, Runtime};

const KATEX_SRC: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../katex/katex.js"));

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

#[cfg(test)]
mod test {
    use super::{LatexConverter, RenderMode};

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
}
