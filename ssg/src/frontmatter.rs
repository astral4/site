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
    fn frontmatter() {
        const BAD_1: &str = "abc123";
        const BAD_2: &str = "---\ntitle: abc\n---";
        const BAD_3: &str = "---\ntitle: abc\nslug: def\ncreated: 123xyz\nupdated: 123xyz\n---";
        const BAD_4: &str = "---\ntitle: \nslug: \ncreated: 2000-01-01\n---";
        const BAD_5: &str = "---\ntitle: abc\nslug: def\ncreated: 2000-02-30\n---";
        const BAD_6: &str =
            "---\ntitle: abc\nslug: def\ncreated: 2000-01-01\nupdated: 1900-01-01\n---";
        const BAD_7: &str =
            "---\ntitle: abc\nslug: def\ncreated: 2000-01-01T00:00Z\nupdated: 2000-01-01T00:00-01:00\n---";

        const GOOD_1: &str = "---\ntitle: abc\nslug: def\ncreated: 2000-01-01\n---";
        const GOOD_2: &str =
            "---\ntitle: abc\nslug: def\ncreated: 2000-01-01\nupdated: 2000-01-01\n---";
        const GOOD_3: &str =
            "---\ntitle: abc\nslug: def\ncreated: 2000-01-01\nupdated: 2000-01-02\n---";
        const GOOD_4: &str =
            "---\ntitle: abc\nslug: def\ncreated: 2000-01-01T01:00\nupdated: 2000-01-01T00:00\n---";

        assert!(
            Frontmatter::from_text(BAD_1).is_err(),
            "parsing should fail if frontmatter is absent"
        );
        assert!(
            Frontmatter::from_text(BAD_2).is_err(),
            "parsing should fail if not all frontmatter fields are present"
        );
        assert!(
            Frontmatter::from_text(BAD_3).is_err(),
            "parsing should fail if date fields are invalid"
        );
        assert!(
            Frontmatter::from_text(BAD_4).is_err(),
            "parsing should fail if title or slug are empty"
        );
        assert!(
            Frontmatter::from_text(BAD_5).is_err(),
            "parsing should fail if a date is invalid"
        );
        assert!(
            Frontmatter::from_text(BAD_6).is_err(),
            "parsing should fail if the last-updated date precedes the creation date"
        );
        assert!(
            Frontmatter::from_text(BAD_7).is_err(),
            "timezone parsing is not supported"
        );
        assert_eq!(
            Frontmatter::from_text(GOOD_1).expect("parsing should succeed"),
            Frontmatter {
                title: "abc".into(),
                slug: "def".into(),
                created: date(2000, 1, 1),
                updated: None
            }
        );
        assert_eq!(
            Frontmatter::from_text(GOOD_2).expect("parsing should succeed"),
            Frontmatter {
                title: "abc".into(),
                slug: "def".into(),
                created: date(2000, 1, 1),
                updated: Some(date(2000, 1, 1))
            }
        );
        assert!(
            Frontmatter::from_text(GOOD_3).is_ok(),
            "parsing should succeed"
        );
        assert_eq!(
            Frontmatter::from_text(GOOD_4).expect("parsing should succeed due to ignoring dates"),
            Frontmatter {
                title: "abc".into(),
                slug: "def".into(),
                created: date(2000, 1, 1),
                updated: Some(date(2000, 1, 1))
            }
        );
    }
}
