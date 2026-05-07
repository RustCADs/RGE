//! [`IconHandle`] — opaque address tuple `(IconSetId, IconName)`.
//!
//! A handle is a small, copyable identifier that callers pass around in
//! place of the underlying SVG bytes. Handles are produced by
//! [`IconRegistry::lookup`](crate::registry::IconRegistry::lookup) and
//! resolved back to bytes via the registry. Decoupling the address from
//! the payload lets the active icon set swap (Lucide → Phosphor → custom)
//! without touching the call sites that asked for `"folder-open"`.
//!
//! Both `IconSetId` and `IconName` are [`String`]-backed wrappers; they
//! are validated on construction (must be non-empty, no whitespace, no
//! filesystem traversal).

use std::fmt;

use serde::{Deserialize, Serialize};

/// Validation error for [`IconSetId`] / [`IconName`].
#[derive(Debug, thiserror::Error)]
pub enum IdError {
    /// The supplied string was empty.
    #[error("identifier must not be empty")]
    Empty,
    /// The supplied string contained whitespace, control bytes, or path
    /// separators that would be unsafe for filesystem lookup.
    #[error("identifier {0:?} contains forbidden characters")]
    BadChars(String),
}

fn validate_id(s: &str) -> Result<(), IdError> {
    if s.is_empty() {
        return Err(IdError::Empty);
    }
    if s.bytes().any(|b| {
        b.is_ascii_whitespace()
            || b.is_ascii_control()
            || b == b'/'
            || b == b'\\'
            || b == b'.'
            || b == b':'
    }) {
        return Err(IdError::BadChars(s.to_owned()));
    }
    Ok(())
}

/// Identifier of an icon set (e.g. `"lucide"`, `"phosphor"`).
///
/// The string form must be non-empty, contain no whitespace, and no
/// path-separator bytes; this constraint makes it safe to interpolate
/// into `assets/sets/<id>.icons.ron` paths without risk of traversal.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct IconSetId(String);

impl IconSetId {
    /// Construct a new identifier; rejects empty / unsafe strings.
    ///
    /// # Errors
    /// Returns [`IdError`] if the string is empty or contains
    /// whitespace, control bytes, `/`, `\`, `.`, or `:`.
    pub fn new(s: impl Into<String>) -> Result<Self, IdError> {
        let s = s.into();
        validate_id(&s)?;
        Ok(Self(s))
    }

    /// Borrow the inner string.
    #[inline]
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for IconSetId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Name of an icon within a set (e.g. `"folder-open"`, `"save"`).
///
/// Same validation rules as [`IconSetId`].
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct IconName(String);

impl IconName {
    /// Construct a new icon name.
    ///
    /// # Errors
    /// Returns [`IdError`] for the same reasons as [`IconSetId::new`],
    /// except that hyphens (`-`) are explicitly allowed (Lucide uses
    /// kebab-case).
    pub fn new(s: impl Into<String>) -> Result<Self, IdError> {
        let s = s.into();
        if s.is_empty() {
            return Err(IdError::Empty);
        }
        if s.bytes().any(|b| {
            b.is_ascii_whitespace()
                || b.is_ascii_control()
                || b == b'/'
                || b == b'\\'
                || b == b'.'
                || b == b':'
        }) {
            return Err(IdError::BadChars(s));
        }
        Ok(Self(s))
    }

    /// Borrow the inner string.
    #[inline]
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for IconName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Opaque handle returned from [`IconRegistry::lookup`].
///
/// `IconHandle` is intentionally cheap to clone (two heap-string allocs
/// — one for the set, one for the name); for hot callers cache the
/// handle once and re-use rather than calling `lookup` per frame.
///
/// [`IconRegistry::lookup`]: crate::registry::IconRegistry::lookup
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct IconHandle {
    /// Identifier of the icon set this handle points into.
    pub set: IconSetId,
    /// Name of the icon within that set.
    pub name: IconName,
}

impl IconHandle {
    /// Construct a handle directly. Most callers should go through
    /// [`IconRegistry::lookup`] instead, which validates that the icon
    /// actually exists.
    ///
    /// [`IconRegistry::lookup`]: crate::registry::IconRegistry::lookup
    #[must_use]
    pub fn new(set: IconSetId, name: IconName) -> Self {
        Self { set, name }
    }
}

impl fmt::Display for IconHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}::{}", self.set, self.name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_ids() {
        assert!(IconSetId::new("lucide").is_ok());
        assert!(IconName::new("folder-open").is_ok());
        assert!(IconName::new("save").is_ok());
    }

    #[test]
    fn empty_id_rejected() {
        assert!(matches!(IconSetId::new(""), Err(IdError::Empty)));
        assert!(matches!(IconName::new(""), Err(IdError::Empty)));
    }

    #[test]
    fn unsafe_chars_rejected() {
        assert!(matches!(
            IconSetId::new("foo bar"),
            Err(IdError::BadChars(_))
        ));
        assert!(matches!(
            IconSetId::new("../etc"),
            Err(IdError::BadChars(_))
        ));
        assert!(matches!(IconName::new("a/b"), Err(IdError::BadChars(_))));
        assert!(matches!(IconName::new("a\\b"), Err(IdError::BadChars(_))));
    }

    #[test]
    fn handle_display() {
        let h = IconHandle::new(
            IconSetId::new("lucide").unwrap(),
            IconName::new("folder-open").unwrap(),
        );
        assert_eq!(h.to_string(), "lucide::folder-open");
    }
}
