use anyhow::{Context as _, Error, Result};
use rquickjs::{Context, Exception, Function, Object, Runtime};

const KATEX_SRC: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/katex/katex.min.js"));

pub struct KatexEngine {
    context: Context,
}

#[derive(Clone, Copy)]
pub enum RenderMode {
    Inline,
    Display,
}

impl KatexEngine {
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
    /// - the `katex.renderToString()` function failed to run
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

#[cfg(test)]
mod test {
    use crate::{KatexEngine, RenderMode};

    #[test]
    fn latex_to_html() {
        let engine = KatexEngine::new().expect("engine initialization should succeed");

        let inline_html = engine
            .latex_to_html("2x+3y=4z", RenderMode::Inline)
            .expect("inline LaTeX conversion should succeed");

        let display_html = engine
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
        let engine = KatexEngine::new().expect("engine initialization should succeed");

        engine
            .latex_to_html("\\frac{", RenderMode::Inline)
            .expect("conversion should fail on invalid LaTeX");
    }
}
