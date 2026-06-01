//! Startup configuration loading.

use std::path::Path;
use std::{fmt, fs, io};

use serde::Deserialize;

use crate::note::{Tag, TagColor};

/// Load the user's tag set from the TOML config at `path`.
pub fn load_tags(path: &Path) -> Result<Vec<Tag>, ConfigError> {
    let text = match fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            return Ok(default_tags());
        }
        Err(e) => return Err(ConfigError::Io(e)),
    };
    let raw: RawConfig = toml::from_str(&text).map_err(ConfigError::Parse)?;
    raw.tag.into_iter().map(RawTag::into_tag).collect()
}

#[derive(Debug, Default, Deserialize)]
struct RawConfig {
    #[serde(default)]
    tag: Vec<RawTag>,
}

#[derive(Debug, Deserialize)]
/// A tag as deserialised from the toml.
struct RawTag {
    name: String,
    color: Option<String>,
}

impl RawTag {
    fn into_tag(self) -> Result<Tag, ConfigError> {
        let color = match self.color {
            Some(hex) => Some(TagColor::from_hex(&hex).ok_or_else(|| {
                ConfigError::BadColor {
                    tag: self.name.clone(),
                    value: hex,
                }
            })?),
            None => None,
        };
        Ok(Tag::new(self.name, color))
    }
}

fn default_tags() -> Vec<Tag> {
    ["todo", "next", "done", "idea", "work", "personal"]
        .into_iter()
        .map(|s| Tag::new(s.to_string(), None))
        .collect()
}

#[derive(Debug)]
pub enum ConfigError {
    Io(io::Error),
    Parse(toml::de::Error),
    BadColor { tag: String, value: String },
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConfigError::Io(e) => write!(f, "config i/o error: {e}"),
            ConfigError::Parse(e) => write!(f, "config parse error: {e}"),
            ConfigError::BadColor { tag, value } => write!(
                f,
                "tag '{tag}' has invalid color '{value}' \
                 (expected #rrggbb hex)"
            ),
        }
    }
}

impl std::error::Error for ConfigError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ConfigError::Io(e) => Some(e),
            ConfigError::Parse(e) => Some(e),
            ConfigError::BadColor { .. } => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> Result<Vec<Tag>, ConfigError> {
        let raw: RawConfig =
            toml::from_str(text).map_err(ConfigError::Parse)?;
        raw.tag.into_iter().map(RawTag::into_tag).collect()
    }

    #[test]
    fn parses_documented_schema() {
        let text = r##"
[[tag]]
name = "hello"
color = "#ff00ee"

[[tag]]
name = "world"
"##;
        let tags = parse(text).unwrap();
        assert_eq!(tags.len(), 2);
        assert_eq!(tags[0].name, "hello");
        assert_eq!(tags[0].color, TagColor::Rgb(0xff, 0x00, 0xee));
        assert_eq!(tags[1].name, "world");
        // "world" has no explicit color -- the hashed fallback kicks in.
        assert_eq!(tags[1].color, TagColor::hashed_from_name("world"));
    }

    #[test]
    fn rejects_bad_hex() {
        let text = r#"
[[tag]]
name = "broken"
color = "not-a-color"
"#;
        let err = parse(text).unwrap_err();
        assert!(matches!(err, ConfigError::BadColor { .. }));
    }

    #[test]
    fn empty_config_is_ok() {
        let tags = parse("").unwrap();
        assert!(tags.is_empty());
    }
}
