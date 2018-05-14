use ::std::cell::*;
use ::std::collections::vec_deque;
use ::std::collections::VecDeque;
use ::std::iter::*;
use ::std::marker::PhantomData;
use ::std::mem::*;
use ::std::num::Wrapping;
use ::std::ops::Deref;
use ::std::sync::*;

use ::owning_ref::*;

use ::ping6_datacommon::*;

pub struct AckWaitlist(Arc<Mutex<AckWaitlistImpl>>);

struct AckWaitlistImpl {
    inner: VecDeque<AckWait>,
    del_tracker: RangeTracker<AckWaitlistImplBufferGetter, AckWait>,
    tmpvec: RefCell<Vec<IRange<u32>>>
}

pub struct AckWait {
    pub seqno: Wrapping<u16>,
    pub data: TrimmingBufferSlice
}

impl AckWait {
    pub fn new(seqno: Wrapping<u16>, data: TrimmingBufferSlice) -> AckWait {
        AckWait {
            seqno,
            data
        }
    }
}

#[derive(Clone)]
struct AckWaitlistImplBufferGetter(Arc<Mutex<AckWaitlistImpl>>);

impl<'a> RangeTrackerParentHandle<'a, AckWait>
        for AckWaitlistImplBufferGetter {
    type Borrowed = MutexGuardRef<'a, AckWaitlistImpl, VecDeque<AckWait>>;
    fn borrow(&'a self) -> Self::Borrowed {
        MutexGuardRef::new(self.0.lock().unwrap()).map(|x| &x.inner)
    }
}

impl AckWaitlist {
    pub fn new(window_size: u32, mtu: u16) -> AckWaitlist {
        assert!(window_size <= ::std::u16::MAX as u32 + 1);
        let ret = AckWaitlist(Arc::new(Mutex::new(AckWaitlistImpl {
            inner: VecDeque::with_capacity(
                window_size as usize * mtu as usize
            ),
            del_tracker: unsafe { uninitialized() },
            tmpvec: RefCell::new(Vec::with_capacity(window_size as usize))
        })));
        forget(replace(
            &mut ret.0.lock().unwrap().del_tracker,
            RangeTracker::new_with_parent(
                AckWaitlistImplBufferGetter(ret.0.clone())
            )
        ));
        ret
    }

    pub fn add(&mut self, wait: AckWait) {
        let mut theself = self.0.lock().unwrap();
        debug_assert!(theself.inner.is_empty()
            || wait.seqno > theself.inner.back().unwrap().seqno
            || (theself.inner.back().unwrap().seqno.0 == ::std::u16::MAX
                && wait.seqno.0 == 0));
        assert!(theself.inner.capacity() - theself.inner.len() > 0);
        theself.inner.push_back(wait);
    }

    // safe to call multiple times with the same arguments
    // and with overlapping ranges
    pub fn remove(&mut self, range: IRange<Wrapping<u16>>) -> bool {
        if range.0 > range.1 {
            self.remove_non_wrapping(IRange(range.0,
                    Wrapping(::std::u16::MAX)))
                && self.remove_non_wrapping(IRange(Wrapping(0), range.1))
        } else {
            self.remove_non_wrapping(range)
        }
    }

    // safe to call multiple times with the same arguments
    pub fn remove_non_wrapping(&mut self, range: IRange<Wrapping<u16>>)
            -> bool {
        let mut theself = self.0.lock().unwrap();

        assert!(range.0 <= range.1);

        let tmpvec_ref = theself.tmpvec.clone();
        let mut tmpvec = tmpvec_ref.borrow_mut();
        debug_assert!(tmpvec.is_empty());

        {
            let mut peekable = Self::iter_from_lock(
                    OwnOrBorrow::new_borrowed(&theself))
                .map(|(ind,x)| (ind as u32, x))
                .skip_while(|&(_,x)| x.seqno < range.0)
                .take_while(|&(_,x)| x.seqno <= range.1)
                .peekable();
            if let Some(&(first_ind, first_ackwait)) = peekable.peek() {
                let mut start_ind = first_ind;
                let mut curr_seqno = first_ackwait.seqno - Wrapping(1);
                let mut last_ind = 0;
                peekable.for_each(|(ind, &AckWait { seqno, .. })| {
                    if curr_seqno + Wrapping(1) != seqno {
                        tmpvec.push(IRange(start_ind, last_ind));
                        start_ind = ind;
                    }
                    curr_seqno = seqno;
                    last_ind = ind;
                });
                tmpvec.push(IRange(start_ind, last_ind));
            }
        }

        let mut removed_something = false;
        for i in tmpvec.drain(..) {
            removed_something = true;
            theself.del_tracker.track_range(i.into());
        }
        removed_something
    }

