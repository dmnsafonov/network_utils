use ::std::cmp::*;
use ::std::collections::vec_deque;
use ::std::collections::VecDeque;
use ::std::iter::*;
use ::std::mem::uninitialized;
use ::std::num::Wrapping;
use ::std::ops::*;
use ::std::slice;

use ::ping6_datacommon::*;

#[derive(Debug)]
pub struct WindowedBuffer<'a> {
    inner: VecDeque<u8>,
    window_size: u32,
    first_available: u16,
    del_tracker: RangeTracker<'a, u8>
}

impl<'a> WindowedBuffer<'a> {
    pub fn new(size: usize, window_size: u32) -> Box<WindowedBuffer<'a>> {
        assert!(window_size <= ::std::u16::MAX as u32 + 1);

        let mut ret = Box::new(WindowedBuffer {
            inner: VecDeque::with_capacity(size),
            window_size: window_size,
            first_available: 0,
            del_tracker: unsafe { uninitialized() }
        });
        ret.del_tracker = unsafe {
            let ptr = &ret.inner as *const VecDeque<u8>;
            RangeTracker::new(ptr.as_ref().unwrap())
        };
        ret
    }

    pub fn add<T>(&mut self, data: T) where T: Into<VecDeque<u8>> {
        let mut vddata = data.into();
        assert!(self.inner.len().checked_add(vddata.len()).is_some());
        self.inner.append(&mut vddata);
    }

    pub fn add_cloning<T>(&mut self, data: T) where T: AsRef<[u8]> {
        let dataref = data.as_ref();
        assert!(self.inner.len().checked_add(dataref.len()).is_some());
        self.inner.extend(dataref[..].iter());
    }

    pub fn get_space_left(&self) -> usize {
        self.inner.capacity() - self.inner.len()
    }

    // availability is moot beyond the current window,
    // so value returned is restrained by the window size
    pub fn get_available(&self) -> u32 {
        let ret = min(self.inner.len() - self.first_available as usize,
            self.window_size as usize - self.first_available as usize);
        debug_assert!(ret <= ::std::u16::MAX as usize + 1);
        ret as u32
    }

    pub fn take(&mut self, size: u32) -> Option<WindowedBufferSlice<'a>> {
        assert!(size <= ::std::u16::MAX as u32 + 1);

        let len = min(self.get_available(), size);
        if len == 0 {
            return None;
        }

        let tracker_ptr = &mut self.del_tracker
            as *mut RangeTracker<'a, u8>;
        let (beginning, ending) = self.inner.as_slices();
        let beg_len = beginning.len();
        Some(if (self.first_available as usize) < beg_len {
            if self.first_available as usize + len as usize <= beg_len {
                WindowedBufferSlice::Direct {
                    tracker: tracker_ptr,
                    start: unsafe {
                        beginning.as_ptr()
                            .offset(self.first_available as isize)
                    },
                    len: len
                }
            } else {
                let mut ret = Vec::with_capacity(len as usize);
                let beg_slice = &beginning[self.first_available as usize..];
                ret.extend_from_slice(beg_slice);
                let ending_len = len as usize - beg_slice.len();
                let end_slice = &ending[0..ending_len];
                ret.extend_from_slice(end_slice);

                self.del_tracker.track_slice(beg_slice);
                self.del_tracker.track_slice(end_slice);

                WindowedBufferSlice::Owning(ret.into())
            }
        } else { unsafe {
            WindowedBufferSlice::Direct {
                tracker: tracker_ptr,
                start: ending.as_ptr()
                    .offset((self.first_available as usize - beg_len) as isize),
                len: len
            }
        }})
    }

    pub fn cleanup(&mut self) {
        if let Some(ind) = self.del_tracker.take_range() {
            self.inner.drain(0 .. ind as usize + 1);
        }
    }
}

pub enum WindowedBufferSlice<'a> {
    Direct {
        tracker: *mut RangeTracker<'a, u8>,
        start: *const u8,
        len: u32
    },
    Owning(Box<[u8]>)
}

impl<'a> Drop for WindowedBufferSlice<'a> {
    fn drop(&mut self) {
        if let &mut WindowedBufferSlice::Direct { tracker, start, len }
                = self { unsafe {
            let tr = tracker.as_mut().unwrap();
            tr.track_slice(slice::from_raw_parts(start, len as usize));
        }}
    }
}

impl<'a> Deref for WindowedBufferSlice<'a> {
    type Target = [u8];
    fn deref(&self) -> &Self::Target {
        match *self {
            WindowedBufferSlice::Direct { start, len, .. } => { unsafe {
                slice::from_raw_parts(start, len as usize)
            }},
            WindowedBufferSlice::Owning(ref boxed) => boxed.as_ref()
        }
    }
}

pub struct AckWaitlist<'a> {
    inner: VecDeque<AckWait<'a>>,
    del_tracker: RangeTracker<'a, AckWait<'a>>
}

pub struct AckWait<'a> {
    pub seqno: Wrapping<u16>,
    pub data: WindowedBufferSlice<'a>
}

