extern crate owning_ref;

use std::cell::*;
use std::ops::*;
use std::rc::Rc;
use std::sync::*;

use owning_ref::*;

type Range = std::ops::Range<usize>;

#[derive(Clone)]
pub struct SRcRef<T> {
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

    pub fn into_borrow<'a>(self) -> OwningSRcRefBorrow<'a, T> {
        OwningSRcRefBorrow {
            borrow: OwningHandle::new_with_fn(RcRef::new(self.inner),
                |x| unsafe {
                    x.as_ref().unwrap().borrow()
                }
            ),
            range: self.range
        }
    }
}

impl<T> SRcRef<T> where T: IndexMut<Range> {
    pub fn borrow_mut(&self) -> RefMutRefMut<T, <T as Index<Range>>::Output> {
        RefMutRefMut::new(self.inner.borrow_mut())
            .map_mut(|x| x.index_mut(self.range.clone()))
    }

    pub fn into_borrow_mut<'a>(self) -> OwningSRcRefMutBorrow<'a, T> {
        OwningSRcRefMutBorrow {
            borrow: OwningHandle::new_with_fn(RcRef::new(self.inner),
                |x| unsafe {
                    x.as_ref().unwrap().borrow_mut()
                }
            ),
            range: self.range
        }
    }
}

// useful because it is impossible implement Deref for SRcRef
// to use OwningHandle
pub struct OwningSRcRefBorrow<'a, T> where T: 'a {
    borrow: OwningHandle<RcRef<RefCell<T>>, Ref<'a, T>>,
    range: Range
}

impl<'a, T> Deref for OwningSRcRefBorrow<'a, T> where T: Index<Range> {
    type Target = <T as Index<Range>>::Output;
    fn deref(&self) -> &Self::Target {
        self.borrow.index(self.range.clone())
    }
}

impl<'a, T> AsRef<<T as Index<Range>>::Output> for OwningSRcRefBorrow<'a, T>
        where T: Index<Range> {
    fn as_ref(&self) -> &<T as Index<Range>>::Output {
        &**self
    }
}

// useful because it is impossible implement Deref for SRcRef
// to use OwningHandle
pub struct OwningSRcRefMutBorrow<'a, T> where T: 'a {
    borrow: OwningHandle<RcRef<RefCell<T>>, RefMut<'a, T>>,
    range: Range
}

impl<'a, T> Deref for OwningSRcRefMutBorrow<'a, T> where T: Index<Range> {
    type Target = <T as Index<Range>>::Output;
    fn deref(&self) -> &Self::Target {
        self.borrow.index(self.range.clone())
    }
}

impl<'a, T> DerefMut for OwningSRcRefMutBorrow<'a, T>
        where T: IndexMut<Range> {
    fn deref_mut(&mut self) -> &mut <Self as Deref>::Target {
        self.borrow.index_mut(self.range.clone())
    }
}

impl<'a, T> AsMut<<T as Index<Range>>::Output> for OwningSRcRefMutBorrow<'a, T>
        where T: IndexMut<Range> {
    fn as_mut(&mut self) -> &mut <T as Index<Range>>::Output {
        &mut **self
    }
}

#[derive(Clone)]
pub struct SArcRef<T> {
    inner: Arc<Mutex<T>>,
    range: Range
}

impl<T> SArcRef<T> where T: Index<Range> {
    pub fn new(x: T, r: Range) -> SArcRef<T> {
        assert!(r.start <= r.end);
        SArcRef {
            inner: Arc::new(Mutex::new(x)),
            range: r
        }
    }

    pub fn from_arcref(x: Arc<Mutex<T>>, r: Range) -> SArcRef<T> {
        assert!(r.start <= r.end);
        SArcRef {
            inner: x,
            range: r
        }
    }

    pub fn range(&self, r: Range) -> SArcRef<T> {
        assert!(r.start <= r.end);
        let self_len = self.range.end - self.range.start;
        let len = r.end - r.start;
        assert!(r.start + len < self_len);
        let new_range = (self.range.start + r.start)
            .. (self.range.start + len);

        SArcRef {
            inner: self.inner.clone(),
            range: new_range
        }
    }

    pub fn lock(&self) -> MutexGuardRef<T, <T as Index<Range>>::Output> {
        MutexGuardRef::new(self.inner.lock().unwrap())
            .map(|x| x.index(self.range.clone()))
    }

    pub fn into_lock<'a>(self) -> OwningSArcRefBorrow<'a, T> {
        OwningSArcRefBorrow {
            lock: OwningHandle::new_with_fn(ArcRef::new(self.inner),
                |x| unsafe {
                    x.as_ref().unwrap().lock().unwrap()
                }
            ),
            range: self.range
        }
    }
}

impl<T> SArcRef<T> where T: IndexMut<Range> {
    pub fn borrow_mut(&self)
            -> MutexGuardRefMut<T, <T as Index<Range>>::Output> {
        MutexGuardRefMut::new(self.inner.lock().unwrap())
            .map_mut(|x| x.index_mut(self.range.clone()))
    }

    pub fn into_borrow_mut<'a>(self) -> OwningSArcRefMutBorrow<'a, T> {
        OwningSArcRefMutBorrow {
            lock: OwningHandle::new_with_fn(ArcRef::new(self.inner),
                |x| unsafe {
                    x.as_ref().unwrap().lock().unwrap()
                }
            ),
            range: self.range
        }
    }
}

// useful because it is impossible implement Deref for SRcRef
// to use OwningHandle
pub struct OwningSArcRefBorrow<'a, T> where T: 'a {
    lock: OwningHandle<ArcRef<Mutex<T>>, MutexGuard<'a, T>>,
    range: Range
}

impl<'a, T> Deref for OwningSArcRefBorrow<'a, T> where T: Index<Range> {
    type Target = <T as Index<Range>>::Output;
    fn deref(&self) -> &Self::Target {
        self.lock.index(self.range.clone())
    }
}

impl<'a, T> AsRef<<T as Index<Range>>::Output> for OwningSArcRefBorrow<'a, T>
        where T: Index<Range> {
    fn as_ref(&self) -> &<T as Index<Range>>::Output {
        &**self
    }
}

// useful because it is impossible implement Deref for SRcRef
// to use OwningHandle
pub struct OwningSArcRefMutBorrow<'a, T> where T: 'a {
    lock: OwningHandle<ArcRef<Mutex<T>>, MutexGuard<'a, T>>,
    range: Range
}

impl<'a, T> Deref for OwningSArcRefMutBorrow<'a, T> where T: Index<Range> {
    type Target = <T as Index<Range>>::Output;
    fn deref(&self) -> &Self::Target {
        self.lock.index(self.range.clone())
    }
}

impl<'a, T> DerefMut for OwningSArcRefMutBorrow<'a, T>
        where T: IndexMut<Range> {
    fn deref_mut(&mut self) -> &mut <Self as Deref>::Target {
        self.lock.index_mut(self.range.clone())
    }
}

impl<'a, T> AsMut<<T as Index<Range>>::Output> for OwningSArcRefMutBorrow<'a, T>
        where T: IndexMut<Range> {
    fn as_mut(&mut self) -> &mut <T as Index<Range>>::Output {
        &mut **self
    }
}
