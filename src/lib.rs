#[macro_use]
extern crate lazy_static;

use std::sync::Arc;

mod stable_list;
use stable_list::StableList;

lazy_static! {
    static ref _CAPTURE_LOG: Box<Caplog> = {
        let handler = Box::new(Caplog {
            logs: Arc::new(StableList::new()),
        });
        log::set_boxed_logger(handler.clone()).unwrap();
        log::set_max_level(log::LevelFilter::Trace);
        handler
    };
}

#[derive(Clone)]
struct Caplog {
    logs: Arc<StableList<Record>>,
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
            })
        }
    }

    fn flush(&self) {}
}

#[derive(Debug)]
pub struct Record {
    pub level: log::Level,
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
    list: Arc<StableList<Record>>,
}

impl CaplogHandle {
    pub fn any_msg_contains(&self, snippet: &str) -> bool {
        self.list
            .bounded_iter(self.start_idx, self.stop_idx)
            .any(|rec| rec.msg.contains(snippet))
    }

    /// Returns an iterator over the items viewable by this handle.
    pub fn iter(&self) -> crate::stable_list::StableListIterator<Record> {
        // TODO remove StableListIterator type from exposed types
        self.list.bounded_iter(self.start_idx, self.stop_idx)
    }

    pub fn stop_recording(&mut self) {
        self.stop_idx = Some(self.list.len());
    }
}

/// Get a handle to the recorded logs. Handle is restricted to only viewing the logs available
/// while it is alive.
///
/// # Example
/// ```rust
/// # use log::{info, warn};
/// info!("not recorded");
/// let handle = caplog::get_handle();
/// info!("recorded");
/// assert!(handle.iter().any(|rec| rec.msg.contains("not recorded")) == false);
/// assert!(handle.any_msg_contains("recorded"));
/// ```
pub fn get_handle() -> CaplogHandle {
    let log_list = _CAPTURE_LOG.logs.clone();
    CaplogHandle {
        start_idx: log_list.len(),
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
        let num_recs = handle.list.len();
        info!("logging message");
        assert!(handle.list.len() > num_recs);
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
}