impl<'a> AckWaitlist<'a> {
    pub fn new(window_size: u32) -> Box<AckWaitlist<'a>> {
        assert!(window_size <= ::std::u16::MAX as u32 + 1);
        let mut ret = Box::new(AckWaitlist {
            inner: VecDeque::with_capacity(window_size as usize),
            del_tracker: unsafe { uninitialized() }
        });
        ret.del_tracker = unsafe {
            let ptr = &mut ret.inner as *mut VecDeque<AckWait<'a>>;
            RangeTracker::new(ptr.as_mut().unwrap())
        };
        ret
    }

    pub fn add(&mut self, wait: AckWait<'a>) {
        debug_assert!(self.inner.is_empty()
            || wait.seqno > self.inner.back().unwrap().seqno
            || (self.inner.back().unwrap().seqno.0 == ::std::u16::MAX
                && wait.seqno.0 == 0));
        assert!(self.inner.capacity() - self.inner.len() > 0);
        self.inner.push_back(wait);
    }

    // safe to call multiple times with the same arguments
    // and with overlapping ranges
    pub fn remove(&mut self, range: IRange<Wrapping<u16>>) {
        if range.0 < range.1 {
            self.remove_not_wrapping(IRange(range.0,
                Wrapping(::std::u16::MAX)));
            self.remove_not_wrapping(IRange(Wrapping(0), range.1));
        } else {
            self.remove_not_wrapping(range);
        }
    }

    // safe to call multiple times with the same arguments
    pub fn remove_not_wrapping(&mut self, range: IRange<Wrapping<u16>>) {
        assert!(range.0 <= range.1);

        let mut ranges_to_delete =
            Vec::with_capacity(((range.1).0 - (range.0).0) as usize);
        {
            let mut peekable = self.iter().0
                .map(|(ind,x)| (ind as u32, x))
                .skip_while(|&(_,x)| x.seqno < range.0
                    || x.seqno > range.1)
                .take_while(|&(_,x)| x.seqno >= range.0
                    && x.seqno <= range.1)
                .peekable();
            if let Some(&(first_ind, first_ackwait)) = peekable.peek() {
                let mut start_ind = first_ind;
                // wrapping is for the case of range.start: 0
                let mut curr_seqno = first_ackwait.seqno - Wrapping(1);
                let mut last_ind = 0;
                peekable.for_each(|(ind, &AckWait { seqno, .. })| {
                    if curr_seqno + Wrapping(1) != seqno {
                        ranges_to_delete.push(IRange(start_ind, ind));
                        start_ind = ind;
                    }
                    curr_seqno = seqno;
                    last_ind = ind;
                });
                ranges_to_delete.push(IRange(start_ind, last_ind));
            }
        }
        for i in ranges_to_delete {
            self.del_tracker.track_range(i);
        }
    }

    pub fn cleanup(&mut self) {
        if let Some(ind) = self.del_tracker.take_range() {
            self.inner.drain(0 .. ind as usize + 1);
        }
    }

    pub fn iter<'b>(&'b self) -> AckWaitlistIterator<'a, 'b> where 'a: 'b {
        self.into_iter()
    }
}

impl<'a, 'b> IntoIterator for &'b AckWaitlist<'a> where 'a: 'b {
    type Item = &'b AckWait<'a>;
    type IntoIter = AckWaitlistIterator<'a, 'b>;

    fn into_iter(self) -> Self::IntoIter {
        AckWaitlistIterator(AckWaitlistIteratorInternal {
            tracker_iter: self.del_tracker.iter().peekable(),
            inner: self.inner.iter().enumerate()
        })
    }
}

struct AckWaitlistIteratorInternal<'a, 'b> where 'a: 'b {
    tracker_iter: Peekable<RangeTrackerIterator<'b, 'b, AckWait<'a>>>,
    inner: Enumerate<vec_deque::Iter<'b, AckWait<'a>>>
}

impl<'a, 'b> Iterator for AckWaitlistIteratorInternal<'a, 'b> where 'a: 'b {
    type Item = (u32, &'b AckWait<'a>);

    fn next(&mut self) -> Option<Self::Item> {
        let mut acked_range_opt = self.tracker_iter.peek().cloned();
        while let Some((ind, wait)) = self.inner.next() {
            while acked_range_opt.is_some()
                    && acked_range_opt.unwrap().1 < ind as u32 {
                self.tracker_iter.next();
                acked_range_opt = self.tracker_iter.peek().cloned();
            }
            if let Some(acked_range) = acked_range_opt {
                if acked_range.contains(ind as u32) {
                    continue;
                }

                return Some((ind as u32, wait));
            }
        }

        return None;
    }
}

pub struct AckWaitlistIterator<'a, 'b>(AckWaitlistIteratorInternal<'a, 'b>)
    where 'a: 'b;

impl<'a, 'b> Iterator for AckWaitlistIterator<'a, 'b> where 'a: 'b {
    type Item = &'b AckWait<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        self.0.next().map(|(_,x)| x)
    }
}
