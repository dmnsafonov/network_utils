use super::*;

use ::std::collections::*;
use ::std::cmp::Ordering;
use ::std::marker::PhantomData;
use ::std::ops::Deref;

#[derive(Debug)]
pub struct RangeTracker<P, E> {
    tracked: P,

    // at no point should two overlapping ranges be insert()'ed into the set
    rangeset: BTreeSet<DTRange>,

    // we return the VecDeque's buffer indices, but we do not increment
    // the rangeset range indicies each deletion, so we need to track how much
    // we are off
    offset: usize,

    _phantom: PhantomData<[E]>
}

pub trait RangeTrackerParentHandle<'a, E>
        where Self::Borrowed: Deref<Target = VecDeque<E>> {
    type Borrowed;
    fn borrow(&'a self) -> Self::Borrowed;
}

impl<P, E> RangeTracker<P, E> {
    pub fn track_range<T>(&mut self, newrange: IRange<T>)
            where DTRange: From<IRange<T>>, T: Ord {
        assert!(newrange.0 <= newrange.1);

        let offset_range = DTRange::from(newrange)
            .offset(self.offset as isize);

        let mut merge_left = None;
        let mut merge_right = None;
        for i in &self.rangeset {
            // we expect the added ranges to not overlap with ones added
            // previously
            debug_assert!((i.0 < offset_range.0 && i.1 < offset_range.0)
                || (i.0 > offset_range.1 && i.1 > offset_range.1));

            if i.1 == offset_range.0 - 1 {
                merge_left = Some(*i);
            }
            if i.0 == offset_range.1 + 1 {
                merge_right = Some(*i);
            }
            if i.0 >= offset_range.1 {
                break;
            }
        }

        let new_l = match merge_left {
            Some(l) => {
                self.rangeset.remove(&l);
                l.0
            },
            None => offset_range.0
        } ;
        let new_r = match merge_right {
            Some(r) => {
                self.rangeset.remove(&r);
                r.1
            },
            None => offset_range.1
        };
        self.rangeset.insert(DTRange(new_l, new_r));
    }

    // consume (0 =.. take_range()) after the call
    pub fn take_range(&mut self) -> Option<usize> {
        if self.rangeset.is_empty() {
            return None;
        }

        let offset_first = self._get_first();
        let first = offset_first.offset(-(self.offset as isize));
        if first.0 != 0 {
            None
        } else {
            self.rangeset.remove(&offset_first);
            self.offset = self.offset.checked_add(first.1 + 1)
                .expect("no overflow");
            Some(first.1)
        }
    }

    // return None when the range is partially tracked
    pub fn is_range_tracked<T>(&self, range: IRange<T>)
            -> Option<bool> where DTRange: From<IRange<T>>, T: Ord {
        let DTRange(l,r) = range.into();
        let range = IRange(l,r);

        for i in self.iter() {
            if i.contains_range(range) {
                return Some(true);
            } else if i.intersects(range) {
                return None;
            }

            if i.1 < l {
                break;
            }
        }

        Some(false)
    }

    fn _get_first(&self) -> DTRange {
        *self.rangeset.iter().next().expect("nonempty range set")
    }

    pub fn iter<'a>(&'a self) -> RangeTrackerIterator<'a, P, E> {
        self.into_iter()
    }
}

pub struct NoParent;

impl<E> RangeTracker<NoParent, E> {
    pub fn new() -> RangeTracker<NoParent, E> {
        RangeTracker {
            tracked: NoParent,
            rangeset: BTreeSet::new(),
            offset: 0,
            _phantom: Default::default()
        }
    }
}

