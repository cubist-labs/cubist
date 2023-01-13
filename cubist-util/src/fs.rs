use std::path::Path;

pub fn is_within(checked: &Path, dir: &Path) -> std::io::Result<bool> {
    let canon_checked = checked.canonicalize()?;
    let canon_dir = dir.canonicalize()?;
    Ok(canon_checked.starts_with(canon_dir))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::Path;
    use tempfile::tempdir;

    #[test]
    fn is_within_test() {
        let tmp = tempdir().unwrap().into_path();
        let foo = tmp.join("foo");
        let bar = tmp.join("foo/bar");
        let barbaz = tmp.join("foo/barbaz");
        fs::create_dir_all(tmp).unwrap();
        fs::create_dir_all(bar.clone()).unwrap();
        fs::create_dir_all(barbaz.clone()).unwrap();
        assert!(!is_within(&bar, &barbaz).unwrap());
        assert!(is_within(&bar, &foo).unwrap());
        assert!(is_within(&barbaz, &foo).unwrap());
    }

    #[test]
    fn is_within_err_test() {
        assert!(is_within(Path::new("foo/bar"), Path::new("foo/bar/baz")).is_err())
    }
}
