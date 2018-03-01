use ::std::collections::VecDeque;
use ::std::cmp::min;
use ::std::slice;
use ::std::ops::*;

use ::range_tracker::*;

pub struct TrimmingBuffer<'a>(Box<Option<TrimmingBufferImpl<'a>>>);

struct TrimmingBufferImpl<'a> {
    inner: VecDeque<u8>,
    first_available: usize,
    del_tracker: RangeTracker<'a, u8>
}

impl<'a> TrimmingBuffer<'a> {
    pub fn new(size: usize) -> TrimmingBuffer<'a> {
        let mut ret = TrimmingBuffer(Box::new(Some(TrimmingBufferImpl {
            inner: VecDeque::with_capacity(size),
            first_available: 0,
            del_tracker: unsafe { ::std::mem::uninitialized() }
        })));

        {
            let theret = (*ret.0).as_mut().unwrap();
            let tracker = unsafe {
                let ptr = &theret.inner as *const VecDeque<u8>;
                RangeTracker::new(ptr.as_ref().unwrap())
            };
            theret.del_tracker = tracker;
        }

        ret
    }

    pub fn add<T>(&mut self, data: T) where T: Into<VecDeque<u8>> {
        let theself = (*self.0).as_mut().unwrap();
        let mut vddata = data.into();
        assert!(theself.inner.len().checked_add(vddata.len()).is_some());
        theself.inner.append(&mut vddata);
    }

    pub fn add_cloning<T>(&mut self, data: T) where T: AsRef<[u8]> {
        let theself = (*self.0).as_mut().unwrap();
        let dataref = data.as_ref();
        assert!(theself.inner.len().checked_add(dataref.len()).is_some());
        theself.inner.extend(dataref[..].iter());
    }

    pub fn get_space_left(&self) -> usize {
        let theself = (*self.0).as_ref().unwrap();
        theself.inner.capacity() - theself.inner.len()
    }

    pub fn get_available(&self) -> usize {
        let theself = (*self.0).as_ref().unwrap();
        theself.inner.len() - theself.first_available
    }

    pub fn take(&mut self, size: usize) -> Option<TrimmingBufferSlice<'a>> {
        let len = min(self.get_available(), size);
        if len == 0 {
            return None;
        }

        let mut theself = self.0.take().unwrap();

        let ret = {
            let tracker_ptr = &mut theself.del_tracker
                as *mut RangeTracker<'a, u8>;
            let (beginning, ending) = theself.inner.as_slices();
            let beg_len = beginning.len();
            if theself.first_available < beg_len {
                if theself.first_available + len <= beg_len {
                    let ind = theself.first_available;
                    TrimmingBufferSlice::Direct {
                        tracker: tracker_ptr,
                        start: beginning[ind .. ind + 1].as_ptr(),
                        len: len
                    }
                } else {
                    let mut ret = Vec::with_capacity(len);
                    let beg_slice = &beginning[theself.first_available..];
                    ret.extend_from_slice(beg_slice);
                    let ending_len = len - beg_slice.len();
                    let end_slice = &ending[0..ending_len];
                    ret.extend_from_slice(end_slice);

                    theself.del_tracker.track_slice(beg_slice);
                    theself.del_tracker.track_slice(end_slice);

                    TrimmingBufferSlice::Owning(ret.into())
                }
            } else {
                let ind = theself.first_available - beg_len;
                TrimmingBufferSlice::Direct {
                    tracker: tracker_ptr,
                    start: ending[ind .. ind + 1].as_ptr(),
                    len: len
                }
            }
        };

        *self.0 = Some(theself);

        Some(ret)
    }

    pub fn cleanup(&mut self) {
        let mut theself = self.0.take().unwrap();
        if let Some(ind) = theself.del_tracker.take_range() {
            theself.inner.drain(0 .. ind + 1);
        }
        *self.0 = Some(theself)
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
