#[macro_use]
extern crate lazy_static;

use std::sync::Arc;

use log;

mod stable_list;
use stable_list::StableList;

lazy_static! {
    static ref _CAPTURE_LOG: Box<Caplog> = {
        let handler = Box::new(Caplog {
           logs: Arc::new(StableList::new())
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

pub struct CaplogHandle {
    list: Arc<StableList<Record>>,
}

impl CaplogHandle {
    pub fn any_msg_contains(&self, snippet: &str) -> bool {
        self.list.iter().any(|rec| dbg!(rec).msg.contains(snippet))
    }
}

pub fn get_handle() -> CaplogHandle {
    CaplogHandle {
        list: _CAPTURE_LOG.logs.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use log::info;

    #[test]
    fn simple_contains() {
        let handle = get_handle();
        let num_recs = handle.list.len();
        info!("logging message");
        assert!(handle.list.len() > num_recs);
        assert!(handle.any_msg_contains("logging message"));
    }
}
