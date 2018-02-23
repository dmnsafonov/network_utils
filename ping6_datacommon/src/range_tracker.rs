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
        let len_usize = newslice.len();
        debug_assert!(len_usize <= ::std::u16::MAX as usize + 1);
        let len = len_usize as u32;
        let ptr = newslice.as_ptr() as usize;

        let (beginning, ending) = self.tracked.as_slices();
        let range = if is_subslice(beginning, newslice) {
                let beg_ptr = beginning.as_ptr() as usize;
                let start = (beg_ptr - ptr) as u32;
                let end = start + len - 1;
                IRange(start, end)
            } else {
                debug_assert!(is_subslice(ending, newslice));
                let end_ptr = ending.as_ptr() as usize;
                let start = (end_ptr - ptr + beginning.len()) as u32;
                let end = start + len - 1;
                IRange(start, end)
            };

        self.track_range(range);
    }

    pub fn track_range(&mut self, newrange: IRange<u32>) {
        debug_assert!(newrange.0 <= newrange.1);

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
    pub fn take_range(&mut self) -> Option<u16> {
        if self.rangeset.is_empty() {
            return None;
        }

        let offset_first = self._get_first();
        let first = offset_first.offset(-(self.offset as isize));
        if first.0 != 0 {
            None
        } else {
            self.rangeset.remove(&offset_first);
            self.offset += first.1 + 1;
            Some(first.1 as u16)
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
    debug_assert!(slice.len() > 0);
    debug_assert!(sub.len() > 0);
    assert!(slice.len() <= ::std::isize::MAX as usize);
    assert!(sub.len() <= ::std::isize::MAX as usize);

    let slice_start = slice.as_ptr();
    let slice_end = slice_start.offset(slice.len() as isize - 1);
    let sub_start = sub.as_ptr();
    let sub_end = sub_start.offset(sub.len() as isize - 1);

    sub_start >= slice_start && sub_end <= slice_end
}}

impl<'a, 'b, E> IntoIterator for &'b RangeTracker<'a, E> where E: 'a, 'a: 'b {
    type Item = IRange<u32>;
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
    type Item = IRange<u32>;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next().map(|x| {
            let DTRange(s,e) = x.offset(-(self.parent.offset as isize));
            IRange(s as u32, e as u32)
        })
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
struct DTRange(usize,usize);

impl DTRange {
    fn offset(&self, off: isize) -> DTRange {
        assert!(off >= 0
            || (self.0 >= (-off) as usize && self.1 >= (-off) as usize));
        DTRange (
            (self.0 as isize + off) as usize,
            (self.1 as isize + off) as usize
        )
    }
}

impl PartialOrd for DTRange {
    fn partial_cmp(&self, other: &DTRange) -> Option<Ordering> {
        debug_assert!(self.0 <= self.1);
        debug_assert!(other.0 <= other.1);
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

impl From<IRange<u32>> for DTRange where {
    fn from(r: IRange<u32>) -> DTRange {
        DTRange(r.0 as usize, r.0 as usize)
    }
}

impl From<IRange<u16>> for DTRange where {
    fn from(r: IRange<u16>) -> DTRange {
        DTRange(r.0 as usize, r.0 as usize)
    }
}
