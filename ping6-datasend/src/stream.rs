use ::std::cmp::*;
use ::std::collections::*;
use ::std::mem::uninitialized;
use ::std::net::SocketAddrV6;
use ::std::num::Wrapping;
use ::std::ops::*;
use ::std::rc::Rc;
use ::std::slice;

use ::futures::prelude::*;
use ::state_machine_future::RentToOwn;

use ::linux_network::*;

use ::config::*;
use ::errors::{Error, Result};
use ::util::InitState;

#[derive(Debug)]
struct WindowedBuffer<'a> {
    inner: VecDeque<u8>,
    window_size: u32,
    first_available: u16,
    del_tracker: DeletionTracker<'a, u8>
}

impl<'a> WindowedBuffer<'a> {
    fn new(size: usize, window_size: u32) -> Box<WindowedBuffer<'a>> {
        assert!(window_size <= ::std::u16::MAX as u32 + 1);

        let mut ret = Box::new(WindowedBuffer {
            inner: VecDeque::with_capacity(size),
            window_size: window_size,
            first_available: 0,
            del_tracker: unsafe { uninitialized() }
        });
        ret.del_tracker = unsafe {
            let ptr = &ret.inner as *const VecDeque<u8>;
            DeletionTracker::new(ptr.as_ref().unwrap())
        };
        ret
    }

    fn add<T>(&mut self, data: T) where T: Into<VecDeque<u8>> {
        let mut vddata = data.into();
        assert!(self.inner.len().checked_add(vddata.len()).is_some());
        self.inner.append(&mut vddata);
    }

    fn add_cloning<T>(&mut self, data: T) where T: AsRef<[u8]> {
        let dataref = data.as_ref();
        assert!(self.inner.len().checked_add(dataref.len()).is_some());
        self.inner.extend(dataref[..].iter());
    }

    fn get_space_left(&self) -> usize {
        self.inner.capacity() - self.inner.len()
    }

    // availability is moot beyond the current window,
    // so value returned is restrained by the window size
    fn get_available(&self) -> u32 {
        let ret = min(self.inner.len() - self.first_available as usize,
            self.window_size as usize - self.first_available as usize);
        debug_assert!(ret <= ::std::u16::MAX as usize + 1);
        ret as u32
    }

    fn take(&mut self, size: u32) -> Option<WindowedBufferSlice<'a>> {
        assert!(size <= ::std::u16::MAX as u32 + 1);

        let len = min(self.get_available(), size);
        if len == 0 {
            return None;
        }

        let tracker_ptr = &mut self.del_tracker
            as *mut DeletionTracker<'a, u8>;
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

    fn cleanup(&mut self) {
        if let Some(ind) = self.del_tracker.take_deletion() {
            self.inner.drain(0 .. ind as usize + 1);
        }
    }
}

// can track stream sized up to usize::MAX/2
#[derive(Debug)]
struct DeletionTracker<'a, E> where E: 'a {
    tracked: &'a VecDeque<E>,

    // at no point should two overlapping ranges be insert()'ed into the set
    rangeset: BTreeSet<DTRange>,

    // we return the VecDeque's buffer indices, but we do not increment
    // the rangeset range indices each deletion, so we need to track how much
    // we are off
    offset: usize
}

impl<'a, E> DeletionTracker<'a, E> {
    fn new(tracked: &'a VecDeque<E>) -> DeletionTracker<'a, E> {
        DeletionTracker {
            tracked: tracked,
            rangeset: BTreeSet::new(),
            offset: 0
        }
    }

    fn track_slice(&mut self, newslice: &[E]) {
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

    fn track_range(&mut self, newrange: IRange<u32>) {
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

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
struct IRange<Idx>(Idx,Idx);

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

enum WindowedBufferSlice<'a> {
    Direct {
        tracker: *mut DeletionTracker<'a, u8>,
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

struct AckWaitlist<'a> {
    inner: VecDeque<AckWait<'a>>,
    del_tracker: DeletionTracker<'a, AckWait<'a>>
}

struct AckWait<'a> {
    seqno: Wrapping<u16>,
    data: WindowedBufferSlice<'a>
}

impl<'a> AckWaitlist<'a> {
    fn new(window_size: u32) -> Box<AckWaitlist<'a>> {
        assert!(window_size <= ::std::u16::MAX as u32 + 1);
        let mut ret = Box::new(AckWaitlist {
            inner: VecDeque::with_capacity(window_size as usize),
            del_tracker: unsafe { uninitialized() }
        });
        ret.del_tracker = unsafe {
            let ptr = &mut ret.inner as *mut VecDeque<AckWait<'a>>;
            DeletionTracker::new(ptr.as_mut().unwrap())
        };
        ret
    }

    fn add(&mut self, wait: AckWait<'a>) {
        debug_assert!(self.inner.is_empty()
            || wait.seqno > self.inner.back().unwrap().seqno
            || (self.inner.back().unwrap().seqno.0 == ::std::u16::MAX
                && wait.seqno.0 == 0));
        assert!(self.inner.capacity() - self.inner.len() > 0);
        self.inner.push_back(wait);
    }

    // safe to call multiple times with the same arguments
    // and with overlapping ranges
    fn remove(&mut self, range: IRange<Wrapping<u16>>) {
        if range.0 < range.1 {
            self.remove_not_wrapping(IRange(range.0,
                Wrapping(::std::u16::MAX)));
            self.remove_not_wrapping(IRange(Wrapping(0), range.1));
        } else {
            self.remove_not_wrapping(range);
        }
    }

    // safe to call multiple times with the same arguments
    fn remove_not_wrapping(&mut self, range: IRange<Wrapping<u16>>) {
        assert!(range.0 <= range.1);

        let del_tracker = &mut self.del_tracker;
        let mut peekable = self.inner.iter()
            .enumerate()
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
                    del_tracker.track_range(IRange(start_ind, ind));
                    start_ind = ind;
                }
                curr_seqno = seqno;
                last_ind = ind;
            });
            del_tracker.track_range(IRange(start_ind, last_ind));
        }
    }

    fn cleanup(&mut self) {
        if let Some(ind) = self.del_tracker.take_deletion() {
            self.inner.drain(0 .. ind as usize + 1);
        }
    }
}

