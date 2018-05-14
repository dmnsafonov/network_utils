use ::std::collections::VecDeque;
use ::std::cmp::min;
use ::std::mem::*;
use ::std::ops::*;
use ::std::slice;
use ::std::sync::*;

use ::IRange;
use ::range_tracker::*;

// destructor must run after all slices' destructors
#[derive(Clone)]
pub struct TrimmingBuffer(Arc<RwLock<TrimmingBufferImpl>>);
unsafe impl Send for TrimmingBuffer {}
unsafe impl Sync for TrimmingBuffer {}

struct TrimmingBufferImpl {
    inner: VecDeque<u8>,
    first_available: usize,
    del_tracker: RangeTracker<TrimmingBufferImplBufferGetter, u8>
}

#[derive(Clone)]
struct TrimmingBufferImplBufferGetter(
    Arc<RwLock<TrimmingBufferImpl>>,
    *const TrimmingBufferImpl
);

impl<'a> RangeTrackerParentHandle<'a, u8> for TrimmingBufferImplBufferGetter {
    type Borrowed = &'a VecDeque<u8>;
    fn borrow(&'a self) -> Self::Borrowed {
        // safe, because we already have a lock in the calling method
        unsafe {
            &self.1.as_ref().unwrap().inner
        }
    }
}

impl TrimmingBuffer {
    pub fn new(size: usize) -> TrimmingBuffer {
        let ret = TrimmingBuffer(Arc::new(RwLock::new(TrimmingBufferImpl {
            inner: VecDeque::with_capacity(size),
            first_available: 0,
            del_tracker: unsafe { uninitialized() }
        })));

        {
            let mut lock = ret.0.write().unwrap();
            let ptr = (&*lock) as *const TrimmingBufferImpl;
            forget(replace(
                &mut lock.del_tracker,
                RangeTracker::new_with_parent(
                    TrimmingBufferImplBufferGetter(ret.0.clone(), ptr)
                )
            ));
        }

        ret
    }

    pub fn add<T>(&mut self, data: T) where T: AsRef<[u8]> {
        let mut theself = self.0.write().unwrap();
        let dataref = data.as_ref();
        let res_len = theself.inner.len().checked_add(dataref.len()).unwrap();
        assert!(res_len <= theself.inner.capacity());
        theself.inner.extend(dataref[..].iter());
    }

    pub fn add_slicing<T>(&mut self, data: T) -> TrimmingBufferSlice
            where T: AsRef<[u8]> {
        let range = {
            let first_ind = self.0.read().unwrap().inner.len();
            let dataref = data.as_ref();
            self.add(dataref);
            let last_ind = self.0.read().unwrap().inner.len() - 1;
            IRange(first_ind, last_ind)
        };
        self.take_range(range)
    }

    pub fn get_space_left(&self) -> usize {
        let theself = self.0.read().unwrap();
        theself.inner.capacity() - theself.inner.len()
    }

    pub fn get_available(&self) -> usize {
        let theself = self.0.read().unwrap();
        theself.inner.len() - theself.first_available
    }

    pub fn take(&mut self, size: usize) -> Option<TrimmingBufferSlice> {
        let len = min(self.get_available(), size);
        if len == 0 {
            return None;
        }

        let first = self.0.read().unwrap().first_available;

        Some(self.take_range(IRange(first, first + len - 1)))
    }

    fn take_range(&mut self, range: IRange<usize>) -> TrimmingBufferSlice {
        let mut theself = self.0.write().unwrap();

        let ilen = theself.inner.len();
        let len = range.len();
        debug_assert!(range.0 < ilen && range.1 < ilen);
        debug_assert!(
            !theself.del_tracker.is_range_tracked(range).unwrap_or(true)
        );
        debug_assert!(theself.first_available <= range.0);
        debug_assert!(len > 0);

        theself.first_available += len;

        let mut ranges_to_free = None;
        let ret = {
            let (beginning, ending) = theself.inner.as_slices();
            let beg_len = beginning.len();
            let inner = if range.0 < beg_len {
                if range.0 + len <= beg_len {
                    let ind = range.0;
                    TrimmingBufferSliceImpl::Direct {
                        parent: self.0.clone(),
                        start: beginning[ind ..= ind].as_ptr(),
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

                    TrimmingBufferSliceImpl::Owning(ret.into())
                }
            } else {
                let ind = range.0 - beg_len;
                TrimmingBufferSliceImpl::Direct {
                    parent: self.0.clone(),
                    start: ending[ind ..= ind].as_ptr(),
                    len: len
                }
            };
            TrimmingBufferSlice(inner)
        };

        if let Some((one, two)) = ranges_to_free {
            theself.del_tracker.track_range(one);
            theself.del_tracker.track_range(two);
        }

        ret
    }

    pub fn cleanup(&mut self) {
        let mut theself = self.0.write().unwrap();
        if let Some(i) = theself.del_tracker.take_range() {
            theself.inner.drain(0 ..= i);
            theself.first_available -= i + 1;
        }
    }
}

pub struct TrimmingBufferSlice(TrimmingBufferSliceImpl);
unsafe impl Send for TrimmingBufferSlice {}
unsafe impl Sync for TrimmingBufferSlice {}
unsafe impl ::owning_ref::StableAddress for TrimmingBufferSlice {}

enum TrimmingBufferSliceImpl {
    Direct {
        parent: Arc<RwLock<TrimmingBufferImpl>>,
        start: *const u8,
        len: usize
    },
    Owning(Box<[u8]>)
}

impl Drop for TrimmingBufferSlice {
    fn drop(&mut self) {
        if let TrimmingBufferSliceImpl::Direct { ref parent, start, len, .. }
                = self.0 { unsafe {
            let mut borrow = parent.write().unwrap();
            borrow.del_tracker.track_slice(slice::from_raw_parts(start, len));
        }}
    }
}

impl Deref for TrimmingBufferSlice {
    type Target = [u8];
    fn deref(&self) -> &Self::Target {
        match self.0 {
            TrimmingBufferSliceImpl::Direct { start, len, .. } => unsafe {
                slice::from_raw_parts(start, len)
            },
            TrimmingBufferSliceImpl::Owning(ref boxed) => boxed.as_ref()
        }
    }
}

impl AsRef<[u8]> for TrimmingBufferSlice {
    fn as_ref(&self) -> &[u8] {
        &*self
    }
}
