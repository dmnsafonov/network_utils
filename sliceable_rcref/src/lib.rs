extern crate owning_ref;

use std::cell::RefCell;
use std::ops::*;
use std::rc::Rc;

use owning_ref::*;

type Range = std::ops::Range<usize>;

#[derive(Clone)]
pub struct SRcRef<T> where T: Index<Range> {
    inner: Rc<RefCell<T>>,
    range: Range
}

impl<T> SRcRef<T> where T: Index<Range> {
    pub fn new(x: T, r: Range) -> SRcRef<T> {
        assert!(r.start <= r.end);
        SRcRef {
            inner: Rc::new(RefCell::new(x)),
            range: r
        }
    }

    pub fn from_rcref(x: Rc<RefCell<T>>, r: Range) -> SRcRef<T> {
        assert!(r.start <= r.end);
        SRcRef {
            inner: x,
            range: r
        }
    }

    pub fn range(&self, r: Range) -> SRcRef<T> {
        assert!(r.start <= r.end);
        let self_len = self.range.end - self.range.start;
        let len = r.end - r.start;
        assert!(r.start + len < self_len);
        let new_range = (self.range.start + r.start)
            .. (self.range.start + len);

        SRcRef {
            inner: self.inner.clone(),
            range: new_range
        }
    }

    pub fn borrow(&self) -> RefRef<T, <T as Index<Range>>::Output> {
        RefRef::new(self.inner.borrow())
            .map(|x| x.index(self.range.clone()))
    }
}

impl<T> SRcRef<T> where T: IndexMut<Range> {
    pub fn borrow_mut(&self) -> RefMutRefMut<T, <T as Index<Range>>::Output> {
        RefMutRefMut::new(self.inner.borrow_mut())
            .map_mut(|x| x.index_mut(self.range.clone()))
    }
}
