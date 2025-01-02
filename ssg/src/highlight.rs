//! Utility for highlighting code in articles by converting Markdown code blocks to styled HTML.

use anyhow::{anyhow, Result};
use syntect::{
    highlighting::{FontStyle, Style, Theme, ThemeSet, ThemeSettings},
    html::{highlighted_html_for_string, styled_line_to_highlighted_html, IncludeBackground},
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

    /// Adds syntax highlighting to a code block, outputting HTML with inline styles.
    /// If no language is provided, the input string is highlighted as plaintext.
    ///
    /// # Errors
    /// This function returns an error if:
    /// - no syntax can be found for the provided language
    /// - `syntect` fails to highlight the provided text
    pub fn highlight_block(&self, text: &str, language: Option<&str>) -> Result<String> {
        let syntax = match language {
            Some(lang) if !lang.is_empty() => {
                self.syntaxes.find_syntax_by_token(lang).ok_or_else(|| {
                    anyhow!("no syntax could be found for the provided language \"{lang}\"")
                })?
            }
            _ => self.syntaxes.find_syntax_plain_text(),
        };

        highlighted_html_for_string(text, &self.syntaxes, syntax, &self.theme).map_err(Into::into)
    }

    /// Adds plaintext highlighting to an inline code segment, outputting HTML with inline styles.
    ///
    /// # Errors
    /// This function returns an error if `syntect` fails to highlight the provided text.
    ///
    /// # Panics
    /// This function panics if the "base16-ocean.light" theme does not contain default text and background colors.
    pub fn highlight_segment(&self, text: &str) -> Result<String> {
        let ThemeSettings {
            foreground: Some(foreground),
            background: Some(background),
            ..
        } = self.theme.settings
        else {
            panic!("\"base16-ocean.light\" should contain default text and background colors");
        };

        let style = Style {
            foreground,
            background,
            font_style: FontStyle::empty(),
        };

        Ok(format!(
            "<code>{}</code>",
            styled_line_to_highlighted_html(&[(style, text)], IncludeBackground::Yes)?
        ))
    }
}

#[cfg(test)]
mod test {
    use super::SyntaxHighlighter;

    #[test]
    fn plaintext() {
        let highlighter = SyntaxHighlighter::new();

        highlighter
            .highlight_segment("abc123")
            .expect("highlighting should succeed");

        highlighter
            .highlight_block("abc123", None)
            .expect("highlighting should succeed");
    }

    #[test]
    fn extension_based_syntax_detection() {
        SyntaxHighlighter::new()
            .highlight_block("const FOO: usize = 42;", Some("rs"))
            .expect("highlighting should succeed");
    }

    #[test]
    fn name_based_syntax_detection() {
        SyntaxHighlighter::new()
            .highlight_block("const FOO: usize = 42;", Some("rust"))
            .expect("highlighting should succeed");
    }

    #[test]
    fn invalid_syntax() {
        SyntaxHighlighter::new()
            .highlight_block("constant foo u0 = \"abc", Some("rust"))
            .expect("highlighting should succeed");
    }

    #[test]
    fn nonexistent_language() {
        assert!(
            SyntaxHighlighter::new()
                .highlight_block("", Some("klingon"))
                .is_err(),
            "syntax detection for non-existent language should fail"
        );
    }
}