impl<P, E> RangeTracker<P, E>
        where for<'a> P: RangeTrackerParentHandle<'a, E> {
    pub fn new_with_parent(tracked: P) -> RangeTracker<P, E> {
        RangeTracker {
            tracked: tracked,
            rangeset: BTreeSet::new(),
            offset: 0,
            _phantom: Default::default()
        }
    }

    pub fn track_slice(&mut self, newslice: &[E]) {
        let range = self.slice_to_range(newslice);
        self.track_range(range);
    }

    pub fn slice_to_range(&self, slice: &[E]) -> IRange<usize> {
        let tracked = self.tracked.borrow();

        let len = slice.len();
        let ptr = slice.as_ptr() as usize;
        let (beginning, ending) = tracked.as_slices();

        if is_subslice(beginning, slice) {
            let beg_ptr = beginning.as_ptr() as usize;
            let start = beg_ptr - ptr;
            let end = start + len - 1;
            IRange(start, end)
        } else {
            assert!(is_subslice(ending, slice));
            let end_ptr = ending.as_ptr() as usize;
            let start = end_ptr - ptr + beginning.len();
            let end = start + len - 1;
            IRange(start, end)
        }
    }

    pub fn is_slice_tracked(&self, slice: &[E]) -> Option<bool> {
        let range = self.slice_to_range(slice);
        self.is_range_tracked(range)
    }
}

fn is_subslice<T>(slice: &[T], sub: &[T]) -> bool { unsafe {
    assert!(slice.len() > 0);
    assert!(sub.len() > 0);
    assert!(slice.len() <= ::std::isize::MAX as usize);
    assert!(sub.len() <= ::std::isize::MAX as usize);

    let slice_start = slice.as_ptr();
    let slice_end = slice_start.offset(slice.len() as isize - 1);
    let sub_start = sub.as_ptr();
    let sub_end = sub_start.offset(sub.len() as isize - 1);

    sub_start >= slice_start && sub_end <= slice_end
}}

impl<'a, P, E> IntoIterator for &'a RangeTracker<P, E> {
    type Item = <RangeTrackerIterator<'a, P, E> as Iterator>::Item;
    type IntoIter = RangeTrackerIterator<'a, P, E>;

    fn into_iter(self) -> Self::IntoIter {
        RangeTrackerIterator {
            parent: self,
            inner: self.rangeset.iter(),
            _phantom: Default::default()
        }
    }
}

pub struct RangeTrackerIterator<'a, P, E> where P: 'a, E: 'a {
    parent: &'a RangeTracker<P, E>,
    inner: ::std::collections::btree_set::Iter<'a, DTRange>,
    _phantom: PhantomData<&'a E>
}

impl<'a, P, E> Iterator for RangeTrackerIterator<'a, P, E> {
    type Item = IRange<usize>;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next().map(|x| {
            let DTRange(s,e) = x.offset(-(self.parent.offset as isize));
            IRange(s, e)
        })
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct DTRange(usize,usize);

impl DTRange {
    fn offset(&self, off: isize) -> DTRange {
        DTRange (
            ((self.0 as isize).checked_add(off)
                .expect("no overflow")) as usize,
            ((self.1 as isize).checked_add(off)
                .expect("no overflow")) as usize
        )
    }
}

impl PartialOrd for DTRange {
    fn partial_cmp(&self, other: &DTRange) -> Option<Ordering> {
        assert!(self.0 <= self.1);
        assert!(other.0 <= other.1);
        if self == other {
            Some(Ordering::Equal)
        } else {
            if self.0 > other.1 {
                Some(Ordering::Greater)
            } else if self.1 < other.0 {
                Some(Ordering::Less)
            } else {
                None
            }
        }
    }
}

impl Ord for DTRange {
    fn cmp(&self, other: &DTRange) -> Ordering {
        self.partial_cmp(other).expect("nonoverlapping ranges")
    }
}

macro_rules! gen_dtrange_from_irange {
    ( $t:ty ) => (
        impl From<IRange<$t>> for DTRange {
            fn from(r: IRange<$t>) -> DTRange {
                DTRange(r.0 as usize, r.1 as usize)
            }
        }
    );

    ( $t:ty, $( $ts:ty ),+ ) => (
        gen_dtrange_from_irange!($t);
        gen_dtrange_from_irange!( $( $ts ),+ );
    );
}

gen_dtrange_from_irange!(usize, u64, u32, u16, u8);
