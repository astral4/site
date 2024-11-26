//! Code for reading app configuration from a TOML file. The configuration file path is supplied via the command line.

use anyhow::{anyhow, Context, Result};
use foldhash::{HashSet, HashSetExt};
use same_file::is_same_file;
use serde::Deserialize;
use std::{env::args, fs::read_to_string, path::Path};
use toml_edit::de::from_str as toml_from_str;

#[derive(Deserialize)]
pub struct Config {
    // Your full name
    pub name: Box<str>,
    // Path to directory for generated site output
    pub output_dir: Box<Path>,
    // Path to site-wide CSS file
    pub site_css_file: Box<Path>,
    // List of titles and paths for all webpage fragment files;
    // for non-article pages like the site index and the "about" page
    pub fragments: Box<[Fragment]>,
    // Path to directory of all articles
    pub articles_dir: Box<Path>,
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
    /// - the output directory path and another path in the config point to the same location
    pub fn from_env() -> Result<Self> {
        let mut args = args().skip(1);

        let config_path = args
            .next()
            .ok_or_else(|| anyhow!("configuration file path was not provided"))?;

        if args.next().is_some() {
            return Err(anyhow!("too many input arguments were provided"));
        }

        let config: Self = toml_from_str(
            &read_to_string(&config_path)
                .with_context(|| format!("failed to read configuration from {config_path}"))?,
        )
        .context("failed to parse configuration file")?;

        config
            .check_paths()
            .context("configuration file is invalid")?;

        Ok(config)
    }

    fn check_paths(&self) -> Result<()> {
        let mut fragment_paths = HashSet::with_capacity(self.fragments.len());

        for fragment in &self.fragments {
            if !fragment.path.is_file() {
                return Err(anyhow!(
                    "`fragments`: {:?} does not point to a file",
                    fragment.path
                ));
            }
            if !fragment_paths.insert(&fragment.path) {
                return Err(anyhow!("`fragments`: duplicate fragment paths found"));
            }
        }

        if !self.articles_dir.is_dir() {
            Err(anyhow!(
                "`articles_dir`: {:?} does not point to a directory",
                self.articles_dir
            ))
        } else if !self.site_css_file.is_file() {
            Err(anyhow!(
                "`site_css_file`: {:?} does not point to a file",
                self.site_css_file
            ))
        } else if is_same_file(&self.output_dir, &self.articles_dir).unwrap() {
            Err(anyhow!(
                "`output_dir` and `articles_dir` point to the same location"
            ))
        } else {
            Ok(())
        }
    }
}
