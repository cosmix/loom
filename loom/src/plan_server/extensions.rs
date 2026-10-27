//! Typed slots later plans hang their own state on, one value per type.

use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::sync::{Arc, PoisonError, RwLock};

type Slot = Arc<dyn Any + Send + Sync>;

/// One shared value per type, set at `install` time and read by routes and hooks.
#[derive(Default)]
pub struct Extensions {
    slots: RwLock<HashMap<TypeId, Slot>>,
}

impl Extensions {
    /// Store `value`, replacing any earlier value of the same type.
    pub fn insert<T: Any + Send + Sync>(&self, value: Arc<T>) {
        self.slots
            .write()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(TypeId::of::<T>(), value);
    }

    /// The value stored for `T`, if any.
    pub fn get<T: Any + Send + Sync>(&self) -> Option<Arc<T>> {
        let slot = self
            .slots
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .get(&TypeId::of::<T>())
            .cloned()?;
        slot.downcast::<T>().ok()
    }
}
