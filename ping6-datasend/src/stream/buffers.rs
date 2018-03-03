use ::std::cell::*;
use ::std::collections::vec_deque;
use ::std::collections::VecDeque;
use ::std::iter::*;
use ::std::marker::PhantomData;
use ::std::mem::uninitialized;
use ::std::num::Wrapping;
use ::std::ops::*;
use ::std::rc::Rc;

use ::owning_ref::*;

use ::ping6_datacommon::*;

pub struct AckWaitlist(Rc<RefCell<AckWaitlistImpl>>);

struct AckWaitlistImpl {
    inner: VecDeque<AckWait>,
    del_tracker: RangeTracker<AckWaitlistImplBufferGetter, AckWait>,
    tmpvec: RefCell<Vec<IRange<u32>>>
}

pub struct AckWait {
    pub seqno: Wrapping<u16>,
    pub data: TrimmingBufferSlice
}

#[derive(Clone)]
struct AckWaitlistImplBufferGetter(Rc<RefCell<AckWaitlistImpl>>);

impl<'a> RangeTrackerParentHandle<'a, AckWait>
        for AckWaitlistImplBufferGetter {
    type Borrowed = RefRef<'a, AckWaitlistImpl, VecDeque<AckWait>>;
    fn borrow(&'a self) -> Self::Borrowed {
        RefRef::new(self.0.borrow()).map(|x| &x.inner)
    }
}

impl AckWaitlist {
    pub fn new(window_size: u32) -> AckWaitlist {
        assert!(window_size <= ::std::u16::MAX as u32 + 1);
        let ret = AckWaitlist(Rc::new(RefCell::new(AckWaitlistImpl {
            inner: VecDeque::with_capacity(window_size as usize),
            del_tracker: unsafe { uninitialized() },
            tmpvec: RefCell::new(Vec::with_capacity(window_size as usize))
        })));
        ret.0.borrow_mut().del_tracker = RangeTracker::new_with_parent(
            AckWaitlistImplBufferGetter(ret.0.clone())
        );
        ret
    }

    pub fn add(&mut self, wait: AckWait) {
        let mut theself = self.0.borrow_mut();
        debug_assert!(theself.inner.is_empty()
            || wait.seqno > theself.inner.back().unwrap().seqno
            || (theself.inner.back().unwrap().seqno.0 == ::std::u16::MAX
                && wait.seqno.0 == 0));
        assert!(theself.inner.capacity() - theself.inner.len() > 0);
        theself.inner.push_back(wait);
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
        let theself = self.0.borrow();

        assert!(range.0 <= range.1);
        let tmpvecref = theself.tmpvec.clone();
        let mut tmpvec = tmpvecref.borrow_mut();
        debug_assert!(tmpvec.is_empty());

        {
            let mut peekable = Self::iter_from_borrow(Ref::clone(&theself))
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
        drop(theself);

        let mut theself = self.0.borrow_mut();
        for i in tmpvec.drain(..) {
            theself.del_tracker.track_range(i.into());
        }
    }

    pub fn cleanup(&mut self) {
        let mut theself = self.0.borrow_mut();
        if let Some(ind) = theself.del_tracker.take_range() {
            theself.inner.drain(0 .. ind as usize + 1);
        }
    }

    pub fn iter<'a>(&'a self) -> AckWaitlistIterator<'a> {
        AckWaitlistIterator(OwningHandle::new_with_fn(self.0.clone(),
            |x| unsafe { DerefWrapper(
                Self::iter_from_borrow(x.as_ref().unwrap().borrow())
            ) }
        ))
    }

    fn iter_from_borrow<'a>(borrow: Ref<'a, AckWaitlistImpl>)
    -> AckWaitlistIteratorInternal<'a> {
        AckWaitlistIteratorInternal(OwningHandle::new_with_fn(borrow,
            |x| {
                let y = unsafe { x.as_ref().unwrap() };
                DerefWrapper(AckWaitlistIteratorInternalImpl {
                    tracker_iter: y.del_tracker.iter().peekable(),
                    inner: y.inner.iter().enumerate(),
                    _phantom: Default::default()
                })
            }
        ))
    }
}

struct DerefWrapper<T>(T);

impl<T> Deref for DerefWrapper<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.0
    }
}

impl<T> DerefMut for DerefWrapper<T> {
    fn deref_mut(&mut self) -> &mut T {
        &mut self.0
    }
}

struct AckWaitlistIteratorInternal<'a>(
    OwningHandle<
        Ref<'a, AckWaitlistImpl>,
        DerefWrapper<AckWaitlistIteratorInternalImpl<'a>>
    >
);

struct AckWaitlistIteratorInternalImpl<'a> {
    tracker_iter: Peekable<RangeTrackerIterator<
        'a,
        AckWaitlistImplBufferGetter,
        AckWait>
    >,
    inner: Enumerate<vec_deque::Iter<'a, AckWait>>,
    _phantom: PhantomData<&'a AckWait>
}

impl<'a> Iterator for AckWaitlistIteratorInternal<'a> {
    type Item = (u32, &'a AckWait);

    fn next(&mut self) -> Option<Self::Item> {
        let mut acked_range_opt = self.0.tracker_iter.peek().cloned();
        while let Some((ind, wait)) = self.0.inner.next() {
            while acked_range_opt.is_some()
                    && acked_range_opt.unwrap().1 < ind {
                self.0.tracker_iter.next();
                acked_range_opt = self.0.tracker_iter.peek().cloned();
            }
            if let Some(acked_range) = acked_range_opt {
                if acked_range.contains_point(ind) {
                    continue;
                }

                return Some((ind as u32, wait));
            }
        }

        return None;
    }
}

pub struct AckWaitlistIterator<'a>(
    OwningHandle<
        Rc<RefCell<AckWaitlistImpl>>,
        DerefWrapper<AckWaitlistIteratorInternal<'a>>
    >
);

impl<'a> Iterator for AckWaitlistIterator<'a> {
    type Item = &'a AckWait;

    fn next(&mut self) -> Option<Self::Item> {
        self.0.next().map(|(_,x)| x)
    }
}
