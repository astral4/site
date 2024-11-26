//! Code for parsing YAML-style frontmatter from articles.

use anyhow::{anyhow, Context, Result};
use gray_matter::{engine::YAML, Matter};
use jiff::civil::Date;
use serde::{
    de::{Error as DeError, Unexpected},
    Deserialize, Deserializer,
};

#[cfg_attr(test, derive(Debug, PartialEq))]
#[derive(Deserialize)]
pub struct Frontmatter {
    pub title: Box<str>,
    pub slug: Box<str>,
    #[serde(deserialize_with = "deserialize_date")]
    pub created: Date,
    #[serde(default, deserialize_with = "deserialize_optional_date")]
    pub updated: Option<Date>,
}

impl Frontmatter {
    /// Parses YAML-style frontmatter from the text content of an article in Markdown format.
    ///
    /// # Errors
    /// This function returns an error if:
    /// - no frontmatter is found in the text
    /// - frontmatter cannot be parsed due to invalid syntax, missing fields, invalid field values, etc.
    /// - the parsed last-updated date is before the parsed creation date
    pub fn from_text(input: &str) -> Result<Self> {
        let matter: Frontmatter = Matter::<YAML>::new()
            .parse(input)
            .data
            .ok_or_else(|| anyhow!("article frontmatter not found"))?
            .deserialize()
            .context("failed to parse article frontmatter")?;

        if matter.updated.is_some_and(|date| date < matter.created) {
            Err(anyhow!(
                "last-updated date precedes creation date of article"
            ))
        } else {
            Ok(matter)
        }
    }
}

fn deserialize_date<'de, D>(deserializer: D) -> Result<Date, D::Error>
where
    D: Deserializer<'de>,
{
    let raw: String = Deserialize::deserialize(deserializer)?;

    raw.parse().map_err(|_| {
        DeError::invalid_value(Unexpected::Str(&raw), &"Expected a date in string form")
    })
}

fn deserialize_optional_date<'de, D>(deserializer: D) -> Result<Option<Date>, D::Error>
where
    D: Deserializer<'de>,
{
    let raw: Option<String> = Deserialize::deserialize(deserializer)?;

    match raw {
        Some(raw) => Ok(Some(raw.parse().map_err(|_| {
            DeError::invalid_value(Unexpected::Str(&raw), &"Expected a date in string form")
        })?)),
        None => Ok(None),
    }
}

#[cfg(test)]
mod test {
    use super::Frontmatter;
    use jiff::civil::date;

    #[test]
    fn missing_frontmatter() {
        // Parsing should fail if frontmatter is absent
        assert!(Frontmatter::from_text("abc123").is_err());
    }

    #[test]
    fn missing_fields() {
        // Parsing should fail if not all frontmatter fields are present
        assert!(Frontmatter::from_text("---\ntitle: abc\n---").is_err());
        assert!(
            Frontmatter::from_text("---\ntitle: abc\nslug: def\nupdated: 2000-01-01\n---").is_err()
        );
    }

    #[test]
    fn empty_fields() {
        // Parsing should fail if title or slug are empty
        assert!(Frontmatter::from_text("---\ntitle: \nslug: \ncreated: 2000-01-01\n---").is_err());
        assert!(
            Frontmatter::from_text("---\ntitle:  \nslug:  \ncreated: 2000-01-01\n---").is_err()
        );
    }

    #[test]
    fn invalid_date() {
        // Parsing should fail if date fields are invalid
        assert!(
            Frontmatter::from_text("---\ntitle: abc\nslug: def\ncreated: 123xyz\n---").is_err()
        );
        assert!(Frontmatter::from_text(
            "---\ntitle: abc\nslug: def\ncreated: 2020-01-01\nupdated: 123xyz\n---"
        )
        .is_err());
        assert!(Frontmatter::from_text(
            "---\ntitle: abc\nslug: def\ncreated: 123xyz\nupdated: 123xyz\n---"
        )
        .is_err());
        assert!(
            Frontmatter::from_text("---\ntitle: abc\nslug: def\ncreated: 2000-02-30\n---").is_err()
        );
    }

    #[test]
    fn no_update_field() {
        assert_eq!(
            Frontmatter::from_text("---\ntitle: abc\nslug: def\ncreated: 2000-01-01\n---")
                .expect("parsing should succeed"),
            Frontmatter {
                title: "abc".into(),
                slug: "def".into(),
                created: date(2000, 1, 1),
                updated: None
            }
        );
    }

    #[test]
    fn update_after_create() {
        // Parsing should fail if the last-updated date precedes the creation date
        assert!(Frontmatter::from_text(
            "---\ntitle: abc\nslug: def\ncreated: 2000-01-01\nupdated: 1900-01-01\n---"
        )
        .is_err());
        assert_eq!(
            Frontmatter::from_text(
                "---\ntitle: abc\nslug: def\ncreated: 2000-01-01\nupdated: 2000-01-01\n---"
            )
            .expect("parsing should succeed"),
            Frontmatter {
                title: "abc".into(),
                slug: "def".into(),
                created: date(2000, 1, 1),
                updated: Some(date(2000, 1, 1))
            }
        );
        assert_eq!(
            Frontmatter::from_text(
                "---\ntitle: abc\nslug: def\ncreated: 2000-01-01\nupdated: 2000-01-02\n---"
            )
            .expect("parsing should succeed"),
            Frontmatter {
                title: "abc".into(),
                slug: "def".into(),
                created: date(2000, 1, 1),
                updated: Some(date(2000, 1, 2))
            }
        );
    }

    #[test]
    fn timezones() {
        // Parsing timezones from date fields is not supported
        assert!(Frontmatter::from_text(
            "---\ntitle: abc\nslug: def\ncreated: 2000-01-01T00:00Z\nupdated: 2000-01-02T00:00-01:00\n---"
        )
        .is_err());
    }

    #[test]
    fn ignore_times() {
        // When times are included in the date fields, we expect the parser
        // to recognize but ignore them when producing a date output.
        assert_eq!(
            Frontmatter::from_text("---\ntitle: abc\nslug: def\ncreated: 2000-01-01T01:00\nupdated: 2000-01-01T00:00\n---")
                .expect("parsing should succeed"),
            Frontmatter {
                title: "abc".into(),
                slug: "def".into(),
                created: date(2000, 1, 1),
                updated: Some(date(2000, 1, 1))
            }
        );
    }
}
