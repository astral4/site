//! Utility for highlighting code in articles by converting Markdown code blocks to styled HTML.

use anyhow::{anyhow, Result};
use syntect::{
    highlighting::{Theme, ThemeSet},
    html::highlighted_html_for_string,
    parsing::SyntaxSet,
};

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

#[cfg(test)]
mod test {
    use super::SyntaxHighlighter;

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
