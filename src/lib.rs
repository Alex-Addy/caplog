#[macro_use]
extern crate lazy_static;

use std::sync::{Arc, Mutex};

use log;

lazy_static! {
    static ref _CAPTURE_LOG: Box<Caplog> = {
        let handler = Box::new(Caplog {
            inner: Arc::new(Mutex::new(CaplogInner::new()))
        });
        log::set_boxed_logger(handler.clone()).unwrap();
        log::set_max_level(log::LevelFilter::Trace);
        handler
    };
}

#[derive(Clone)]
struct Caplog {
    inner: Arc<Mutex<CaplogInner>>,
}

impl log::Log for Caplog {
    fn enabled(&self, _metadata: &log::Metadata) -> bool {
        true
    }

    fn log(&self, record: &log::Record) {
        if self.enabled(record.metadata()) {
            self.inner.lock().unwrap().logs.push(Record {
                level: record.level(),
                msg: record.args().to_string(),
            })
        }
    }

    fn flush(&self) {}
}

struct Record {
    level: log::Level,
    msg: String,
}

struct CaplogInner {
    logs: Vec<Record>,
}

impl CaplogInner {
    fn new() -> Self {
        Self { logs: Vec::new() }
    }
}

pub struct CaplogHandle {
    start_idx: usize,
    end_idx: Option<usize>,
}

impl CaplogHandle {
    pub fn any_msg_contains(&self, sub_string: &str) -> bool {
        let lock = _CAPTURE_LOG.inner.lock().unwrap();
        let msg_range = if let Some(end) = self.end_idx {
            self.start_idx..end
        } else {
            self.start_idx..lock.logs.len()
        };
        lock.logs[msg_range].iter().any(|rec| rec.msg.contains(sub_string))
    }
    pub fn stop_recording(&mut self) {
        self.end_idx = Some(_CAPTURE_LOG.inner.lock().unwrap().logs.len());
    }
}

pub fn get_handle() -> CaplogHandle {
    CaplogHandle {
        start_idx: _CAPTURE_LOG.inner.lock().unwrap().logs.len(),
        end_idx: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use log::info;

    #[test]
    fn simple_contains() {
        let mut handle = get_handle();
        let present = "present msg";
        let not_present = "not present in messages";
        info!("{}", present);
        handle.stop_recording();
        info!("{}", not_present);
        assert!(handle.any_msg_contains(present));
        assert!(!handle.any_msg_contains(not_present));
    }
}
