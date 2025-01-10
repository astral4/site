//! Utility for highlighting code in articles by converting Markdown code blocks to styled HTML.

use anyhow::{anyhow, Result};
use phf::{phf_set, Set};
use std::borrow::Cow;
use syntect::{
    easy::HighlightLines,
    highlighting::{FontStyle, Style, Theme, ThemeSet, ThemeSettings},
    html::{
        append_highlighted_html_for_styled_line, start_highlighted_html_snippet,
        styled_line_to_highlighted_html, IncludeBackground,
    },
    parsing::SyntaxSet,
    util::LinesWithEndings,
};

// Names of themes in the default theme set
// https://docs.rs/syntect/5.2.0/syntect/highlighting/struct.ThemeSet.html#method.load_defaults
pub(crate) const THEME_NAMES: Set<&str> = phf_set! {
    "base16-ocean.dark",
    "base16-eighties.dark",
    "base16-mocha.dark",
    "base16-ocean.light",
    "InspiredGitHub",
    "Solarized (dark)",
    "Solarized (light)",
};

pub struct SyntaxHighlighter {
    syntaxes: SyntaxSet,
    theme: Theme,
}

impl SyntaxHighlighter {
    /// Initializes a utility to add syntax highlighting to code.
    /// Hightlighting styles are based on the input theme.
    /// The current implementation uses the `syntect` crate.
    ///
    /// # Panics
    /// This function panics if the default theme set of `syntect` does not contain the input theme.
    #[must_use]
    pub fn new(theme: &str) -> Self {
        let syntaxes = SyntaxSet::load_defaults_newlines();

        // To obtain an owned `Theme`, we call `BTreeMap::remove()` instead of `BTreeMap::get()`.
        // This is fine because we do not need the entire `ThemeSet` after this.
        // (If we did, we could just call `ThemeSet::load_defaults()` again.)
        let theme = ThemeSet::load_defaults()
            .themes
            .remove(theme)
            .unwrap_or_else(|| panic!("default theme set should include \"{theme}\""));

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
        // Find language syntax
        let syntax = match language {
            Some(lang) if !lang.is_empty() => {
                self.syntaxes.find_syntax_by_token(lang).ok_or_else(|| {
                    anyhow!("no syntax could be found for the provided language \"{lang}\"")
                })?
            }
            _ => self.syntaxes.find_syntax_plain_text(),
        };

        // Highlight line by line
        let mut highlighter = HighlightLines::new(syntax, &self.theme);
        let (mut output, background) = start_highlighted_html_snippet(&self.theme);

        for line in LinesWithEndings::from(text) {
            // Replace starting tabs with spaces (1 tab = 4 spaces)
            let num_starting_whitespace_bytes: usize = line
                .chars()
                .take_while(|c| c.is_whitespace())
                .map(char::len_utf8)
                .sum();

            let line = if num_starting_whitespace_bytes > 0 {
                let (whitespace, remaining) = line.split_at(num_starting_whitespace_bytes);
                let mut line = whitespace.replace('\t', "    ");
                line.reserve_exact(remaining.len());
                line.push_str(remaining);
                Cow::Owned(line)
            } else {
                Cow::Borrowed(line)
            };

            // Highlight line
            let regions = highlighter.highlight_line(&line, &self.syntaxes)?;
            append_highlighted_html_for_styled_line(
                &regions,
                IncludeBackground::IfDifferent(background),
                &mut output,
            )?;
        }

        // Add closing tag; the opening tag was added in `start_highlighted_html_snippet()`
        output.push_str("</pre>");

        Ok(output)
    }

    /// Adds plaintext highlighting to an inline code segment, outputting HTML with inline styles.
    ///
    /// # Errors
    /// This function returns an error if `syntect` fails to highlight the provided text.
    ///
    /// # Panics
    /// This function panics if the selected theme does not contain default text and background colors.
    pub fn highlight_segment(&self, text: &str) -> Result<String> {
        let ThemeSettings {
            foreground: Some(foreground),
            background: Some(background),
            ..
        } = self.theme.settings
        else {
            panic!(
                "\"{}\" should contain default text and background colors",
                self.theme.name.as_deref().unwrap_or("selected theme"),
            );
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
    use super::{SyntaxHighlighter, THEME_NAMES};

    #[test]
    fn plaintext() {
        for theme in &THEME_NAMES {
            let highlighter = SyntaxHighlighter::new(theme);

            highlighter
                .highlight_segment("abc123")
                .expect("highlighting should succeed");

            highlighter
                .highlight_block("abc123", None)
                .expect("highlighting should succeed");
        }
    }

    #[test]
    fn extension_based_syntax_detection() {
        for theme in &THEME_NAMES {
            SyntaxHighlighter::new(theme)
                .highlight_block("const FOO: usize = 42;", Some("rs"))
                .expect("highlighting should succeed");
        }
    }

    #[test]
    fn name_based_syntax_detection() {
        for theme in &THEME_NAMES {
            SyntaxHighlighter::new(theme)
                .highlight_block("const FOO: usize = 42;", Some("rust"))
                .expect("highlighting should succeed");
        }
    }

    #[test]
    fn invalid_syntax() {
        for theme in &THEME_NAMES {
            SyntaxHighlighter::new(theme)
                .highlight_block("constant foo u0 = \"abc", Some("rust"))
                .expect("highlighting should succeed");
        }
    }

    #[test]
    fn nonexistent_language() {
        for theme in &THEME_NAMES {
            assert!(
                SyntaxHighlighter::new(theme)
                    .highlight_block("abc", Some("klingon"))
                    .is_err(),
                "syntax detection for non-existent language should fail"
            );
        }
    }

    #[test]
    fn tabs_to_spaces() {
        const TEXT_SPACES: &str = "
abc
    abc
        abc
     abc
     abc
          abc
";
        const TEXT_TABS: &str = "
abc
\tabc
\t\tabc
\t abc
 \tabc
 \t \tabc
";

        for theme in &THEME_NAMES {
            let highlighter = SyntaxHighlighter::new(theme);

            assert_eq!(
                highlighter
                    .highlight_block(TEXT_SPACES, None)
                    .expect("highlighting should succeed"),
                highlighter
                    .highlight_block(TEXT_TABS, None)
                    .expect("highlighting should succeed"),
            );
        }
    }
}
