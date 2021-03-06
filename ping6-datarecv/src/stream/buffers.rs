use ::std::cmp::*;
use ::std::collections::*;
use ::std::num::Wrapping;
use ::std::ops::Deref;
use ::std::sync::{atomic::{Ordering, *}, *};
use ::std::time::*;

use ::errors::Error;
use ::futures::prelude::*;
use ::tokio::timer::Interval;

use ::ping6_datacommon::*;

use ::stream::packet::parse_stream_client_packet;

pub struct DataOrderer {
    buffer: TrimmingBuffer,
    order: BinaryHeap<OrderedTrimmingBufferSlice>
}

struct OrderedTrimmingBufferSlice(TrimmingBufferSlice);

impl OrderedTrimmingBufferSlice {
    fn take(self) -> TrimmingBufferSlice {
        self.0
    }
}

impl<'a> PartialEq for OrderedTrimmingBufferSlice {
    fn eq(&self, other: &Self) -> bool {
        let l = parse_stream_client_packet(&self.0).seqno;
        let r = parse_stream_client_packet(&other.0).seqno;

        l == r
    }
}

impl Eq for OrderedTrimmingBufferSlice {}

impl PartialOrd for OrderedTrimmingBufferSlice {
    fn partial_cmp(&self, other: &Self)
            -> Option<::std::cmp::Ordering> {
        let l = parse_stream_client_packet(&self.0).seqno;
        let r = parse_stream_client_packet(&other.0).seqno;

        Reverse(l).partial_cmp(&Reverse(r))
    }
}

impl Ord for OrderedTrimmingBufferSlice {
    fn cmp(&self, other: &Self) -> ::std::cmp::Ordering {
        self.partial_cmp(other).unwrap()
    }
}

impl From<TrimmingBufferSlice> for OrderedTrimmingBufferSlice {
    fn from(x: TrimmingBufferSlice) -> Self {
        OrderedTrimmingBufferSlice(x)
    }
}

impl Deref for OrderedTrimmingBufferSlice {
    type Target = TrimmingBufferSlice;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DataOrderer {
    pub fn new(window_size: u32, mtu: u16) -> Self {
        Self {
            buffer: TrimmingBuffer::new(window_size as usize * mtu as usize),
            order: BinaryHeap::with_capacity(window_size as usize)
        }
    }

    pub fn add<T>(&mut self, packet: T) where T: AsRef<[u8]> {
        let slice = self.buffer.add_slicing(packet);
        self.order.push(slice.into());
    }

    pub fn get_space_left(&self) -> usize {
        self.buffer.get_space_left()
    }

    pub fn peek_seqno(&self) -> Option<u16> {
        self.order.peek().map(|x| {
            let packet = parse_stream_client_packet(x);
            packet.seqno
        })
    }

    pub fn take(&mut self) -> Option<TrimmingBufferSlice> {
        self.order.pop().map(|x| x.take())
    }

    pub fn cleanup(&mut self) {
        self.buffer.cleanup();
    }

    pub fn is_empty(&self) -> bool {
        self.order.is_empty()
    }
}

pub struct SeqnoTracker {
    tracker: RangeTracker<NoParent, NoElement>,
    window_start: Wrapping<u16>
}

const U16_MAX_P1: usize = u16::max_value() as usize + 1;

impl SeqnoTracker {
    pub fn new(next_seqno: Wrapping<u16>) -> Self {
        Self {
            tracker: RangeTracker::new(),
            window_start: next_seqno
        }
    }

    pub fn add(&mut self, x: Wrapping<u16>) -> bool {
        let ax = self.pos_to_sequential(x);
        let range = IRange(ax, ax);
        if self.tracker.is_range_tracked(range).unwrap() {
            false
        } else {
            self.tracker.track_range(IRange(ax, ax));
            true
        }
    }

    pub fn pos_to_sequential(&self, x: Wrapping<u16>) -> usize {
        (x - self.window_start).0 as usize
    }

    pub fn take(&mut self)
    -> (VecDeque<IRange<Wrapping<u16>>>, Wrapping<u16>) {
        let ret = self.tracker.into_iter().map(|IRange(l,r)| {
            IRange(
                self.pos_from_sequential(l),
                self.pos_from_sequential(r)
            )
        }).collect();
        let window_start = self.window_start;
        if let Some(x) = self.tracker.take_range() {
            self.window_start = self.pos_from_sequential(x) + Wrapping(1);
        }
        (ret, window_start)
    }

    #[allow(clippy::cast_possible_truncation)]
    pub fn pos_from_sequential(&self, x: usize) -> Wrapping<u16> {
        Wrapping((x % U16_MAX_P1) as u16) + self.window_start
    }

    pub fn is_empty(&self) -> bool {
        self.tracker.is_empty()
    }
}

pub struct TimedAckSeqnoGenerator {
    tracker: Arc<Mutex<SeqnoTracker>>,
    period: Duration,
    interval: Option<Interval>,
    active: bool,
    stopped: Arc<AtomicBool>,
    timeless: Arc<AtomicBool>
}

impl TimedAckSeqnoGenerator {
    pub fn new(tracker: Arc<Mutex<SeqnoTracker>>, dur: Duration) -> Self {
        Self {
            tracker,
            period: dur,
            interval: None,
            active: false,
            stopped: Arc::new(AtomicBool::new(false)),
            timeless: Arc::new(AtomicBool::new(true))
        }
    }

    pub fn start(&mut self) {
        assert!(self.interval.is_none());
        self.active = true;
        fence(Ordering::Release);
    }

    pub fn handle(&mut self) -> AckGenHandle {
        AckGenHandle {
            stopped: self.stopped.clone(),
            timeless: self.timeless.clone()
        }
    }
}

#[derive(Clone)]
pub struct AckGenHandle {
    stopped: Arc<AtomicBool>,
    timeless: Arc<AtomicBool>
}

impl AckGenHandle {
    pub fn stop(&mut self) {
        self.stopped.store(true, Ordering::Release);
    }

    pub fn request_timeless(&mut self) {
        self.timeless.store(true, Ordering::Release);
    }
}

impl Stream for TimedAckSeqnoGenerator {
    type Item = (VecDeque<IRange<Wrapping<u16>>>, Wrapping<u16>);
    type Error = Error;

    fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        let stopped = self.stopped.load(Ordering::Acquire);
        if stopped {
            return Ok(Async::Ready(None));
        }
        if !self.active {
            return Ok(Async::NotReady);
        }

        let period = self.period;
        let interval = self.interval.get_or_insert_with(||
            Interval::new(Instant::now(), period)
        );

        if !self.timeless.swap(false, Ordering::Acquire) {
            try_ready!(interval.poll());
        }
        while interval.poll()?.is_ready() {}
        let (ranges, ws) = self.tracker.lock().unwrap().take();
        if ranges.is_empty() {
            return Ok(Async::NotReady);
        }
        Ok(Async::Ready(Some((ranges, ws))))
    }
}
