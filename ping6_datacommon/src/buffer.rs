use ::std::cell::RefCell;
use ::std::collections::VecDeque;
use ::std::cmp::min;
use ::std::slice;
use ::std::ops::*;
use ::std::rc::Rc;

use ::owning_ref::RefRef;

use ::IRange;
use ::range_tracker::*;

// destructor must run after all slices' destructors
pub struct TrimmingBuffer(Rc<RefCell<TrimmingBufferImpl>>);

pub struct TrimmingBufferImpl {
    inner: VecDeque<u8>,
    first_available: usize,
    del_tracker: RangeTracker<TrimmingBufferImplBufferGetter, u8>
}

#[derive(Clone)]
struct TrimmingBufferImplBufferGetter(Rc<RefCell<TrimmingBufferImpl>>);

impl<'a> RangeTrackerParentHandle<'a, u8> for TrimmingBufferImplBufferGetter {
    type Borrowed = RefRef<'a, TrimmingBufferImpl, VecDeque<u8>>;
    fn borrow(&'a self) -> Self::Borrowed {
        RefRef::new(self.0.borrow()).map(|x| &x.inner)
    }
}

impl TrimmingBuffer {
    pub fn new(size: usize) -> TrimmingBuffer {
        let ret = TrimmingBuffer(Rc::new(RefCell::new(TrimmingBufferImpl {
            inner: VecDeque::with_capacity(size),
            first_available: 0,
            del_tracker: unsafe { ::std::mem::uninitialized() }
        })));
        ret.0.borrow_mut().del_tracker = RangeTracker::new(
            TrimmingBufferImplBufferGetter(ret.0.clone())
        );

        ret
    }

    pub fn add<T>(&mut self, data: T) where T: AsRef<[u8]> {
        let mut theself = self.0.borrow_mut();
        let dataref = data.as_ref();
        assert!(theself.inner.len().checked_add(dataref.len()).is_some());
        theself.inner.extend(dataref[..].iter());
    }

    pub fn add_slicing<T>(&mut self, data: T) -> TrimmingBufferSlice
            where T: AsRef<[u8]> {
        let range = {
            let first_ind = self.0.borrow().inner.len() - 1;
            let dataref = data.as_ref();
            self.add(dataref);
            let last_ind = self.0.borrow().inner.len() - 1;
            IRange(first_ind, last_ind)
        };
        self.take_range(range)
    }

    pub fn get_space_left(&self) -> usize {
        let theself = self.0.borrow();
        theself.inner.capacity() - theself.inner.len()
    }

    pub fn get_available(&self) -> usize {
        let theself = self.0.borrow();
        theself.inner.len() - theself.first_available
    }

    pub fn take(&mut self, size: usize) -> Option<TrimmingBufferSlice> {
        let len = min(self.get_available(), size);
        if len == 0 {
            return None;
        }

        let first = self.0.borrow().first_available;

        Some(self.take_range(IRange(first, first + len - 1)))
    }

    fn take_range(&mut self, range: IRange<usize>) -> TrimmingBufferSlice {
        let mut theself = self.0.borrow_mut();

        let ilen = theself.inner.len();
        let len = range.len();
        debug_assert!(range.0 < ilen && range.1 < ilen);
        debug_assert!(
            !theself.del_tracker.is_range_tracked(range).unwrap_or(true)
        );
        debug_assert!(theself.first_available <= range.0);
        debug_assert!(len > 0);

        let mut ranges_to_free = None;
        let ret = {
            let (beginning, ending) = theself.inner.as_slices();
            let beg_len = beginning.len();
            if range.0 < beg_len {
                if range.0 + len <= beg_len {
                    let ind = range.0;
                    TrimmingBufferSlice::Direct {
                        parent: self.0.clone(),
                        start: beginning[ind .. ind + 1].as_ptr(),
                        len: len
                    }
                } else {
                    let mut ret = Vec::with_capacity(len);
                    let beg_slice = &beginning[range.0..];
                    ret.extend_from_slice(beg_slice);
                    let ending_len = len - beg_slice.len();
                    let end_slice = &ending[0..ending_len];
                    ret.extend_from_slice(end_slice);

                    ranges_to_free = Some((
                        theself.del_tracker.slice_to_range(beg_slice),
                        theself.del_tracker.slice_to_range(end_slice)
                    ));

                    TrimmingBufferSlice::Owning(ret.into())
                }
            } else {
                let ind = range.0 - beg_len;
                TrimmingBufferSlice::Direct {
                    parent: self.0.clone(),
                    start: ending[ind .. ind + 1].as_ptr(),
                    len: len
                }
            }
        };

        if let Some((one, two)) = ranges_to_free {
            theself.del_tracker.track_range(one);
            theself.del_tracker.track_range(two);
        }

        ret
    }

    pub fn cleanup(&mut self) {
        let mut theself = self.0.borrow_mut();
        let ind = theself.del_tracker.take_range();
        if let Some(ind) = ind {
            theself.inner.drain(0 .. ind + 1);
        }
    }
}

pub enum TrimmingBufferSlice {
    Direct {
        parent: Rc<RefCell<TrimmingBufferImpl>>,
        start: *const u8,
        len: usize
    },
    Owning(Box<[u8]>)
}

impl Drop for TrimmingBufferSlice {
    fn drop(&mut self) {
        if let &mut TrimmingBufferSlice::Direct { ref parent, start, len, .. }
                = self { unsafe {
            let mut borrow = parent.borrow_mut();
            borrow.del_tracker.track_slice(slice::from_raw_parts(start, len));
        }}
    }
}

impl Deref for TrimmingBufferSlice {
    type Target = [u8];
    fn deref(&self) -> &Self::Target {
        match *self {
            TrimmingBufferSlice::Direct { start, len, .. } => { unsafe {
                slice::from_raw_parts(start, len)
            }},
            TrimmingBufferSlice::Owning(ref boxed) => boxed.as_ref()
        }
    }
}
