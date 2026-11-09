//! Single-instance guard backed by the `single-instance` crate.

use anyhow::{Context, Result};

/// Holds the process-wide single-instance lock until dropped.
pub struct SingleInstance {
    inner: single_instance::SingleInstance,
}

impl SingleInstance {
    /// Acquires a named instance lock.
    ///
    /// A second process with the same name receives `is_single() == false`.
    pub fn acquire(name: &str) -> Result<Self> {
        let inner = single_instance::SingleInstance::new(name)
            .with_context(|| format!("failed to acquire single-instance lock `{name}`"))?;
        Ok(Self { inner })
    }

    /// Returns whether this process owns the lock.
    pub fn is_single(&self) -> bool {
        self.inner.is_single()
    }
}
