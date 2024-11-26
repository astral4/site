//! Code for CSS minification and font dependency analysis.

use anyhow::{Context, Result};
use lightningcss::{
    error::Error,
    printer::PrinterOptions,
    rules::{
        font_face::{FontFaceProperty, FontFormat, Source},
        CssRule,
    },
    stylesheet::{MinifyOptions, ParserFlags, ParserOptions, StyleSheet},
    targets::{Browsers, Features, Targets},
};
use std::collections::HashSet;

/// Parses the input string as CSS. This function returns:
/// - minified CSS compatible with a set of "reasonable" target browser versions
/// - a list of font dependencies (highest-priority sources only)
///
/// # Errors
/// This function returns an error if:
/// - the input string cannot be successfully parsed as CSS
/// - parsed stylesheet cannot be serialized to a string
/// - parsed stylesheet cannot be minified for the target browser versions
///
/// # Panics
/// This function panics if:
/// - querying for the default set of target browser versions returns an error
/// - the default set of target browser versions does not exist
pub fn transform_css(source: &str) -> Result<CssOutput> {
    // Parse input as CSS
    let mut stylesheet = StyleSheet::parse(
        source,
        ParserOptions {
            // The source file path will be included higher in the error chain
            filename: String::new(),
            css_modules: None,
            source_index: Default::default(),
            // The CSS parser should error instead of reporting success while emitting warnings
            error_recovery: false,
            warnings: None,
            // Support CSS nesting parsing
            flags: ParserFlags::NESTING,
        },
    )
    .map_err(Error::into_owned)
    .context("failed to parse input as valid CSS")?;

    // Determine target browser versions for stylesheet compilation
    let targets = Targets {
        browsers: Some(
            Browsers::from_browserslist(["defaults"])
                .expect("query for browserslist defaults should succeed")
                .expect("browser targets should exist"),
        ),
        include: Features::empty(),
        exclude: Features::empty(),
    };

    // Minify stylesheet based on target browser versions
    stylesheet
        .minify(MinifyOptions {
            targets,
            unused_symbols: HashSet::default(),
        })
        .context("failed to minify CSS")?;

    // Serialize stylesheet to string
    let css = stylesheet
        .to_css(PrinterOptions {
            // Remove whitespace
            minify: true,
            project_root: None,
            targets,
            analyze_dependencies: None,
            pseudo_classes: None,
        })
        .context("failed to serialize minified CSS")?
        .code;

    // Find the highest-priority source for each font used by the stylesheet
    let top_fonts = stylesheet
        .rules
        .0
        .into_iter()
        .filter_map(|rule| match rule {
            CssRule::FontFace(font_rule) => Some(font_rule.properties),
            _ => None,
        })
        .flatten()
        .filter_map(|property| match property {
            FontFaceProperty::Source(sources) => Some(sources),
            _ => None,
        })
        .filter_map(|mut sources| (!sources.is_empty()).then(|| sources.swap_remove(0))) // Gets the first element in owned form
        .filter_map(|src| match src {
            Source::Url(url_src) => Some(url_src),
            Source::Local(_) => None,
        })
        .map(|src| Font {
            path: Box::from(&*src.url.url),
            mime: src.format.and_then(|format| match format {
                FontFormat::WOFF2 => Some("font/woff2"),
                FontFormat::WOFF => Some("font/woff"),
                FontFormat::TrueType => Some("font/ttf"),
                FontFormat::OpenType => Some("font/otf"),
                FontFormat::SVG => Some("image/svg+xml"),
                _ => None,
            }),
        })
        .collect();

    Ok(CssOutput { css, top_fonts })
}

#[cfg_attr(test, derive(Debug, PartialEq))]
pub struct CssOutput {
    pub css: String,
    pub top_fonts: Vec<Font>,
}

#[cfg_attr(test, derive(Debug, PartialEq))]
pub struct Font {
    pub path: Box<str>,
    pub mime: Option<&'static str>,
}

#[cfg(test)]
mod test {
    use super::{transform_css, CssOutput, Font};

    #[test]
    fn no_fonts() {
        assert_eq!(
            transform_css("p { font-size: 1em }").expect("CSS transformation should succeed"),
            CssOutput {
                css: "p{font-size:1em}".into(),
                top_fonts: vec![]
            }
        );
    }

    #[test]
    fn one_font() {
        assert_eq!(
            transform_css("@font-face { src: url('foo.bin') format('woff2'); }")
                .expect("CSS transformation should succeed"),
            CssOutput {
                css: "@font-face{src:url(foo.bin)format(\"woff2\")}".into(),
                top_fonts: vec![Font {
                    path: "foo.bin".into(),
                    mime: Some("font/woff2")
                }]
            }
        );
    }

    #[test]
    fn multiple_fonts() {
        assert_eq!(
            transform_css("@font-face { src: url('foo.bin') format('woff'), url('bar.bin') format('ttf'); } @font-face { src: url('baz.bin'); }")
                .expect("CSS transformation should succeed"),
            CssOutput {
                css: "@font-face{src:url(foo.bin)format(\"woff\"),url(bar.bin)format(\"ttf\")}@font-face{src:url(baz.bin)}".into(),
                top_fonts: vec![Font {
                    path: "foo.bin".into(),
                    mime: Some("font/woff")
                }, Font {
                    path: "baz.bin".into(),
                    mime: None
                }]
            }
        );
    }
}