    pub fn cleanup(&mut self) {
        let mut theself = self.0.lock().unwrap();
        if let Some(ind) = theself.del_tracker.take_range() {
            theself.inner.drain(0 ..= ind as usize);
        }
    }

    // may return false 'false' if cleanup() was not called
    pub fn is_empty(&self) -> bool {
        let theself = self.0.lock().unwrap();
        theself.inner.is_empty()
    }

    pub fn is_full(&self) -> bool {
        let theself = self.0.lock().unwrap();
        theself.inner.capacity() == theself.inner.len()
    }

    pub fn first_seqno(&self) -> Option<Wrapping<u16>> {
        self.iter().next().map(|x| x.seqno)
    }

    pub fn iter<'a>(&'a self) -> AckWaitlistIterator<'a> {
        AckWaitlistIterator(OwningHandle::new_with_fn(self.0.clone(),
            |x| unsafe { DerefWrapper(
                Self::iter_from_lock(
                    OwnOrBorrow::new_owned(
                        x.as_ref().unwrap().lock().unwrap()
                    )
                )
            ) }
        ))
    }

    fn iter_from_lock<'a>(
        lock: OwnOrBorrow<'a, MutexGuard<'a, AckWaitlistImpl>>
    ) -> AckWaitlistIteratorInternal<'a> {
        AckWaitlistIteratorInternal(OwningHandle::new_with_fn(lock,
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

struct AckWaitlistIteratorInternal<'a>(
    OwningHandle<
        OwnOrBorrow<'a, MutexGuard<'a, AckWaitlistImpl>>,
        DerefWrapper<AckWaitlistIteratorInternalImpl<'a>>
    >
);

struct AckWaitlistIteratorInternalImpl<'a> {
    tracker_iter: Peekable<RangeTrackerIterator<
        'a,
        AckWaitlistImplBufferGetter,
        AckWait
    >>,
    inner: Enumerate<vec_deque::Iter<'a, AckWait>>,
    _phantom: PhantomData<&'a AckWait>
}

impl<'a> Iterator for AckWaitlistIteratorInternal<'a> {
    type Item = (u32, &'a AckWait);

    fn next(&mut self) -> Option<Self::Item> {
        let mut acked_range_opt = self.0.tracker_iter.peek().map(|x| *x);
        while let Some((ind, wait)) = self.0.inner.next() {
            while acked_range_opt.is_some()
                    && acked_range_opt.as_ref().unwrap().1 < ind {
                self.0.tracker_iter.next();
                acked_range_opt = self.0.tracker_iter.peek().map(|x| *x);
            }

            if let Some(acked_range) = acked_range_opt {
                if acked_range.contains_point(ind) {
                    continue;
                }
            }

            return Some((ind as u32, wait));
        }

        return None;
    }
}

pub struct AckWaitlistIterator<'a>(
    OwningHandle<
        Arc<Mutex<AckWaitlistImpl>>,
        DerefWrapper<AckWaitlistIteratorInternal<'a>>
    >
);

impl<'a> Iterator for AckWaitlistIterator<'a> {
    type Item = &'a AckWait;

    fn next(&mut self) -> Option<Self::Item> {
        self.0.next().map(|(_,x)| x)
    }
}

enum OwnOrBorrow<'a, T> where T: 'a {
    Own(T),
    Borrow(&'a T)
}

impl<'a, T> OwnOrBorrow<'a, T> {
    fn new_borrowed(x: &'a T) -> OwnOrBorrow<'a, T> {
        OwnOrBorrow::Borrow(x)
    }

    fn new_owned(x: T) -> OwnOrBorrow<'a, T> {
        OwnOrBorrow::Own(x)
    }
}

impl<'a, T> Deref for OwnOrBorrow<'a, T> where T: Deref {
    type Target = <T as Deref>::Target;
    fn deref(&self) -> &Self::Target {
        match self {
            OwnOrBorrow::Own(ref x) => x,
            OwnOrBorrow::Borrow(x) => x
        }
    }
}

unsafe impl<'a, T> StableAddress for OwnOrBorrow<'a, T>
    where T: StableAddress {}
