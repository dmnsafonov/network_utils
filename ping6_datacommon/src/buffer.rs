use ::std::collections::VecDeque;
use ::std::cmp::min;
use ::std::marker::PhantomData;
use ::std::slice;
use ::std::ops::*;

use ::IRange;
use ::range_tracker::*;

pub struct TrimmingBuffer<'a>(Box<TrimmingBufferImpl<'a>>);

struct TrimmingBufferImpl<'a> {
    inner: VecDeque<u8>,
    first_available: usize,
    del_tracker: RangeTracker<'a, u8>
}

impl<'a> TrimmingBuffer<'a> {
    pub fn new(size: usize) -> TrimmingBuffer<'a> {
        let mut ret = TrimmingBuffer(Box::new(TrimmingBufferImpl {
            inner: VecDeque::with_capacity(size),
            first_available: 0,
            del_tracker: unsafe { ::std::mem::uninitialized() }
        }));

        let tracker = unsafe {
            let ptr = &ret.0.inner as *const VecDeque<u8>;
            RangeTracker::new(ptr.as_ref().unwrap())
        };
        ret.0.del_tracker = tracker;

        ret
    }

    pub fn add<T>(&mut self, data: T) where T: AsRef<[u8]> {
        let dataref = data.as_ref();
        assert!(self.0.inner.len().checked_add(dataref.len()).is_some());
        self.0.inner.extend(dataref[..].iter());
    }

    pub fn add_slicing<T>(&mut self, data: T) -> TrimmingBufferSlice<'a>
            where T: AsRef<[u8]> {
        let first_ind = self.0.inner.len() - 1;
        let dataref = data.as_ref();
        self.add(dataref);
        let last_ind = self.0.inner.len() - 1;
        self.take_range(IRange(first_ind, last_ind))
    }

    pub fn get_space_left(&self) -> usize {
        self.0.inner.capacity() - self.0.inner.len()
    }

    pub fn get_available(&self) -> usize {
        self.0.inner.len() - self.0.first_available
    }

    pub fn take(&'a mut self, size: usize) -> Option<TrimmingBufferSlice<'a>> {
        let len = min(self.get_available(), size);
        if len == 0 {
            return None;
        }

        let first = self.0.first_available;

        Some(self.take_range(IRange(first, first + len - 1)))
    }

    fn take_range(&mut self, range: IRange<usize>) -> TrimmingBufferSlice<'a> {
        let ilen = self.0.inner.len();
        let len = range.len();
        debug_assert!(range.0 < ilen && range.1 < ilen);
        debug_assert!(
            !self.0.del_tracker.is_range_tracked(range).unwrap_or(true)
        );
        debug_assert!(self.0.first_available <= range.0);
        debug_assert!(len > 0);

        let mut ranges_to_free = None;
        let ret = {
            let tracker_ptr = &mut self.0.del_tracker
                as *mut RangeTracker<'a, u8>;
            let (beginning, ending) = self.0.inner.as_slices();
            let beg_len = beginning.len();
            if range.0 < beg_len {
                if range.0 + len <= beg_len {
                    let ind = range.0;
                    TrimmingBufferSlice::Direct {
                        tracker: tracker_ptr,
                        start: beginning[ind .. ind + 1].as_ptr(),
                        len: len,
                        _phantom: Default::default()
                    }
                } else {
                    let mut ret = Vec::with_capacity(len);
                    let beg_slice = &beginning[range.0..];
                    ret.extend_from_slice(beg_slice);
                    let ending_len = len - beg_slice.len();
                    let end_slice = &ending[0..ending_len];
                    ret.extend_from_slice(end_slice);

                    ranges_to_free = Some((
                        self.0.del_tracker.slice_to_range(beg_slice),
                        self.0.del_tracker.slice_to_range(end_slice)
                    ));

                    TrimmingBufferSlice::Owning(ret.into())
                }
            } else {
                let ind = range.0 - beg_len;
                TrimmingBufferSlice::Direct {
                    tracker: tracker_ptr,
                    start: ending[ind .. ind + 1].as_ptr(),
                    len: len,
                    _phantom: Default::default()
                }
            }
        };

        if let Some((one, two)) = ranges_to_free {
            self.0.del_tracker.track_range(one);
            self.0.del_tracker.track_range(two);
        }

        ret
    }

    pub fn cleanup(&mut self) {
        let ind = self.0.del_tracker.take_range();
        if let Some(ind) = ind {
            self.0.inner.drain(0 .. ind + 1);
        }
    }
}

pub enum TrimmingBufferSlice<'a> {
    Direct {
        tracker: *mut RangeTracker<'a, u8>,
        start: *const u8,
        len: usize,
        _phantom: PhantomData<&'a [u8]>
    },
    Owning(Box<[u8]>)
}

impl<'a> Drop for TrimmingBufferSlice<'a> {
    fn drop(&mut self) {
        if let &mut TrimmingBufferSlice::Direct { tracker, start, len, .. }
                = self { unsafe {
            let tr = tracker.as_mut().unwrap();
            tr.track_slice(slice::from_raw_parts(start, len));
        }}
    }
}

impl<'a> Deref for TrimmingBufferSlice<'a> {
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
