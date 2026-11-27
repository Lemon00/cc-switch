//! Per-database change notifications. Persistence does not select cloud backends.
//!
//! Table notifications are conservative dirty signals from SQLite's update hook,
//! not committed domain events: a later rollback can still produce a signal.
//! Observers must not query this connection, and must defer snapshot reads until
//! after the database lock is released. Full replacements emit a separate signal.

pub trait DatabaseChangeObserver: Send + Sync {
    fn table_changed(&self, table: &str);
    fn database_replaced(&self);
}

pub struct NoOpDatabaseChangeObserver;

impl DatabaseChangeObserver for NoOpDatabaseChangeObserver {
    fn table_changed(&self, _table: &str) {}
    fn database_replaced(&self) {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::Database;
    use std::sync::{Arc, Mutex};

    #[derive(Default)]
    struct Recorder(Mutex<Vec<String>>);

    impl DatabaseChangeObserver for Recorder {
        fn table_changed(&self, table: &str) {
            self.0.lock().unwrap().push(table.into());
        }

        fn database_replaced(&self) {
            self.0.lock().unwrap().push("replaced".into());
        }
    }

    #[test]
    fn notifications_belong_to_the_database_instance() {
        let first = Arc::new(Recorder::default());
        let second = Arc::new(Recorder::default());
        let first_db = Database::memory_with_observer(first.clone()).unwrap();
        let second_db = Database::memory_with_observer(second.clone()).unwrap();
        first.0.lock().unwrap().clear();
        second.0.lock().unwrap().clear();

        first_db.set_setting("observer-test", "first").unwrap();
        assert_eq!(*first.0.lock().unwrap(), ["settings"]);
        assert!(second.0.lock().unwrap().is_empty());
        second_db.set_setting("observer-test", "second").unwrap();
        assert_eq!(*second.0.lock().unwrap(), ["settings"]);
        assert_eq!(*first.0.lock().unwrap(), ["settings"]);
        assert_eq!(
            first_db.get_setting("observer-test").unwrap().as_deref(),
            Some("first")
        );
    }

    #[test]
    fn rollback_signals_dirty_tables_but_never_a_completed_replacement() {
        let recorder = Arc::new(Recorder::default());
        let db = Database::memory_with_observer(recorder.clone()).unwrap();
        recorder.0.lock().unwrap().clear();
        {
            let conn = db.conn.lock().unwrap();
            conn.execute_batch("BEGIN; INSERT INTO settings (key, value) VALUES ('rolled-back', 'value'); ROLLBACK;").unwrap();
        }
        assert_eq!(db.get_setting("rolled-back").unwrap(), None);
        assert_eq!(*recorder.0.lock().unwrap(), ["settings"]);
    }
}
