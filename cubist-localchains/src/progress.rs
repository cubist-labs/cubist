use std::{path::Path, time::Duration};

use console::Term;
use indicatif::{
    HumanBytes, HumanCount, HumanDuration, MultiProgress, ProgressBar, ProgressDrawTarget,
    ProgressState, ProgressStyle,
};

use crate::error::Result;

const DOWNLOAD_STYLE: &str = "{prefix:>13} [{elapsed:>4}] [{bar:.cyan}] <SUFFIX>";
const EXTRACT_STYLE: &str = "{prefix:>13} [{elapsed:>4}] {pos}/{len} {msg}";

struct DrawTarget {
    target: ProgressDrawTarget,
    is_user_attended: bool,
}

fn draw_target() -> DrawTarget {
    let term = Term::buffered_stderr();
    let refresh_rate = 20;
    let is_user_attended = term.is_term();
    let target = ProgressDrawTarget::term(term, refresh_rate);
    DrawTarget {
        target,
        is_user_attended,
    }
}

/// Progress bar to display while downloading chain binaries.
///
/// The "length" of this progress bar is the total number of bytes to be downloaded,
/// and its "position" is the number of bytes downloaded so far.
pub struct DownloadPb {
    pb: ProgressBar,
}

enum DownloadStyleKind {
    InProgress,
    End,
}

impl Default for DownloadPb {
    fn default() -> Self {
        Self::new()
    }
}

impl DownloadPb {
    /// Constructor
    pub fn new() -> Self {
        let pb = ProgressBar::with_draw_target(None, draw_target().target)
            .with_style(Self::style(DownloadStyleKind::InProgress))
            .with_prefix("starting")
            .with_position(0);
        pb.tick();
        Self { pb }
    }

    /// Report that downloading has started.
    ///
    /// # Arguments
    /// * `len` - number of bytes to be downloaded (if known)
    pub fn started(&self, len: Option<u64>) {
        self.pb.set_prefix("downloading");
        if let Some(len) = len {
            self.pb.set_length(len);
        }
    }

    /// Update current progress.
    ///
    /// # Arguments
    /// * `bytes` - total number of bytes downloaded so far
    pub fn update(&self, bytes: u64) {
        self.pb.set_position(bytes);
    }

    /// Report that downloading has finished.
    pub fn finished(&self) {
        self.pb.set_style(Self::style(DownloadStyleKind::End));
        self.pb.finish();
        if !draw_target().is_user_attended {
            println!(
                "  downloaded {} bytes in {}",
                HumanBytes(self.pb.position()),
                HumanDuration(self.pb.elapsed())
            );
        }
    }

    fn style(kind: DownloadStyleKind) -> ProgressStyle {
        let suffix = match kind {
            DownloadStyleKind::InProgress => "{bytes}/{total_bytes}",
            DownloadStyleKind::End => "{total_bytes}",
        };
        let sty = DOWNLOAD_STYLE.replace("<SUFFIX>", suffix);
        ProgressStyle::with_template(&sty)
            .unwrap()
            .progress_chars("#>-")
    }
}

/// Progress bar to display while extracting downloaded chain binaries.
///
/// The "length" of this progress bar is the number of items to be extracted,
/// and its "position" is the number of items extracted so far.
pub struct ExtractPb {
    pb: ProgressBar,
}

impl Default for ExtractPb {
    fn default() -> Self {
        Self::new()
    }
}

impl ExtractPb {
    /// Constructor
    pub fn new() -> Self {
        let sty = ProgressStyle::with_template(EXTRACT_STYLE).unwrap();
        let pb = ProgressBar::with_draw_target(None, draw_target().target)
            .with_style(sty)
            .with_prefix("extracting")
            .with_position(0);
        pb.tick();
        Self { pb }
    }

    /// Report how many items are to be extracted
    ///
    /// # Arguments
    /// * `len` - number of items to be extracted (if known)
    pub fn started(&self, len: Option<u64>) {
        if let Some(len) = len {
            self.pb.set_length(len);
        }
    }

    /// Report that an item is being extracted
    ///
    /// # Arguments
    /// * `item` - name of the item being extracted
    pub fn extracting(&self, item: &Path) {
        let name = item.file_name().and_then(|n| n.to_str()).unwrap_or("");
        self.pb.set_message(name.to_owned());
    }

    /// Report that an item has been extracted
    ///
    /// # Arguments
    /// * `item` - name of the item that was extracted
    pub fn extracted(&self, _item: &Path) {
        self.pb.set_message("");
        self.pb.inc(1);
    }

    /// Report that extracting has finished.
    pub fn finished(&self) {
        self.pb.set_message("");
        self.pb.finish();
        if !draw_target().is_user_attended {
            println!(
                "  extracted {} {} in {}",
                HumanCount(self.pb.position()),
                if self.pb.position() > 1 {
                    "files"
                } else {
                    "file"
                },
                HumanDuration(self.pb.elapsed())
            );
        }
    }
}

