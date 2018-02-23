extern crate owning_ref;

use std::cell::RefCell;
use std::ops::*;
use std::rc::Rc;

use owning_ref::*;

pub struct SRcRef<T,R> where T: Index<R> {
    inner: Rc<RefCell<T>>,
    range: R
}

impl<T, R> SRcRef<T,R> where T: Index<R>, R: Copy {
    pub fn new(x: T, r: R) -> SRcRef<T,R> {
        SRcRef {
            inner: Rc::new(RefCell::new(x)),
            range: r
        }
    }

    pub fn from_rcref(x: Rc<RefCell<T>>, r: R) -> SRcRef<T,R> {
        SRcRef {
            inner: x,
            range: r
        }
    }

    pub fn set_range(&mut self, r: R) {
        self.range = r;
    }

    pub fn get_range(&self) -> R {
        self.range.clone()
    }

    pub fn borrow(&self) -> RefRef<T, <T as Index<R>>::Output> {
        RefRef::new(self.inner.borrow())
            .map(|x| x.index(self.range))
    }
}

impl<T,R> SRcRef<T,R> where T: IndexMut<R>, R: Copy {
    pub fn borrow_mut(&self) -> RefMutRefMut<T, <T as Index<R>>::Output> {
        RefMutRefMut::new(self.inner.borrow_mut())
            .map_mut(|x| x.index_mut(self.range))
    }
}
