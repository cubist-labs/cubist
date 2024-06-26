//! This temporary module defines imports. It's temporary because we think we want
//! to pull import resolution out of IG, since it's up to the cli how it wants to
//! lay out files.
use solang_parser::pt;
use solang_parser::pt::Docable;
use std::fmt;

/// An import.  Simply wraps an [`pt::Import`] AST node from solc
#[derive(Debug)]
pub struct Import(pt::Import);

impl Import {
    pub fn new(import: &pt::Import) -> Import {
        Import(import.clone())
    }
}

impl fmt::Display for Import {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{};", self.0.display())
    }
}
