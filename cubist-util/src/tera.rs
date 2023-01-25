//! Utility for embedding a directory full of Tera files.

use cubist_config::util::OrBug;

use rust_embed::RustEmbed;
use std::borrow::Cow;
use std::str::from_utf8;
use tera::Tera;

/// This trait provides a function that builds a Tera instance
/// from all .tpl files (recursively) contained in a single directory
/// specified by `path`, under the `RustEmbed` top-level import path.
///
/// `path` should either be "" (for the top-level directory) or
/// "subdir/subsubdir/name/" (including trailing slash) for any
/// directories below the top level.
pub trait TeraEmbed: RustEmbed {
    fn tera_from_prefix(path: &str) -> Tera {
        // make sure the path name is well formed
        assert!(path.is_empty() || path.ends_with('/'));

        let mut res = Tera::default();
        let iter = Self::iter()
            .filter(|f| f.starts_with(path) && f.ends_with(".tpl"))
            .map(|f| {
                let data = match Self::get(f.as_ref())
                    .expect("in CubeTemplates but not?")
                    .data
                {
                    Cow::Borrowed(d) => {
                        Cow::from(from_utf8(d).or_bug("Failed to parse templates."))
                    }
                    Cow::Owned(d) => {
                        Cow::from(String::from_utf8(d).or_bug("Failed to parse templates."))
                    }
                };
                let name = match f {
                    Cow::Borrowed(f) => {
                        Cow::from(f.strip_prefix(path).expect("should start with path arg"))
                    }
                    Cow::Owned(f) => Cow::from(
                        f.strip_prefix(path)
                            .expect("should start with path arg")
                            .to_owned(),
                    ),
                };
                (name, data)
            });
        res.add_raw_templates(iter)
            .or_bug("Failed to parse templates.");
        res
    }
}
