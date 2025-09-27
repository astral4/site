//! Code for reading the app config from a TOML file. The config file path is supplied via the command line.

use crate::highlight::THEME_NAMES;
use anyhow::{bail, Context, Result};
use camino::Utf8Path;
use foldhash::{HashSet, HashSetExt};
use same_file::Handle;
use serde::Deserialize;
use std::{env::args, fs::read_to_string};
use toml_edit::de::from_str as toml_from_str;

macro_rules! transform_paths {
    ($config:expr, $base_path:expr, [$( $field_path:ident ),*]) => {
        $(
            $config.$field_path = ::camino::Utf8Path::new($base_path)
                .parent()
                // We expect the parent to exist because otherwise
                // the config path does not point to a file and cannot be read from
                .expect("config file path should have parent")
                .join(&$config.$field_path)
                .into();
        )*
    };
}

#[derive(Deserialize)]
pub struct Config {
    // Path to directory for generated site output
    pub output_dir: Box<Utf8Path>,
    // Path to site-wide CSS file
    pub site_css_file: Box<Utf8Path>,
    // Path to site-wide head template HTML file
    pub head_template_html_file: Box<Utf8Path>,
    // Path to site-wide body template HTML file
    pub body_template_html_file: Box<Utf8Path>,
    // List of titles and paths for all webpage fragment files;
    // for non-article pages like the site index and the "about" page
    pub fragments: Box<[Fragment]>,
    // Path to directory containing all articles
    pub articles_dir: Box<Utf8Path>,
    // Name of theme for code syntax highlighting
    pub code_theme: Box<str>,
}

#[derive(Deserialize)]
pub struct Fragment {
    pub title: Box<str>,
    pub path: Box<Utf8Path>,
}

impl Config {
    /// Reads a config file from a path provided by command-line arguments.
    ///
    /// # Errors
    /// This function returns an error if:
    /// - not enough command-line arguments are provided
    /// - too many command-line arguments are provided
    /// - a config parameter interpreted as a directory path does not point to a directory
    /// - a config parameter interpreted as a file path does not point to a file
    /// - multiple fragment paths point to the same file
    ///
    /// # Panics
    /// This function panics if the provided config file path has no parent.
    pub fn from_env() -> Result<Self> {
        // Get path to config file from command-line arguments
        let mut args = args().skip(1);

        let Some(config_path) = args.next() else {
            bail!("configuration file path was not provided");
        };

        if args.next().is_some() {
            bail!("too many input arguments were provided");
        }

        // Parse config
        let mut config: Self = toml_from_str(
            &read_to_string(&config_path)
                .with_context(|| format!("failed to read configuration from {config_path}"))?,
        )
        .context("failed to parse configuration file")?;

        // Interpret relative paths in the config as relative to the config file's location
        transform_paths!(
            config,
            &config_path,
            [
                output_dir,
                site_css_file,
                head_template_html_file,
                body_template_html_file,
                articles_dir
            ]
        );

        for fragment in &mut config.fragments {
            transform_paths!(fragment, &config_path, [path]);
        }

        // Validate config settings
        config.validate().context("configuration file is invalid")?;

        Ok(config)
    }

    fn validate(&self) -> Result<()> {
        if !THEME_NAMES.contains(&self.code_theme) {
            bail!("`theme`: {} is an invalid theme name", self.code_theme);
        } else if self.output_dir.is_dir() {
            bail!(
                "`output_dir`: {} already exists as a directory",
                self.output_dir
            );
        } else if !self.articles_dir.is_dir() {
            bail!(
                "`articles_dir`: {} could not be opened or does not point to a directory",
                self.articles_dir
            );
        } else if !self.site_css_file.is_file() {
            bail!(
                "`site_css_file`: {} could not be opened or does not point to a file",
                self.site_css_file
            );
        } else if !self.head_template_html_file.is_file() {
            bail!(
                "`head_template_html_file`: {} could not be opened or does not point to a file",
                self.head_template_html_file
            );
        } else if !self.body_template_html_file.is_file() {
            bail!(
                "`body_template_html_file`: {} could not be opened or does not point to a file",
                self.body_template_html_file
            );
        }

        // Validate `fragments` field
        let mut fragment_paths = HashSet::with_capacity(self.fragments.len());

        for fragment in &self.fragments {
            if fragment.path.file_stem().is_none_or(str::is_empty) {
                bail!("`fragments`: empty file name found");
            }

            let handle = Handle::from_path(fragment.path.as_ref()).with_context(|| {
                format!(
                    "`fragments`: {} could not be opened or does not point to a file",
                    fragment.path
                )
            })?;

            if !fragment_paths.insert(handle) {
                bail!("`fragments`: found multiple fragment paths pointing to the same file");
            }
        }

        Ok(())
    }
}
