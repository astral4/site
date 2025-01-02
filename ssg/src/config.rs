//! Code for reading the app config from a TOML file. The config file path is supplied via the command line.

use crate::highlight::THEME_NAMES;
use anyhow::{anyhow, Context, Result};
use camino::Utf8Path;
use foldhash::{HashSet, HashSetExt};
use same_file::Handle;
use serde::Deserialize;
use std::{env::args, ffi::OsStr, fs::read_to_string, path::Path};
use toml_edit::de::from_str as toml_from_str;

macro_rules! transform_paths {
    ($config:expr, $base_path:expr, [$( $field_path:ident: $path_ty:ty ),*]) => {
        $(
            $config.$field_path = <$path_ty>::new($base_path)
                .parent()
                // We expect the parent to exist because otherwise
                // the config path does not point to a file and cannot be read from
                .expect("config file path should have parent")
                .join(&$config.$field_path)
                .into_boxed_path();
        )*
    };
}

#[derive(Deserialize)]
pub struct Config {
    // Name of the website author
    pub author: Box<str>,
    // Path to directory for generated site output
    pub output_dir: Box<Path>,
    // Path to site-wide CSS file
    pub site_css_file: Box<Path>,
    // Path to site-wide template HTML file
    pub template_html_file: Box<Path>,
    // List of titles and paths for all webpage fragment files;
    // for non-article pages like the site index and the "about" page
    pub fragments: Box<[Fragment]>,
    // Name of theme for code syntax highlighting
    pub theme: Box<str>,
    // Path to directory containing all articles
    pub articles_dir: Box<Utf8Path>,
}

#[derive(Deserialize)]
pub struct Fragment {
    pub title: Box<str>,
    pub path: Box<Path>,
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

        let config_path = args
            .next()
            .ok_or_else(|| anyhow!("configuration file path was not provided"))?;

        if args.next().is_some() {
            return Err(anyhow!("too many input arguments were provided"));
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
            [output_dir: Path, site_css_file: Path, template_html_file: Path, articles_dir: Utf8Path]
        );
        for fragment in &mut config.fragments {
            transform_paths!(fragment, &config_path, [path: Path]);
        }

        // Validate config settings
        config.validate().context("configuration file is invalid")?;

        Ok(config)
    }

    fn validate(&self) -> Result<()> {
        if !THEME_NAMES.contains(&self.theme) {
            Err(anyhow!("`theme`: {} is an invalid theme name", self.theme))
        } else if self.output_dir.is_dir() {
            Err(anyhow!(
                "`output_dir`: {:?} already exists as a directory",
                self.output_dir
            ))
        } else if !self.articles_dir.is_dir() {
            Err(anyhow!(
                "`articles_dir`: {:?} could not be opened or does not point to a directory",
                self.articles_dir
            ))
        } else if !self.site_css_file.is_file() {
            Err(anyhow!(
                "`site_css_file`: {:?} could not be opened or does not point to a file",
                self.site_css_file
            ))
        } else if !self.template_html_file.is_file() {
            Err(anyhow!(
                "`template_html_file`: {:?} could not be opened or does not point to a file",
                self.template_html_file
            ))
        } else {
            let mut fragment_paths = HashSet::with_capacity(self.fragments.len());

            for fragment in &self.fragments {
                if fragment.path.file_stem().is_none_or(OsStr::is_empty) {
                    return Err(anyhow!("`fragments`: empty file name found"));
                }

                let handle = Handle::from_path(&fragment.path).with_context(|| {
                    format!(
                        "`fragments`: {:?} could not be opened or does not point to a file",
                        fragment.path
                    )
                })?;

                if !fragment_paths.insert(handle) {
                    return Err(anyhow!(
                        "`fragments`: found multiple fragment paths pointing to the same file"
                    ));
                }
            }

            Ok(())
        }
    }
}
