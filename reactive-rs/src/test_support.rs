use std::fmt::Debug;
use std::sync::{Arc, Mutex};

use crate::{Listener, ValueListener};

/// Records listener calls, in order, across threads.
#[derive(Clone, Default)]
pub(crate) struct Log(Arc<Mutex<Vec<String>>>);

impl Log {
    pub(crate) fn push(&self, entry: impl Into<String>) {
        self.0.lock().unwrap().push(entry.into());
    }

    pub(crate) fn take(&self) -> Vec<String> {
        std::mem::take(&mut self.0.lock().unwrap())
    }

    pub(crate) fn listener(&self, name: &'static str) -> Listener {
        let log = self.clone();
        Arc::new(move || log.push(name))
    }

    pub(crate) fn value_listener<T: Debug + 'static>(&self) -> ValueListener<T> {
        let log = self.clone();
        Arc::new(move |v| log.push(format!("value {v:?}")))
    }
}

pub(crate) fn assert_send_sync<T: Send + Sync>() {}