/// Progress bar to display during server initialization.
///
/// The "length" of this progress bar is the expected total duration (in milliseconds)
/// and its "position" is the elapsed time since start (also in milliseconds).
pub struct ServerPb {
    pb: ProgressBar,
    url: String,
    name: String,
    name_width: u8,
}

/// Many [`ServerPb`] at once
pub struct ServerMpb {
    mpb: MultiProgress,
    max_name_len: u8,
}

enum ServerStyle {
    Start,
    End(bool),
}

impl ServerMpb {
    /// Constructor
    pub fn new(max_name_len: u8) -> Self {
        let mpb = MultiProgress::new();
        Self { mpb, max_name_len }
    }

    /// Report that a new server is starting
    ///
    /// # Arguments
    /// * `url` - target chain endpoint
    /// * `name` - target chain name
    /// * `eta` - estimated duration of the server initialization
    pub fn add(&mut self, url: String, name: String, eta: Duration) -> ServerPb {
        let spb = ServerPb::new(eta, name, self.max_name_len, url);
        let pb = self.mpb.add(spb.pb);
        let spb = ServerPb { pb, ..spb };
        spb.start();
        spb
    }
}

impl ServerPb {
    /// Constructor
    ///
    /// # Arguments
    /// * `eta` - estimated total duration
    /// * `name` - name of the server being started
    /// * `name_width` - width to use when rendering `name`
    /// * `url` - server url
    pub fn new(eta: Duration, name: String, name_width: u8, url: String) -> Self {
        let pb = ProgressBar::with_draw_target(Some(eta.as_millis() as u64), draw_target().target);
        Self {
            pb,
            url,
            name,
            name_width,
        }
    }

    /// Start the progress bar.  Call `auto_update` on this instance
    /// concurrently with some other task to continue to refresh this
    /// progress bar.
    pub fn start(&self) {
        self.pb.set_style(self.style(ServerStyle::Start));
        self.pb.set_prefix(self.name.clone());
        self.pb.set_position(0);
        if !draw_target().is_user_attended {
            println!("  starting {}...", self.name);
        }
    }

    /// Returns a task that updates this progress bar every 100ms
    /// until the bar is marked as finished.  Tasks in Rust are
    /// inert, so you must await on it (presumably together with
    /// some other task) to keep it going).
    pub async fn auto_update(&self) -> Result<()> {
        while !self.pb.is_finished() {
            let pos = self.pb.elapsed().as_millis() as u64;
            self.pb.set_position(pos);
            // extend length if position is already greater or equal
            if let Some(len) = self.pb.length() {
                if pos >= len {
                    self.pb.set_length(((len as f32) * 1.1) as u64);
                }
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
        Ok(())
    }

    /// Report that the server is done initializing
    ///
    /// # Arguments
    /// * `succeeded` - whether the server was initialized successfully
    pub fn finished(&self, succeeded: bool) {
        self.pb.set_style(self.style(ServerStyle::End(succeeded)));
        self.pb.set_length(self.pb.position());
        self.pb.finish();
        if !draw_target().is_user_attended {
            println!(
                "  {} initialized in {}: {}",
                self.name,
                HumanDuration(self.pb.elapsed()),
                self.url
            );
        }
    }

    /// If already finished, it returns the total duration (from start
    /// to finish); otherwise, returns expected total duration.
    pub fn duration(&self) -> Duration {
        if self.pb.is_finished() {
            Duration::from_millis(self.pb.length().expect("We always set length"))
        } else {
            self.pb.duration()
        }
    }

    fn style(&self, kind: ServerStyle) -> ProgressStyle {
        let (spinner_end, suffix, style) = match kind {
            ServerStyle::Start => (" ", "eta: {eta}", ""),
            ServerStyle::End(ok) => {
                if ok {
                    ("✔", "{url:.yellow}", ".green.bold")
                } else {
                    ("✘", "", ".red.bold")
                }
            }
        };
        let tick_chars = "⠁⠁⠉⠙⠚⠒⠂⠂⠒⠲⠴⠤⠄⠄⠤⠠⠠⠤⠦⠖⠒⠐⠐⠒⠓⠋⠉⠈⠈";
        let url = self.url.clone();
        ProgressStyle::with_template(
            &"  {prefix:><NAME_WIDTH>.blue.bold} {spinner:<STYLE>} [{elapsed:>4}] [{bar:.cyan}] <SUFFIX> "
                .replace("<NAME_WIDTH>", &self.name_width.to_string())
                .replace("<SUFFIX>", suffix)
                .replace("<STYLE>", style),
        )
        .unwrap()
        .progress_chars(".. ")
        .with_key(
            "url",
            move |_: &ProgressState, w: &mut dyn std::fmt::Write| write!(w, "{}", url).unwrap(),
        )
        .tick_chars(&format!("{tick_chars}{spinner_end}"))
    }
}
