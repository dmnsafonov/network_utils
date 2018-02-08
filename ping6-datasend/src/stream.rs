use ::std::cmp::*;
use ::std::collections::*;
use ::std::ops::Deref;
use ::std::slice;

use ::config::*;
use ::errors::Result;

use ::util::InitState;

#[derive(Debug)]
struct WindowedBuffer<'a> {
    inner: VecDeque<u8>,
    window_size: u16,
    first_available: u16,
    del_tracker: DeletionTracker<'a>
}

impl<'a> WindowedBuffer<'a> {
    fn new(size: usize, window_size: u16) -> Box<WindowedBuffer<'a>> {
        let mut ret = Box::new(WindowedBuffer {
            inner: VecDeque::with_capacity(size),
            window_size: window_size,
            first_available: 0,
            del_tracker: unsafe { ::std::mem::uninitialized() }
        });
        ret.del_tracker = unsafe {
            let ptr = ret.as_ref() as *const WindowedBuffer;
            DeletionTracker::new(ptr.as_ref().unwrap())
        };
        ret
    }

    fn add<T>(&mut self, data: T) -> usize where T: Into<VecDeque<u8>> {
        let mut vddata = data.into();
        let len = min(self.get_space_left(), vddata.len());
        self.inner.append(&mut vddata);
        len
    }

    fn add_cloning<T>(&mut self, data: T) -> usize where T: AsRef<[u8]> {
        let dataref = data.as_ref();
        let len = min(self.get_space_left(), dataref.len());
        self.inner.extend(dataref[0..len].iter());
        len
    }

    fn get_space_left(&self) -> usize {
        self.inner.capacity() - self.inner.len()
    }

    // availability is moot beyond the current window,
    // so value returned is restrained by the window size
    fn get_available(&self) -> u16 {
        let ret = min(self.inner.len() - self.first_available as usize,
            self.window_size as usize - self.first_available as usize);
        debug_assert!(ret <= ::std::u16::MAX as usize);
        ret as u16
    }

    fn take(&mut self, size: u16) -> WindowedBufferSlice<'a> {
        let len = min(self.get_available(), size) as u16;
        let self_ptr = self as *mut WindowedBuffer;
        let (beginning, ending) = self.inner.as_slices();
        let beg_len = beginning.len();
        if (self.first_available as usize) < beg_len {
            if (self.first_available + len) as usize <= beg_len { unsafe {
                WindowedBufferSlice::Direct {
                    parent: self_ptr,
                    start: beginning.as_ptr()
                        .offset(self.first_available as isize),
                    len: len
                }
            }} else {
                let mut ret = Vec::with_capacity(len as usize);
                let beg_slice = &beginning[self.first_available as usize..];
                ret.extend_from_slice(beg_slice);
                let ending_len = len as usize - beg_slice.len();
                let end_slice = &ending[0..ending_len];
                ret.extend_from_slice(end_slice);

                self.del_tracker.track(beg_slice);
                self.del_tracker.track(end_slice);

                WindowedBufferSlice::Owning(ret.into())
            }
        } else { unsafe {
            WindowedBufferSlice::Direct {
                parent: self_ptr,
                start: ending.as_ptr()
                    .offset((self.first_available as usize - beg_len) as isize),
                len: len
            }
        }}
    }

    fn cleanup(&mut self) {
        if let Some(ind) = self.del_tracker.take_deletion() {
            self.inner.drain(0 .. ind as usize + 1);
        }
    }
}

// can track stream sized up to usize::MAX/2
#[derive(Debug)]
struct DeletionTracker<'a> {
    parent: &'a WindowedBuffer<'a>,

    // at no point should two overlapping ranges be insert()'ed into the set
    rangeset: BTreeSet<DTRange>,

    // we return the WindowedBuffer's buffer indices, but we do not increment
    // the rangeset range indices each deletion, so we need to track how much
    // we are off
    offset: usize
}

impl<'a> DeletionTracker<'a> {
    fn new(parent: &'a WindowedBuffer) -> DeletionTracker<'a> {
        DeletionTracker {
            parent: parent,
            rangeset: BTreeSet::new(),
            offset: 0
        }
    }

    fn track(&mut self, newrange: &[u8]) {
        let len = newrange.len();
        debug_assert!(len <= ::std::u16::MAX as usize);
        let ptr = newrange.as_ptr() as usize;

        let (beginning, ending) = self.parent.inner.as_slices();
        let range = if is_subslice(beginning, newrange) {
                let beg_ptr = beginning.as_ptr() as usize;
                let start = beg_ptr - ptr;
                let end = start + len - 1;
                DTRange(start, end)
            } else {
                debug_assert!(is_subslice(ending, newrange));
                let end_ptr = ending.as_ptr() as usize;
                let start = end_ptr - ptr;
                let end = start + len - 1;
                DTRange(start, end)
            };

        let offset_range = range.offset(self.offset as isize);

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
            self.offset += first.1 + 1;
            Some(first.1 as u16)
        }
    }

    fn _get_first(&self) -> DTRange {
        *self.rangeset.iter().next().expect("nonempty range set")
    }
}

fn is_subslice<T>(slice: &[T], sub: &[T]) -> bool { unsafe {
    assert!(slice.len() <= ::std::isize::MAX as usize);
    assert!(sub.len() <= ::std::isize::MAX as usize);

    let slice_start = slice.as_ptr();
    let slice_end = slice_start.offset(slice.len() as isize - 1);
    let sub_start = sub.as_ptr();
    let sub_end = sub_start.offset(sub.len() as isize - 1);

    sub_start >= slice_start && sub_end <= slice_end
}}

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

enum WindowedBufferSlice<'a> {
    Direct {
        parent: *mut WindowedBuffer<'a>,
        start: *const u8,
        len: u16
    },
    Owning(Box<[u8]>)
}

impl<'a> Drop for WindowedBufferSlice<'a> {
    fn drop(&mut self) {
        if let &mut WindowedBufferSlice::Direct { parent, start, len }
                = self { unsafe {
            let p = parent.as_mut().unwrap();
            p.del_tracker.track(slice::from_raw_parts(start, len as usize));
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

pub fn stream_mode((config, src, dst, mut sock): InitState) -> Result<()> {
    let _stream_conf = extract!(ModeConfig::Stream(_), config.mode).unwrap();

    unimplemented!()
}
