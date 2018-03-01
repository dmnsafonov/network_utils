use super::*;

use ::std::collections::*;
use ::std::cmp::Ordering;

#[derive(Debug)]
pub struct RangeTracker<'a, E> where E: 'a {
    tracked: &'a VecDeque<E>,

    // at no point should two overlapping ranges be insert()'ed into the set
    rangeset: BTreeSet<DTRange>,

    // we return the VecDeque's buffer indices, but we do not increment
    // the rangeset range indicies each deletion, so we need to track how much
    // we are off
    offset: usize
}

impl<'a, E> RangeTracker<'a, E> {
    pub fn new(tracked: &'a VecDeque<E>) -> RangeTracker<'a, E> {
        RangeTracker {
            tracked: tracked,
            rangeset: BTreeSet::new(),
            offset: 0
        }
    }

    pub fn track_slice(&mut self, newslice: &[E]) {
        let range = self.slice_to_range(newslice);
        self.track_range(range);
    }

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

    pub fn slice_to_range(&self, slice: &[E]) -> IRange<usize> {
        let len = slice.len();
        let ptr = slice.as_ptr() as usize;
        let (beginning, ending) = self.tracked.as_slices();

        if is_subslice(beginning, slice) {
            let beg_ptr = beginning.as_ptr() as usize;
            let start = beg_ptr - ptr;
            let end = start + len - 1;
            IRange(start, end)
        } else {
            debug_assert!(is_subslice(ending, slice));
            let end_ptr = ending.as_ptr() as usize;
            let start = end_ptr - ptr + beginning.len();
            let end = start + len - 1;
            IRange(start, end)
        }
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

    fn _get_first(&self) -> DTRange {
        *self.rangeset.iter().next().expect("nonempty range set")
    }

    pub fn iter<'b>(&'b self) -> RangeTrackerIterator<'a, 'b, E> where 'a: 'b {
        self.into_iter()
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

impl<'a, 'b, E> IntoIterator for &'b RangeTracker<'a, E> where E: 'a, 'a: 'b {
    type Item = <RangeTrackerIterator<'a, 'b, E> as Iterator>::Item;
    type IntoIter = RangeTrackerIterator<'a, 'b, E>;

    fn into_iter(self) -> Self::IntoIter {
        RangeTrackerIterator {
            parent: self,
            inner: self.rangeset.iter()
        }
    }
}

pub struct RangeTrackerIterator<'a, 'b, E> where E: 'a, 'a: 'b {
    parent: &'b RangeTracker<'a, E>,
    inner: ::std::collections::btree_set::Iter<'b, DTRange>
}

impl<'a, 'b, E> Iterator for RangeTrackerIterator<'a, 'b, E>
        where E: 'a, 'a: 'b {
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

impl From<IRange<usize>> for DTRange where {
    fn from(r: IRange<usize>) -> DTRange {
        DTRange(r.0, r.1)
    }
}

impl From<IRange<u64>> for DTRange where {
    fn from(r: IRange<u64>) -> DTRange {
        DTRange(r.0 as usize, r.1 as usize)
    }
}

impl From<IRange<u32>> for DTRange where {
    fn from(r: IRange<u32>) -> DTRange {
        DTRange(r.0 as usize, r.1 as usize)
    }
}

impl From<IRange<u16>> for DTRange where {
    fn from(r: IRange<u16>) -> DTRange {
        DTRange(r.0 as usize, r.1 as usize)
    }
}
