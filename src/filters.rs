//! Local, multi-value filters. Empty means unrestricted; OR within a field.
use std::{collections::HashSet, fmt};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Selection(Vec<String>);
impl Selection {
    pub fn values(&self) -> &[String] {
        &self.0
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    pub fn len(&self) -> usize {
        self.0.len()
    }
    pub fn clear(&mut self) {
        self.0.clear();
    }
    pub fn single(&self) -> &str {
        if self.len() == 1 { &self.0[0] } else { "" }
    }
    pub fn contains(&self, value: &str) -> bool {
        if value.is_empty() {
            return self.is_empty();
        }
        self.0
            .iter()
            .any(|v| v.to_lowercase() == value.to_lowercase())
    }
    pub fn insert(&mut self, value: String) {
        let value = value.trim();
        if !value.is_empty() && !self.contains(value) {
            self.0.push(value.into());
        }
    }
    pub fn toggle(&mut self, value: &str) {
        if value.is_empty() {
            self.clear();
        } else if self.contains(value) {
            let value = value.to_lowercase();
            self.0.retain(|v| v.to_lowercase() != value);
        } else {
            self.insert(value.into());
        }
    }
    pub fn normalized(&self) -> Vec<String> {
        self.0.iter().map(|v| v.to_lowercase()).collect()
    }
    pub fn label(&self, empty: &str) -> String {
        if self.is_empty() {
            empty.into()
        } else {
            self.0.join(" OR ")
        }
    }
}
impl From<Vec<String>> for Selection {
    fn from(values: Vec<String>) -> Self {
        let mut seen = HashSet::new();
        Self(
            values
                .into_iter()
                .map(|v| v.trim().to_owned())
                .filter(|v| !v.is_empty() && seen.insert(v.to_lowercase()))
                .collect(),
        )
    }
}
impl From<String> for Selection {
    fn from(value: String) -> Self {
        vec![value].into()
    }
}
impl From<&str> for Selection {
    fn from(value: &str) -> Self {
        value.to_owned().into()
    }
}
impl PartialEq<&str> for Selection {
    fn eq(&self, value: &&str) -> bool {
        if value.is_empty() {
            self.is_empty()
        } else {
            self.len() == 1 && self.single() == *value
        }
    }
}
impl fmt::Display for Selection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.label(""))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn toggles_are_case_insensitive_and_all_is_exclusive() {
        let mut s = Selection::default();
        s.toggle("To Do");
        s.toggle("On Hold");
        assert_eq!(s.len(), 2);
        s.toggle("TO DO");
        assert_eq!(s, "On Hold");
        s.toggle("");
        assert!(s.is_empty());
        s.insert(" On Hold ".into());
        s.insert("on hold".into());
        assert_eq!(s.len(), 1);
    }
    #[test]
    fn exact_values_with_commas_are_not_split() {
        let s = Selection::from("Review, then test");
        assert_eq!(s.len(), 1);
        assert_eq!(s.single(), "Review, then test");
    }
}
