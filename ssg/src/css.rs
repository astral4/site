//! Code for CSS minification and dependency analysis.

use anyhow::{Context, Result};
use lightningcss::{
    dependencies::{Dependency, DependencyOptions},
    error::Error,
    printer::PrinterOptions,
    stylesheet::{MinifyOptions, ParserFlags, ParserOptions, StyleSheet},
    targets::{Browsers, Features, Targets},
};
use std::collections::HashSet;

pub struct CssOutput {
    css: String,
    dependencies: Option<Vec<String>>,
}

/// Parses the input string as CSS. This function returns:
/// - minified CSS compatible with a set of "reasonable" target browser versions
/// - a list of `url()` dependencies if they exist
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

    // Find `url()` dependencies in stylesheet
    let dependencies = stylesheet
        .to_css(PrinterOptions {
            // Opt into dependency analysis
            analyze_dependencies: Some(DependencyOptions::default()),
            ..Default::default()
        })
        .context("failed to serialize CSS for dependency analysis")?
        .dependencies
        .map(|deps| {
            deps.into_iter()
                .filter_map(|dep| match dep {
                    Dependency::Import(_) => None,
                    Dependency::Url(dep) => Some(dep.url),
                })
                .collect()
        });

    // Determine target browser versions for CSS compilation
    let targets = Targets {
        browsers: Some(
            Browsers::from_browserslist(["defaults"])
                .expect("query for browserslist defaults should succeed")
                .expect("browser targets should exist"),
        ),
        include: Features::empty(),
        exclude: Features::empty(),
    };

    // Minify CSS based on target browser versions
    stylesheet
        .minify(MinifyOptions {
            targets,
            unused_symbols: HashSet::default(),
        })
        .context("failed to minify CSS")?;

    // Serialize CSS to string
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

    Ok(CssOutput { css, dependencies })
}
