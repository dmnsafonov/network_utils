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

use ::linux_network::futures::U8Slice;
use ::ping6_datacommon::*;

use ::stream::packet::parse_stream_client_packet;

pub struct DataOrderer {
    order: BinaryHeap<OrderedBufferRef>
}

struct OrderedBufferRef(U8Slice);

impl OrderedBufferRef {
    fn take(self) -> U8Slice {
        self.0
    }
}

impl<'a> PartialEq for OrderedBufferRef {
    fn eq(&self, other: &OrderedBufferRef) -> bool {
        let lbuf = self.0.borrow();
        let rbuf = other.0.borrow();

        let l = parse_stream_client_packet(&lbuf).seqno;
        let r = parse_stream_client_packet(&rbuf).seqno;

        l == r
    }
}

impl Eq for OrderedBufferRef {}

impl PartialOrd for OrderedBufferRef {
    fn partial_cmp(&self, other: &OrderedBufferRef)
            -> Option<Ordering> {
        let lbuf = self.0.borrow();
        let rbuf = other.0.borrow();

        let l = parse_stream_client_packet(&lbuf).seqno;
        let r = parse_stream_client_packet(&rbuf).seqno;

        Reverse(l).partial_cmp(&Reverse(r))
    }
}

impl Ord for OrderedBufferRef {
    fn cmp(&self, other: &OrderedBufferRef) -> Ordering {
        self.partial_cmp(other).unwrap()
    }
}

impl From<U8Slice> for OrderedBufferRef {
    fn from(x: U8Slice) -> Self {
        OrderedBufferRef(x)
    }
}

impl Deref for OrderedBufferRef {
    type Target = U8Slice;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DataOrderer {
    pub fn new(size: usize) -> DataOrderer {
        DataOrderer {
            order: BinaryHeap::with_capacity(size)
        }
    }

    pub fn add(&mut self, packet: U8Slice) {
        self.order.push(packet.into());
    }

    pub fn peek_seqno(&self) -> Option<u16> {
        self.order.peek().map(|x| {
            let packet_ref = x.borrow();
            let packet = parse_stream_client_packet(&packet_ref);
            packet.seqno
        })
    }

    pub fn take(&mut self) -> Option<U8Slice> {
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
