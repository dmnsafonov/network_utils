//# the data buffer is a binaryheap of indices into a ring buffer of payloads,
//# sorted by stream positions (MAX-pos for it being a max-heap)
//#     take ping6_datasend::stream::WindowedBuffer, remove window_size from it
//#     to remake it into a general buffer; then make a wrapper
//#     for the prio-queue logic
//
// the ack tracker gets every seqno, gives it to rangetracker,
// gives out a stream of seqno ranges to ack every Duration by using
// timeout_stream::Interval

use ::std::cmp::*;
use ::std::collections::*;
use ::std::ops::Deref;

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
    fn new(size: usize) -> DataOrderer {
        DataOrderer {
            buffer: TrimmingBuffer::new(size),
            order: BinaryHeap::with_capacity(size)
        }
    }

    fn add<T>(&mut self, packet: T) where T: AsRef<[u8]> {
        let slice = self.buffer.add_slicing(packet);
        self.order.push(slice.into());
    }

    fn peek_seqno(&self) -> Option<u16> {
        self.order.peek().map(|x| {
            let packet = parse_stream_client_packet(x);
            packet.seqno
        })
    }

    fn take(&mut self) -> Option<TrimmingBufferSlice> {
        self.order.pop().map(|x| x.take())
    }
}
