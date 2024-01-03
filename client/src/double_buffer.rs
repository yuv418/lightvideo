use log::debug;
use parking_lot::{
    lock_api::{RwLockReadGuard, RwLockWriteGuard},
    RawRwLock, RwLock,
};

pub struct Frame {
    pub buffer: Vec<u8>,
    pub width: usize,
    pub height: usize,
}

pub struct DoubleBuffer {
    back: RwLock<Option<Frame>>,
    front: RwLock<Option<Frame>>,
}

impl DoubleBuffer {
    pub fn new(capacity: usize, width: usize, height: usize) -> Self {
        Self {
            back: RwLock::new(Some(Frame {
                buffer: vec![0; capacity],
                width,
                height,
            })),
            front: RwLock::new(Some(Frame {
                buffer: vec![0; capacity],
                width,
                height,
            })),
        }
    }

    pub fn new_uninitialized() -> Self {
        Self {
            back: RwLock::new(None),
            front: RwLock::new(None),
        }
    }

    pub fn initialize(&self, capacity: usize, width: usize, height: usize) {
        *self.back.write() = Some(Frame {
            buffer: vec![0; capacity],
            width,
            height,
        });
        *self.front.write() = Some(Frame {
            buffer: vec![0; capacity],
            width,
            height,
        });
    }

    pub fn uninitialized(&self) -> bool {
        self.back.read().is_none()
    }

    // mutable reference to back
    pub fn back(&self) -> Option<RwLockWriteGuard<'_, RawRwLock, Option<Frame>>> {
        self.back.try_write()
    }

    pub fn swap(&self) {
        let mut back_mut = self.back.write();
        let mut front_mut = self.front.write();

        debug!(
            "before: back_mut {:?} front_mut {:?}",
            &back_mut.as_mut().unwrap().buffer[0..20],
            &front_mut.as_mut().unwrap().buffer[0..20]
        );

        std::mem::swap(&mut *back_mut, &mut *front_mut);
    }

    // immutable reference to front
    pub fn front(&self) -> RwLockReadGuard<'_, RawRwLock, Option<Frame>> {
        self.front.read()
    }
}
