use log::debug;
use parking_lot::{
    lock_api::{RwLockReadGuard, RwLockWriteGuard},
    RawRwLock, RwLock,
};

pub struct DoubleBuffer {
    back: RwLock<Option<Vec<u8>>>,
    front: RwLock<Option<Vec<u8>>>,
}

impl DoubleBuffer {
    pub fn new(capacity: usize) -> Self {
        Self {
            back: RwLock::new(Some(vec![0; capacity])),
            front: RwLock::new(Some(vec![0; capacity])),
        }
    }

    pub fn new_uninitialized() -> Self {
        Self {
            back: RwLock::new(None),
            front: RwLock::new(None),
        }
    }

    pub fn initialize(&self, capacity: usize) {
        *self.back.write() = Some(vec![0; capacity]);
        *self.front.write() = Some(vec![0; capacity]);
    }

    pub fn uninitialized(&self) -> bool {
        self.back.read().is_none()
    }

    // mutable reference to back
    pub fn back(&self) -> Option<RwLockWriteGuard<'_, RawRwLock, Option<Vec<u8>>>> {
        self.back.try_write()
    }

    pub fn swap(&self) {
        let mut back_mut = self.back.write();
        let mut front_mut = self.front.write();

        debug!(
            "before: back_mut {:?} front_mut {:?}",
            &back_mut.as_mut().unwrap()[0..20],
            &front_mut.as_mut().unwrap()[0..20]
        );

        std::mem::swap(&mut *back_mut, &mut *front_mut);
    }

    // immutable reference to front
    pub fn front(&self) -> RwLockReadGuard<'_, RawRwLock, Option<Vec<u8>>> {
        self.front.read()
    }
}
