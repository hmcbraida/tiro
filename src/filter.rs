//! Free-form filter language for note search.
//!
//! Whitespace-separated tokens:
//! - `tag:NAME` requires a tag,
//! - `-tag:NAME` excludes a tag,
//! - any other token is a text term; every text term must match the note
//!   (orderless AND) against either the contents or any tag.
//!
//! Parsing is infallible: malformed input degrades to text.

use crate::note::StoredNote;

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Filter {
    pub text_terms: Vec<String>,
    pub require_tags: Vec<String>,
    pub exclude_tags: Vec<String>,
}

impl Filter {
    pub fn parse(input: &str) -> Self {
        let mut text_terms: Vec<String> = Vec::new();
        let mut require_tags: Vec<String> = Vec::new();
        let mut exclude_tags: Vec<String> = Vec::new();

        for tok in input.split_whitespace() {
            if let Some(name) = tok.strip_prefix("tag:") {
                if name.is_empty() || !is_valid_tag(name) {
                    text_terms.push(tok.to_lowercase());
                } else {
                    require_tags.push(name.to_string());
                }
            } else if let Some(name) = tok.strip_prefix("-tag:") {
                if name.is_empty() || !is_valid_tag(name) {
                    text_terms.push(tok.to_lowercase());
                } else {
                    exclude_tags.push(name.to_string());
                }
            } else {
                text_terms.push(tok.to_lowercase());
            }
        }

        Filter {
            text_terms,
            require_tags,
            exclude_tags,
        }
    }

    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.text_terms.is_empty()
            && self.require_tags.is_empty()
            && self.exclude_tags.is_empty()
    }

    pub fn matches(&self, note: &StoredNote) -> bool {
        for req in &self.require_tags {
            if !note.note.tags.contains(req) {
                return false;
            }
        }
        for ex in &self.exclude_tags {
            if note.note.tags.contains(ex) {
                return false;
            }
        }
        if self.text_terms.is_empty() {
            return true;
        }
        let contents = note.note.contents.to_lowercase();
        let tags: Vec<String> =
            note.note.tags.iter().map(|t| t.to_lowercase()).collect();
        self.text_terms.iter().all(|term| {
            contents.contains(term) || tags.iter().any(|t| t.contains(term))
        })
    }
}

fn is_valid_tag(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::note::Note;
    use std::collections::HashSet;

    fn note(contents: &str, tags: &[&str]) -> StoredNote {
        let tags: HashSet<String> =
            tags.iter().map(|s| (*s).to_string()).collect();
        StoredNote::new("id".into(), Note::new(contents.into(), tags))
    }

    #[test]
    fn empty_filter_matches_anything() {
        let f = Filter::parse("");
        assert!(f.is_empty());
        assert!(f.matches(&note("hello", &["work"])));
    }

    #[test]
    fn bare_text_matches_contents() {
        let f = Filter::parse("hello");
        assert!(f.matches(&note("say hello world", &[])));
        assert!(!f.matches(&note("nope", &[])));
    }

    #[test]
    fn bare_text_matches_tags() {
        let f = Filter::parse("wor");
        assert!(f.matches(&note("nothing here", &["work"])));
    }

    #[test]
    fn single_required_tag() {
        let f = Filter::parse("tag:work");
        assert_eq!(f.require_tags, vec!["work"]);
        assert!(f.matches(&note("", &["work"])));
        assert!(!f.matches(&note("", &["personal"])));
    }

    #[test]
    fn multiple_tags_anded() {
        let f = Filter::parse("tag:work tag:todo");
        assert!(f.matches(&note("", &["work", "todo"])));
        assert!(!f.matches(&note("", &["work"])));
        assert!(!f.matches(&note("", &["todo"])));
    }

    #[test]
    fn exclude_tag() {
        let f = Filter::parse("-tag:done");
        assert!(f.matches(&note("", &["work"])));
        assert!(!f.matches(&note("", &["done"])));
    }

    #[test]
    fn mixed_text_and_tag() {
        let f = Filter::parse("foo tag:work -tag:done");
        assert!(f.matches(&note("foo bar", &["work"])));
        assert!(!f.matches(&note("foo bar", &["work", "done"])));
        assert!(!f.matches(&note("foo bar", &["personal"])));
        assert!(!f.matches(&note("no match", &["work"])));
    }

    #[test]
    fn bare_tag_prefix_is_text() {
        let f = Filter::parse("tag:");
        assert!(f.require_tags.is_empty());
        assert_eq!(f.text_terms, vec!["tag:"]);
    }

    #[test]
    fn invalid_tag_chars_become_text() {
        let f = Filter::parse("tag:has space");
        // "tag:has" is malformed (space breaks the token), so the whole
        // token is "tag:has" -- has only alphanumeric, so still treated as a
        // tag. The "space" token is text.
        assert_eq!(f.require_tags, vec!["has"]);
        assert_eq!(f.text_terms, vec!["space"]);

        let f = Filter::parse("tag:a/b");
        assert!(f.require_tags.is_empty());
        assert_eq!(f.text_terms, vec!["tag:a/b"]);
    }

    #[test]
    fn multiple_text_terms_anded_orderless() {
        let f = Filter::parse("sarah birthday");
        assert!(f.matches(&note("sarah's birthday is tomorrow", &[])));
        assert!(f.matches(&note("birthday party for sarah", &[])));
        assert!(!f.matches(&note("sarah went home", &[])));
        assert!(!f.matches(&note("happy birthday", &[])));
    }

    #[test]
    fn text_terms_can_match_across_contents_and_tags() {
        let f = Filter::parse("sarah birthday");
        assert!(f.matches(&note("sarah came over", &["birthday"])));
        assert!(f.matches(&note("cake time", &["sarah", "birthday"])));
    }
}
