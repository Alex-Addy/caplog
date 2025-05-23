//! Utility for capturing and verification of log messages. Intended for use in testing of
//! applications and libraries.
//!
//! Inspired by the fixture of the same name from `pytest`.
//!
//! # Using Caplog
//!
//! The primary functions of interest are `get_handle` and `CaplogHandle::iter`. Use `get_handle`
//! at the top of your test in order to get a view of the messages being logged. After this, call
//! `.iter` on the handle at any point in your test to get an `Iterator<Record>`. This iterator can
//! then be used to view log messages.
//!
//! ```rust
//! # use log::{info, warn};
//! use caplog::get_handle;
//!
//! let handle = caplog::get_handle();
//! warn!("terrible thing happened!");
//! assert!(handle.iter().any(|(_, rec)| rec.msg.contains("terrible")));
//! ```
//!
//! # Handle's view of logs
//!
//! Each handle has access to all messages sent while it was alive. This means that messages sent
//! before it is made will not be available via `iter` or any other functions on it. So it is
//! recommended to call `get_handle` at the top of tests to ensure the messages will be scope.
//!
//! # Threading concerns
//!
//! As the `log` interface is global, messages from other threads may be visible via the handle.
//! Due to this, it is recommended to check for messages unique to the test when possible. For
//! example:
//!
//! ```rust
//! # use log::{info, warn};
//! # use caplog::get_handle;
//!
//! fn handle_request(id: u32) -> Result<(), ()> {
//!    info!("Got request from client {}", id);
//!    Ok(())
//! }
//!
//! let handle = caplog::get_handle();
//! let client_id = 12345; // id unique to this test
//! handle_request(client_id).unwrap();
//! handle.any_msg_contains(&format!("Got request from client {}", client_id));
//! ```
//!
//! Due to `info!` and the other `log` macros being blocking, it can be guaranteed that a message
//! will be visible to the same thread it was called on by the time it returns.
//!
//! # Interaction with other log handlers
//!
//! `log`'s interface only allows for a single log handler at a time. In order to prevent collision
//! with the regular handler, it is recommended to put initialization code for it either inside of
//! main or put a `[cfg(not(test))]` attribute on it.

#[macro_use]
extern crate lazy_static;

use std::sync::Arc;

lazy_static! {
    static ref _CAPTURE_LOG: Box<Caplog> = {
        let handler = Box::new(Caplog {
            logs: Arc::new(boxcar::Vec::new()),
        });
        log::set_boxed_logger(handler.clone()).unwrap();
        log::set_max_level(log::LevelFilter::Trace);
        handler
    };
}

#[derive(Clone)]
struct Caplog {
    logs: Arc<boxcar::Vec<Record>>,
}

impl log::Log for Caplog {
    fn enabled(&self, _metadata: &log::Metadata) -> bool {
        true
    }

    fn log(&self, record: &log::Record) {
        if self.enabled(record.metadata()) {
            self.logs.push(Record {
                level: record.level(),
                msg: record.args().to_string(),
            });
        }
    }

    fn flush(&self) {}
}

#[derive(Debug)]
/// A single log message.
pub struct Record {
    /// The level at which the message was logged.
    pub level: log::Level,

    /// The message formatted as a string
    pub msg: String,
}

/// Provides access to the logs stored in Caplog.
///
/// Access is limited to the time the handle has been alive. Log messages sent before handle has
/// been created and after `stop_recording` have been called will not be visible to the methods
/// provided by CaplogHandle.
pub struct CaplogHandle {
    start_idx: usize,
    stop_idx: Option<usize>,
    list: Arc<boxcar::Vec<Record>>,
}

impl CaplogHandle {
    // Returns true iff any log message within the capture range contains the provided snippet.
    pub fn any_msg_contains(&self, snippet: &str) -> bool {
        self.iter().any(|(_, rec)| rec.msg.contains(snippet))
    }

    /// Returns an iterator over the items viewable by this handle.
    ///
    /// Values are yielded in the form of (index, Record). There may be in progress concurrent
    /// writes that create gaps, so `index` may not be strictly sequential.
    pub fn iter(&self) -> Box<dyn Iterator<Item = (usize, &Record)> + '_> {
        match self.stop_idx {
            None => Box::new(self.list.iter().skip(self.start_idx)),
            Some(stop_idx) => Box::new(
                self.list
                    .iter()
                    .skip(self.start_idx)
                    .take(stop_idx - self.start_idx),
            ),
        }
    }

    pub fn stop_recording(&mut self) {
        self.stop_idx = Some(self.list.count());
    }
}

/// Get a handle to the recorded logs. Handle is bounded to only viewing the logs available while
/// it is alive.
///
/// # Example
/// ```rust
/// # use log::{info, warn};
/// info!("not recorded");
/// let handle = caplog::get_handle();
/// info!("recorded");
/// assert!(handle.iter().any(|(_, rec)| rec.msg.contains("not recorded")) == false);
/// assert!(handle.any_msg_contains("recorded"));
/// ```
pub fn get_handle() -> CaplogHandle {
    let log_list = _CAPTURE_LOG.logs.clone();
    CaplogHandle {
        start_idx: log_list.count(),
        stop_idx: None,
        list: log_list.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use log::{error, info, trace, warn};

    #[test]
    // Ensures that an info level log is recorded and any_msg_contains can see it
    fn simple_any_msg_contains() {
        let handle = get_handle();
        let num_recs = handle.list.count();
        info!("logging message");
        assert!(handle.list.count() > num_recs);
        assert!(handle.any_msg_contains("logging message"));
    }

    #[test]
    /// Verify an error message is recorded
    fn verify_simple_error() {
        let mut handle = get_handle();
        let message = "verify_simple_error";
        // Using the function name to help uniqueness
        error!("{}", message);
        handle.stop_recording();
        assert!(handle.any_msg_contains(message));
    }

    #[test]
    /// Verify an info message is recorded
    fn verify_simple_info() {
        let mut handle = get_handle();
        let message = "verify_simple_info";
        // Using the function name to help uniqueness
        info!("{}", message);
        handle.stop_recording();
        assert!(handle.any_msg_contains(message));
    }
    #[test]
    /// Verify a warn message is recorded
    fn verify_simple_warn() {
        let mut handle = get_handle();
        let message = "verify_simple_warn";
        // Using the function name to help uniqueness
        warn!("{}", message);
        handle.stop_recording();
        assert!(handle.any_msg_contains(message));
    }

    #[test]
    /// Verify a trace message is recorded
    fn verify_simple_trace() {
        let mut handle = get_handle();
        let message = "verify_simple_trace";
        // Using the function name to help uniqueness
        trace!("{}", message);
        handle.stop_recording();
        assert!(handle.any_msg_contains(message));
    }

    #[test]
    /// Verify that separate handles with different stop and start indexes works correctly.
    fn verify_concurrent_handles() {
        let messages = ["zero", "one", "two"];
        let full_handle = get_handle();
        warn!("{}", messages[0]);
        let mut partial_handle = get_handle();
        warn!("{}", messages[1]);
        partial_handle.stop_recording();
        warn!("{}", messages[2]);

        // only full should have first message
        assert!(full_handle.any_msg_contains(messages[0]));
        assert!(!partial_handle.any_msg_contains(messages[0]));
        // both handles should have second
        assert!(full_handle.any_msg_contains(messages[1]));
        assert!(partial_handle.any_msg_contains(messages[1]));
        // only full should have final message
        assert!(full_handle.any_msg_contains(messages[2]));
        assert!(!partial_handle.any_msg_contains(messages[2]));
    }
}
