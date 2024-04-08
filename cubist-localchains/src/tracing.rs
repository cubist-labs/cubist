use std::process::Stdio;

use tokio::{
    io::{AsyncBufReadExt, AsyncRead, BufReader},
    process::Child,
};
use tracing::{event, metadata::LevelFilter, Level};

const MIN_LEVEL: Level = Level::TRACE;

/// Whether we should trace std out/err of child processes
pub(crate) fn enable_stdout_tracing() -> bool {
    LevelFilter::current() >= MIN_LEVEL
}

/// Appropriate [`Stdio`] to use when spawning child processes, i.e.,
/// "piped" if tracing is enabled and "null" otherwise.
pub(crate) fn child_stdio() -> Stdio {
    if enable_stdout_tracing() {
        Stdio::piped()
    } else {
        Stdio::null()
    }
}

/// Asynchronously log std out and err of a given child process.
pub(crate) async fn trace_stdout(name: &str, child: &mut Child) {
    if enable_stdout_tracing() {
        let pid = child.id().unwrap_or(0);
        if let Some(stdout) = child.stdout.take() {
            trace_stream(format!("[{name}:{pid}][OUT]"), stdout);
        }
        if let Some(stderr) = child.stderr.take() {
            trace_stream(format!("[{name}:{pid}][ERR]"), stderr);
        }
    }
}

fn trace_stream<R: AsyncRead + Send + Unpin + 'static>(prefix: String, inner: R) {
    tokio::spawn(async move {
        let mut reader = BufReader::new(inner).lines();
        while let Ok(Some(line)) = reader.next_line().await {
            event!(MIN_LEVEL, "{prefix} {line}");
        }
    });
}
