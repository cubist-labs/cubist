use std::fmt::Debug;

/// Implemented for "option" types, i.e., those that can be unwrapped
/// into a value, to provide a utility method that either unwraps the
/// value or reports a bug.
pub trait OrBug<T> {
    /// Unwrap or report a bug.
    #[track_caller]
    fn or_bug(self, msg: &str) -> T;
}

fn format_msg(msg: &str) -> String {
    format!("[BUG] {msg}")
}

impl<TR, TE: Debug> OrBug<TR> for Result<TR, TE> {
    #[track_caller]
    fn or_bug(self, msg: &str) -> TR {
        self.unwrap_or_else(|e| panic!("{}: {e:?}", &format_msg(msg)))
    }
}

impl<T> OrBug<T> for Option<T> {
    #[track_caller]
    fn or_bug(self, msg: &str) -> T {
        self.unwrap_or_else(|| panic!("{}", &format_msg(msg)))
    }
}
