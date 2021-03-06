#![allow(unknown_lints)]
#![warn(bare_trait_objects)]
#![warn(clippy::pedantic)]

use std::ops::*;
use std::sync::atomic::*;

pub struct SendBox<T>(Box<T>) where T: ?Sized;
unsafe impl<T> Send for SendBox<T> where T: ?Sized {}

impl<T> SendBox<T> where T: ?Sized {
    pub unsafe fn new(x: Box<T>) -> Self {
        fence(Ordering::Release);
        SendBox(x)
    }

    pub fn into_box(self) -> Box<T> {
        fence(Ordering::Acquire);
        self.0
    }

    pub fn propagate(&self) {
        fence(Ordering::SeqCst);
    }
}

impl<T> Deref for SendBox<T> where T: ?Sized {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        fence(Ordering::SeqCst);
        &*self.0
    }
}

impl<T> DerefMut for SendBox<T> where T: ?Sized {
    fn deref_mut(&mut self) -> &mut <Self as Deref>::Target {
        fence(Ordering::SeqCst);
        &mut *self.0
    }
}
