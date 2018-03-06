use ::std::cell::RefCell;
use ::std::cmp::*;
use ::std::collections::*;
use ::std::num::Wrapping;
use ::std::ops::Deref;
use ::std::rc::Rc;
use ::std::time::Duration;

use ::errors::Error;
use ::futures::prelude::*;
use ::tokio_timer::*;

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
    fn eq(&self, other: &OrderedTrimmingBufferSlice) -> bool {
        let l = parse_stream_client_packet(&self.0).seqno;
        let r = parse_stream_client_packet(&other.0).seqno;

        l == r
    }
}

impl Eq for OrderedTrimmingBufferSlice {}

impl PartialOrd for OrderedTrimmingBufferSlice {
    fn partial_cmp(&self, other: &OrderedTrimmingBufferSlice)
            -> Option<Ordering> {
        let l = parse_stream_client_packet(&self.0).seqno;
        let r = parse_stream_client_packet(&other.0).seqno;

        Reverse(l).partial_cmp(&Reverse(r))
    }
}

impl Ord for OrderedTrimmingBufferSlice {
    fn cmp(&self, other: &OrderedTrimmingBufferSlice) -> Ordering {
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
    pub fn new(size: usize) -> DataOrderer {
        DataOrderer {
            buffer: TrimmingBuffer::new(size),
            order: BinaryHeap::with_capacity(size)
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
}

pub struct SeqnoTracker {
    tracker: RangeTracker<NoParent, NoElement>,
    window_start: Wrapping<u16>
}

const U16_MAX_P1: usize = ::std::u16::MAX as usize + 1;

impl SeqnoTracker {
    pub fn new(next_seqno: Wrapping<u16>) -> SeqnoTracker {
        SeqnoTracker {
            tracker: RangeTracker::new(),
            window_start: -next_seqno
        }
    }

    pub fn add(&mut self, x: Wrapping<u16>) {
        let ax = self.to_sequential(x);
        self.tracker.track_range(IRange(ax, ax));
    }

    pub fn to_sequential(&self, x: Wrapping<u16>) -> usize {
        (x - self.window_start).0 as usize
    }

    pub fn take(&mut self) -> Vec<IRange<Wrapping<u16>>> {
        let ret = self.tracker.into_iter().map(|IRange(l,r)| {
            IRange(
                self.from_sequential(l),
                self.from_sequential(r)
            )
        }).collect();
        if let Some(x) = self.tracker.take_range() {
            self.window_start += Wrapping(x as u16) + Wrapping(1);
        }
        ret
    }

    pub fn from_sequential(&self, x: usize) -> Wrapping<u16> {
        Wrapping((x % U16_MAX_P1) as u16) + self.window_start
    }
}

pub struct TimedAckSeqnoGenerator {
    tracker: Rc<RefCell<SeqnoTracker>>,
    timer: Timer,
    period: Duration,
    interval: Option<Interval>
}

impl TimedAckSeqnoGenerator {
    pub fn new(tracker: Rc<RefCell<SeqnoTracker>>, timer: Timer, dur: Duration)
            -> TimedAckSeqnoGenerator {
        TimedAckSeqnoGenerator {
            tracker: tracker,
            timer: timer,
            period: dur,
            interval: None
        }
    }

    pub fn start(&mut self) {
        assert!(self.interval.is_none());
        self.interval = Some(self.timer.interval(self.period));
    }
}

impl Stream for TimedAckSeqnoGenerator {
    type Item = Vec<IRange<Wrapping<u16>>>;
    type Error = Error;

    fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        let interval = self.interval.as_mut().expect("a started ack interval");
        let ranges = self.tracker.borrow_mut().take();
        try_ready!(interval.poll());

        Ok(if ranges.is_empty() {
            Async::NotReady
        } else {
            Async::Ready(Some(ranges))
        })
    }
}
