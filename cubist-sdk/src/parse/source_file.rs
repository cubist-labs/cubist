/// Our representation of an source file.
/// We use this representation because we need to package a file's
/// contents and AST and target so that we can both analyze it and
/// later produce an interface for it and a bridge file. In other
/// words: we need this information packaged together multiple times
use crate::{CubistSdkError, Result};
use cubist_config::{Config, Target};
use cubist_util::fs::is_within;
use solang_parser::pt;
use std::fs;
use std::path::{Path, PathBuf};

/// A source file with its AST and additional meta information
#[derive(Debug)]
pub struct SourceFile {
    /// The source file's absolute path
    pub file_name: PathBuf,
    /// The source file's relative path to a root contracts folder
    pub rel_path: PathBuf,
    /// The AST the file contains
    pub pt: pt::SourceUnit,
    /// Comments from the source file
    pub comments: Vec<pt::Comment>,
    /// What target the code in the file runs on
    pub target: Target,
}

impl SourceFile {
    /// Create a new source file given a Cubist contract config.
    /// Errors if it encounters problems with in file system or the parser.
    pub fn new(file: impl AsRef<Path>, rel_path: PathBuf, target: Target) -> Result<Self> {
        let code = fs::read_to_string(file.as_ref())
            .map_err(|e| CubistSdkError::ReadFileError(file.as_ref().into(), e))?;
        let file_name = file.as_ref().to_path_buf();
        match solang_parser::parse(&code, 0) {
            Ok((pt, comments)) => Ok(SourceFile {
                file_name,
                rel_path,
                pt,
                comments,
                target,
            }),
            Err(es) => Err(CubistSdkError::ParseError(file_name, es)),
        }
    }

    /// Returns a list of import directives in the source file
    pub fn extract_import_directives(&self) -> Vec<pt::Import> {
        self.pt
            .0
            .iter()
            .filter_map(|part| match part {
                pt::SourceUnitPart::ImportDirective(imp) => Some(imp.clone()),
                _ => None,
            })
            .collect::<Vec<_>>()
    }

    /// Check the imports in this file for:
    /// (1) Absolute paths that point into the contracts root directory.
    ///     This is a problem since Cubist copies the contents of the contracts dir.
    /// (2) Relative paths that point outside the contracts root directory.
    ///     This is a problem for the same reason as the previous.
    pub fn check_imports(&self, config: &Config) -> Result<()> {
        let root_dir = &config.contracts().root_dir;
        for import in self.extract_import_directives() {
            let import_lit = match import {
                pt::Import::Plain(s, ..) => s,
                pt::Import::GlobalSymbol(s, ..) => s,
                pt::Import::Rename(s, ..) => s,
            };
            // Check for external (node) imports. Those are fine.
            // In the future we'll need to extend this for other external imports we support
            if import_lit.string.starts_with('@') {
                continue;
            }
            if import_lit.unicode {
                return Err(CubistSdkError::UnicodeImportError(
                    import_lit.string,
                    self.file_name.clone(),
                ));
            }
            let path = Path::new(&import_lit.string);
            // Is it an absolute path that points into the root directory?
            if path.is_absolute() {
                match is_within(path, root_dir) {
                    Err(e) => {
                        return Err(CubistSdkError::CanonicalizationError(
                            import_lit.string,
                            self.file_name.clone(),
                            Some(e),
                        ))
                    }
                    Ok(points_into_root) => {
                        if points_into_root {
                            return Err(CubistSdkError::AbsolutePathError(
                                import_lit.string,
                                self.file_name.clone(),
                            ));
                        }
                    }
                }
            }
            // Is it a relative path that points outside the root directory?
            if path.is_relative() {
                let parent_path = self.file_name.parent();
                if parent_path.is_none() {
                    return Err(CubistSdkError::CanonicalizationError(
                        import_lit.string,
                        self.file_name.clone(),
                        None,
                    ));
                }
                let full_path = parent_path.unwrap().join(path);
                match is_within(&full_path, root_dir) {
                    Err(e) => {
                        return Err(CubistSdkError::CanonicalizationError(
                            import_lit.string,
                            self.file_name.clone(),
                            Some(e),
                        ))
                    }
                    Ok(is_within_root) => {
                        if !is_within_root {
                            return Err(CubistSdkError::RelativePathError(
                                import_lit.string,
                                self.file_name.clone(),
                            ));
                        }
                    }
                }
            }
        }
        Ok(())
    }
}
