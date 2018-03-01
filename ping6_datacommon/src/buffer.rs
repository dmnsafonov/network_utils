use ::std::collections::VecDeque;
use ::std::cmp::min;
use ::std::slice;
use ::std::ops::*;

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

    pub fn add<T>(&mut self, data: T) where T: Into<VecDeque<u8>> {
        let mut vddata = data.into();
        assert!(self.0.inner.len().checked_add(vddata.len()).is_some());
        self.0.inner.append(&mut vddata);
    }

    pub fn get_space_left(&self) -> usize {
        self.0.inner.capacity() - self.0.inner.len()
    }

    pub fn get_available(&self) -> usize {
        self.0.inner.len() - self.0.first_available
    }

    pub fn take(&mut self, size: usize) -> Option<TrimmingBufferSlice<'a>> {
        let len = min(self.get_available(), size);
        if len == 0 {
            return None;
        }

        let mut ranges_to_free = None;
        let ret = {
            let tracker_ptr = &mut self.0.del_tracker
                as *mut RangeTracker<'a, u8>;
            let (beginning, ending) = self.0.inner.as_slices();
            let beg_len = beginning.len();
            if self.0.first_available < beg_len {
                if self.0.first_available + len <= beg_len {
                    let ind = self.0.first_available;
                    TrimmingBufferSlice::Direct {
                        tracker: tracker_ptr,
                        start: beginning[ind .. ind + 1].as_ptr(),
                        len: len
                    }
                } else {
                    let mut ret = Vec::with_capacity(len);
                    let beg_slice = &beginning[self.0.first_available..];
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
                let ind = self.0.first_available - beg_len;
                TrimmingBufferSlice::Direct {
                    tracker: tracker_ptr,
                    start: ending[ind .. ind + 1].as_ptr(),
                    len: len
                }
            }
        };

        if let Some((one, two)) = ranges_to_free {
            self.0.del_tracker.track_range(one);
            self.0.del_tracker.track_range(two);
        }

        Some(ret)
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
        len: usize
    },
    Owning(Box<[u8]>)
}

impl<'a> Drop for TrimmingBufferSlice<'a> {
    fn drop(&mut self) {
        if let &mut TrimmingBufferSlice::Direct { tracker, start, len }
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
