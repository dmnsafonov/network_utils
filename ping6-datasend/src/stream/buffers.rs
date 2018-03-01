use ::std::cell::RefCell;
use ::std::collections::vec_deque;
use ::std::collections::VecDeque;
use ::std::iter::*;
use ::std::mem::uninitialized;
use ::std::num::Wrapping;

use ::ping6_datacommon::*;

pub struct AckWaitlist<'a> {
    inner: VecDeque<AckWait<'a>>,
    del_tracker: RangeTracker<'a, AckWait<'a>>,
    tmpvec: RefCell<Vec<IRange<u32>>>
}

pub struct AckWait<'a> {
    pub seqno: Wrapping<u16>,
    pub data: TrimmingBufferSlice<'a>
}

impl<'a> AckWaitlist<'a> {
    pub fn new(window_size: u32) -> Box<AckWaitlist<'a>> {
        assert!(window_size <= ::std::u16::MAX as u32 + 1);
        let mut ret = Box::new(AckWaitlist {
            inner: VecDeque::with_capacity(window_size as usize),
            del_tracker: unsafe { uninitialized() },
            tmpvec: RefCell::new(Vec::with_capacity(window_size as usize))
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
            self.remove_non_wrapping(IRange(range.0,
                Wrapping(::std::u16::MAX)));
            self.remove_non_wrapping(IRange(Wrapping(0), range.1));
        } else {
            self.remove_non_wrapping(range);
        }
    }

    // safe to call multiple times with the same arguments
    pub fn remove_non_wrapping(&mut self, range: IRange<Wrapping<u16>>) {
        assert!(range.0 <= range.1);
        let mut tmpvec = self.tmpvec.borrow_mut();
        debug_assert!(tmpvec.is_empty());

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
                        tmpvec.push(IRange(start_ind, ind));
                        start_ind = ind;
                    }
                    curr_seqno = seqno;
                    last_ind = ind;
                });
                tmpvec.push(IRange(start_ind, last_ind));
            }
        }
        for i in tmpvec.drain(..) {
            self.del_tracker.track_range(i.into());
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
                    && acked_range_opt.unwrap().1 < ind {
                self.tracker_iter.next();
                acked_range_opt = self.tracker_iter.peek().cloned();
            }
            if let Some(acked_range) = acked_range_opt {
                if acked_range.contains(ind) {
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
