use std::net::TcpListener;

/// Tries to find next available port starting from `start + 1`,
/// searching up to `start + 1000`.  Returns [`None`] if none are
/// available.
pub fn try_next_available_port(start: u16) -> Option<u16> {
    let mut num_tries_left = 1000;
    let mut port = start + 1;
    while num_tries_left > 0 {
        if is_available_port(port) {
            return Some(port);
        } else {
            num_tries_left -= 1;
            port += 1;
        }
    }

    None
}

/// Returns next available port starting from `start + 1`, searchinig
/// up to `start + 1000`.
///
/// # Panics
///
/// If no available port is found within this range.
pub fn next_available_port(start: u16) -> u16 {
    try_next_available_port(start)
        .unwrap_or_else(|| panic!("Could not find an open port starting from {}", start + 1))
}

/// Returns whether a given port is currently availble.
pub fn is_available_port(port: u16) -> bool {
    TcpListener::bind(("127.0.0.1", port)).is_ok()
}
