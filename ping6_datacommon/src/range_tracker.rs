use ::std::collections::*;
use ::std::cmp::Ordering;
use ::std::marker::PhantomData;
use ::std::ops::Deref;

use ::IRange;

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
    #[allow(clippy::cast_possible_wrap)]
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

            if offset_range.0 != 0 && i.1 == offset_range.0 - 1 {
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
    #[allow(clippy::cast_possible_wrap)]
    pub fn take_range(&mut self) -> Option<usize> {
        if self.rangeset.is_empty() {
            return None;
        }

        let offset_first = self._get_first();
        let first = offset_first.offset(-(self.offset as isize));
        if first.0 == 0 {
            self.rangeset.remove(&offset_first);
            self.offset = self.offset.checked_add(first.1 + 1)
                .expect("no overflow");
            Some(first.1)
        } else {
            None
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

    pub fn iter(&self) -> RangeTrackerIterator<P, E> {
        self.into_iter()
    }

    pub fn is_empty(&self) -> bool {
        self.rangeset.is_empty()
    }
}

pub struct NoParent;
pub struct NoElement;

impl RangeTracker<NoParent, NoElement> {
    #[allow(clippy::default_trait_access)]
    pub fn new() -> Self {
        Self {
            tracked: NoParent,
            rangeset: BTreeSet::new(),
            offset: 0,
            _phantom: Default::default()
        }
    }
}

impl Default for RangeTracker<NoParent, NoElement> {
    fn default() -> Self {
        Self::new()
    }
}

impl<P, E> RangeTracker<P, E>
        where for<'a> P: RangeTrackerParentHandle<'a, E> {
    #[allow(clippy::default_trait_access)]
    pub fn new_with_parent(tracked: P) -> Self {
        Self {
            tracked,
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
            let start = ptr - beg_ptr;
            let end = start + len - 1;
            IRange(start, end)
        } else {
            assert!(is_subslice(ending, slice));
            let end_ptr = ending.as_ptr() as usize;
            let start = ptr - end_ptr + beginning.len();
            let end = start + len - 1;
            IRange(start, end)
        }
    }

    pub fn is_slice_tracked(&self, slice: &[E]) -> Option<bool> {
        let range = self.slice_to_range(slice);
        self.is_range_tracked(range)
    }
}

#[allow(clippy::cast_sign_loss)]
fn is_subslice<T>(slice: &[T], sub: &[T]) -> bool {
    assert!(!slice.is_empty());
    assert!(!sub.is_empty());
    assert!(slice.len() <= isize::max_value() as usize);
    assert!(sub.len() <= isize::max_value() as usize);

    let slice_start = slice.as_ptr();
    let slice_end = slice[(slice.len() - 1) .. slice.len()].as_ptr();
    let sub_start = sub.as_ptr();
    let sub_end = sub[(sub.len() - 1) .. sub.len()].as_ptr();

    sub_start >= slice_start && sub_end <= slice_end
}

impl<'a, P, E> IntoIterator for &'a RangeTracker<P, E> {
    type Item = <RangeTrackerIterator<'a, P, E> as Iterator>::Item;
    type IntoIter = RangeTrackerIterator<'a, P, E>;

    #[allow(clippy::default_trait_access)]
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

    #[allow(clippy::cast_possible_wrap)]
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
    #[allow(clippy::cast_sign_loss, clippy::cast_possible_wrap)]
    fn offset(&self, off: isize) -> Self {
        DTRange (
            ((self.0 as isize).checked_add(off)
                .expect("no overflow")) as usize,
            ((self.1 as isize).checked_add(off)
                .expect("no overflow")) as usize
        )
    }
}

impl PartialOrd for DTRange {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        assert!(self.0 <= self.1);
        assert!(other.0 <= other.1);
        if self == other {
            Some(Ordering::Equal)
        } else if self.0 > other.1 {
            Some(Ordering::Greater)
        } else if self.1 < other.0 {
            Some(Ordering::Less)
        } else {
            None
        }
    }
}

impl Ord for DTRange {
    fn cmp(&self, other: &Self) -> Ordering {
        self.partial_cmp(other).expect("nonoverlapping ranges")
    }
}

macro_rules! gen_dtrange_from_irange {
    ( $t:ty ) => (
        impl From<IRange<$t>> for DTRange {
            #[allow(clippy::cast_possible_truncation)]
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

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn empty_not_iterable() {
        let tracker = RangeTracker::new();
        assert!(tracker.iter().next().is_none());
    }

    #[test]
    fn one_range_track() {
        let mut tracker = RangeTracker::new();
        tracker.track_range(IRange(0usize, 5usize));
        let mut iter = tracker.iter();
        assert_eq!(iter.next().unwrap(), IRange(0usize, 5usize));
        assert!(iter.next().is_none());
    }

    #[test]
    fn merge_right() {
        let mut tracker = RangeTracker::new();
        tracker.track_range(IRange(0usize, 5usize));
        tracker.track_range(IRange(6usize, 10usize));
        let mut iter = tracker.iter();
        assert_eq!(iter.next().unwrap(), IRange(0usize, 10usize));
        assert!(iter.next().is_none());
    }

    #[test]
    fn merge_left() {
        let mut tracker = RangeTracker::new();
        tracker.track_range(IRange(6usize, 10usize));
        tracker.track_range(IRange(0usize, 5usize));
        let mut iter = tracker.iter();
        assert_eq!(iter.next().unwrap(), IRange(0usize, 10usize));
        assert!(iter.next().is_none());
    }

    #[test]
    fn merge_both_sides() {
        let mut tracker = RangeTracker::new();
        tracker.track_range(IRange(8usize, 10usize));
        tracker.track_range(IRange(0usize, 3usize));
        tracker.track_range(IRange(4usize, 7usize));
        let mut iter = tracker.iter();
        assert_eq!(iter.next().unwrap(), IRange(0usize, 10usize));
        assert!(iter.next().is_none());
    }

    #[test]
    fn iterate_multiple_ranges() {
        let mut tracker = RangeTracker::new();
        tracker.track_range(IRange(1usize, 10usize));
        tracker.track_range(IRange(300usize, 500usize));
        tracker.track_range(IRange(15usize, 30usize));
        let mut iter = tracker.iter();
        assert_eq!(iter.next().unwrap(), IRange(1usize, 10usize));
        assert_eq!(iter.next().unwrap(), IRange(15usize, 30usize));
        assert_eq!(iter.next().unwrap(), IRange(300usize, 500usize));
        assert!(iter.next().is_none());
    }

    #[test]
    fn take() {
        let mut tracker = RangeTracker::new();
        tracker.track_range(IRange(0usize, 10usize));
        tracker.track_range(IRange(300usize, 500usize));
        tracker.track_range(IRange(15usize, 30usize));
        assert_eq!(tracker.take_range().unwrap(), 10);
        let mut iter = tracker.iter();
        assert_eq!(iter.next().unwrap(), IRange(4usize, 19usize));
        assert_eq!(iter.next().unwrap(), IRange(289usize, 489usize));
        assert!(iter.next().is_none());
    }

    #[test]
    fn take_last() {
        let mut tracker = RangeTracker::new();
        tracker.track_range(IRange(0usize, 10usize));
        tracker.track_range(IRange(300usize, 500usize));
        assert_eq!(tracker.take_range().unwrap(), 10);
        tracker.track_range(IRange(0usize, 288usize));
        assert_eq!(tracker.take_range().unwrap(), 489);
        assert!(tracker.iter().next().is_none());
    }

    #[test]
    fn is_range_tracked() {
        let mut tracker = RangeTracker::new();
        tracker.track_range(IRange(0usize, 10usize));
        tracker.track_range(IRange(300usize, 500usize));
        tracker.track_range(IRange(15usize, 30usize));
        assert!(tracker.is_range_tracked(IRange(0usize, 0usize)).unwrap());
        assert!(tracker.is_range_tracked(IRange(10usize, 11usize)).is_none());
        assert!(tracker.is_range_tracked(IRange(9usize, 301usize)).is_none());
        assert!(!tracker.is_range_tracked(IRange(11usize, 12usize)).unwrap());
    }

    #[test]
    fn track_size_one() {
        let mut tracker = RangeTracker::new();
        tracker.track_range(IRange(0usize, 10usize));
        tracker.track_range(IRange(14usize, 20usize));
        tracker.track_range(IRange(11usize, 11usize));
        tracker.track_range(IRange(13usize, 13usize));
        tracker.track_range(IRange(12usize, 12usize));
        let mut iter = tracker.iter();
        assert_eq!(iter.next().unwrap(), IRange(0usize, 20usize));
    }

    // TODO: test slices
}
