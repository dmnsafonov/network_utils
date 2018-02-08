use ::std::cmp::*;
use ::std::collections::*;
use ::std::ops::Range;

use ::config::*;
use ::errors::Result;

use ::util::InitState;

#[derive(Debug)]
struct WindowedBuffer {
    inner: VecDeque<u8>,
    window_size: u16,
    window_start: u16,
    del_tracker: DeletionTracker
}

impl WindowedBuffer {
    fn new(size: usize, window_size: u16) -> WindowedBuffer {
        WindowedBuffer {
            inner: VecDeque::with_capacity(size),
            window_size: window_size,
            window_start: 0,
            del_tracker: DeletionTracker::new()
        }
    }

    fn add<T>(&mut self, data: T) -> usize where T: Into<VecDeque<u8>> {
        let mut vddata = data.into();
        let len = min(self.inner.capacity() - self.inner.len(), vddata.len());
        self.inner.append(&mut vddata);
        len
    }

    fn add_cloning<T>(&mut self, data: T) -> usize where T: AsRef<[u8]> {
        let dataref = data.as_ref();
        let len = min(self.inner.capacity() - self.inner.len(), dataref.len());
        self.inner.extend(dataref[0..len].iter());
        len
    }
}

// can track stream sized of up to usize::MAX/2
#[derive(Debug)]
struct DeletionTracker {
    // at no point should two overlapping ranges be insert()'ed into the set
    rangeset: BTreeSet<DTRange>,

    // we return the WindowedBuffer's buffer indices, but we do not increment
    // the rangeset range indices each deletion, so we need to track how much
    // we are off
    offset: usize
}

impl DeletionTracker {
    fn new() -> DeletionTracker {
        DeletionTracker {
            rangeset: BTreeSet::new(),
            offset: 0
        }
    }

    fn track(&mut self, newrange: Range<u16>) {
        let offset_range = DTRange::from(newrange).offset(self.offset as isize);

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
            if i.0 > offset_range.1 + 1 {
                merge_right = Some(*i);
            }
            if i.0 > offset_range.1 {
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

    // remove (0 =.. get_deletion()) after the call
    fn take_deletion(&mut self) -> Option<u16> {
        if self.rangeset.is_empty() {
            return None;
        }

        let offset_first = self._get_first();
        let first = offset_first.offset(-(self.offset as isize));
        if first.0 != 0 {
            None
        } else {
            self.rangeset.remove(&offset_first);
            Some(first.1 as u16)
        }
    }

    fn _get_first(&self) -> DTRange {
        *self.rangeset.iter().next().expect("nonempty range set")
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
struct DTRange(usize,usize);

impl DTRange {
    fn offset(&self, off: isize) -> DTRange {
        DTRange(
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
            if other.0 < self.0 && other.1 < self.0 {
                Some(Ordering::Greater)
            } else if other.0 > self.1 && other.1 > self.1 {
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

impl From<Range<u16>> for DTRange {
    fn from(range: Range<u16>) -> DTRange {
        DTRange(range.start as usize, range.end as usize)
    }
}

pub fn stream_mode((config, src, dst, mut sock): InitState) -> Result<()> {
    let _stream_conf = extract!(ModeConfig::Stream(_), config.mode).unwrap();

    unimplemented!()
}
