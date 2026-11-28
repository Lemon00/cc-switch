#[cfg(not(test))]
use std::sync::{Mutex, MutexGuard};

/// Lock a std `Mutex`, taking the guard even if a panic poisoned it. The poison
/// flag stays set; call `clear_poison` where the caller resets the value.
#[cfg(not(test))]
pub(crate) trait MutexExt<T: ?Sized> {
    fn lock_recover(&self) -> MutexGuard<'_, T>;
}

#[cfg(not(test))]
impl<T: ?Sized> MutexExt<T> for Mutex<T> {
    fn lock_recover(&self) -> MutexGuard<'_, T> {
        self.lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

fn read_optional(path: &std::path::Path) -> Result<Option<Vec<u8>>, crate::error::AppError> {
    match std::fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(crate::error::AppError::io(path, error)),
    }
}

pub fn write_and_commit<T>(
    path: &std::path::Path,
    write: impl FnOnce() -> Result<(), crate::error::AppError>,
    commit: impl FnOnce() -> Result<T, crate::error::AppError>,
) -> Result<T, crate::error::AppError> {
    let previous = read_optional(path)?;
    write()?;
    let written = read_optional(path)?;
    match commit() {
        Ok(result) => Ok(result),
        Err(error) => {
            if read_optional(path)? != written {
                return Err(crate::error::AppError::Message(format!(
                    "Config update failed ({error}); {} changed externally after the write and was preserved",
                    path.display()
                )));
            }
            let rollback = match previous {
                Some(bytes) => crate::config::atomic_write_private(path, &bytes),
                None => match std::fs::remove_file(path) {
                    Ok(()) => Ok(()),
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
                    Err(error) => Err(crate::error::AppError::io(path, error)),
                },
            };
            if let Err(rollback_error) = rollback {
                return Err(crate::error::AppError::Message(format!(
                    "Config update failed ({error}); restoring {} also failed: {rollback_error}",
                    path.display()
                )));
            }
            Err(error)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failed_commit_only_restores_the_contents_we_wrote() {
        for initially_present in [false, true] {
            for external_edit in [false, true] {
                let dir = tempfile::tempdir().unwrap();
                let path = dir.path().join("config.json");
                if initially_present {
                    std::fs::write(&path, "original").unwrap();
                }
                let result = write_and_commit(
                    &path,
                    || crate::config::atomic_write_private(&path, b"written"),
                    || -> Result<(), crate::error::AppError> {
                        if external_edit {
                            std::fs::write(&path, "external").unwrap();
                        }
                        Err(crate::error::AppError::Message("commit failed".into()))
                    },
                );
                assert!(result.is_err());
                let expected = if external_edit {
                    Some(b"external".to_vec())
                } else if initially_present {
                    Some(b"original".to_vec())
                } else {
                    None
                };
                assert_eq!(read_optional(&path).unwrap(), expected);
            }
        }
    }
}