#[derive(StateMachineFuture)]
enum StreamMachine {
    #[state_machine_future(start, transitions(WaitForSynAck))]
    SendFirstSyn {
        init_state: StreamInitState,
        try_number: u32
    },

    #[state_machine_future(transitions(SendFirstSyn, SendAck))]
    WaitForSynAck {
        init_state: StreamInitState,
        try_number: u32
    },

    #[state_machine_future(transitions(SendData))]
    SendAck {
        init_state: StreamInitState
    },

    #[state_machine_future(transitions(ReceivedServerFin, SendFin, WaitForAck))]
    SendData {
        init_state: StreamInitState,

    },

    #[state_machine_future(transitions(ReceivedServerFin, SendData, SendFin))]
    WaitForAck {
        init_state: StreamInitState
    },

    #[state_machine_future(transitions(SendFinAck))]
    ReceivedServerFin {
        init_state: StreamInitState
    },

    #[state_machine_future(transitions(ReceivedServerFin, WaitForLastAck))]
    SendFinAck {
        init_state: StreamInitState
    },

    #[state_machine_future(transitions(ConnectionTerminated))]
    WaitForLastAck {
        init_state: StreamInitState
    },

    #[state_machine_future(transitions(WaitForFinAck))]
    SendFin {
        init_state: StreamInitState
    },

    #[state_machine_future(transitions(SendFin, SendLastAck))]
    WaitForFinAck {
        init_state: StreamInitState
    },

    #[state_machine_future(transitions(ConnectionTerminated))]
    SendLastAck {
        init_state: StreamInitState
    },

    #[state_machine_future(ready)]
    ConnectionTerminated(TerminationReason),

    #[state_machine_future(error)]
    ErrorState(Error)
}

enum TerminationReason {
    DataSent,
    ServerFin
}

impl PollStreamMachine for StreamMachine {
    fn poll_send_first_syn<'a>(
        state: &'a mut RentToOwn<'a, SendFirstSyn>
    ) -> Poll<AfterSendFirstSyn, Error> {
        unimplemented!()
    }

    fn poll_wait_for_syn_ack<'a>(
        state: &'a mut RentToOwn<'a, WaitForSynAck>
    ) -> Poll<AfterWaitForSynAck, Error> {
        unimplemented!()
    }

    fn poll_send_ack<'a>(
        state: &'a mut RentToOwn<'a, SendAck>
    ) -> Poll<AfterSendAck, Error> {
        unimplemented!()
    }

    fn poll_send_data<'a>(
        state: &'a mut RentToOwn<'a, SendData>
    ) -> Poll<AfterSendData, Error> {
        unimplemented!()
    }

    fn poll_wait_for_ack<'a>(
        state: &'a mut RentToOwn<'a, WaitForAck>
    ) -> Poll<AfterWaitForAck, Error> {
        unimplemented!()
    }

    fn poll_received_server_fin<'a>(
        state: &'a mut RentToOwn<'a, ReceivedServerFin>
    ) -> Poll<AfterReceivedServerFin, Error> {
        unimplemented!()
    }

    fn poll_send_fin_ack<'a>(
        state: &'a mut RentToOwn<'a, SendFinAck>
    ) -> Poll<AfterSendFinAck, Error> {
        unimplemented!()
    }

    fn poll_wait_for_last_ack<'a>(
        state: &'a mut RentToOwn<'a, WaitForLastAck>
    ) -> Poll<AfterWaitForLastAck, Error> {
        unimplemented!()
    }

    fn poll_send_fin<'a>(
        state: &'a mut RentToOwn<'a, SendFin>
    ) -> Poll<AfterSendFin, Error> {
        unimplemented!()
    }

    fn poll_wait_for_fin_ack<'a>(
        state: &'a mut RentToOwn<'a, WaitForFinAck>
    ) -> Poll<AfterWaitForFinAck, Error> {
        unimplemented!()
    }

    fn poll_send_last_ack<'a>(
        state: &'a mut RentToOwn<'a, SendLastAck>
    ) -> Poll<AfterSendLastAck, Error> {
        unimplemented!()
    }
}

struct StreamInitState {
    config: Rc<Config>,
    src: SocketAddrV6,
    dst: SocketAddrV6,
    sock: futures::IpV6RawSocketAdapter
}

pub fn stream_mode((config, src, dst, mut sock): InitState) -> Result<()> {
    let _stream_conf = extract!(ModeConfig::Stream(_), config.mode).unwrap();

    unimplemented!()
}
