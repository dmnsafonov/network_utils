use ::std::cmp::*;
use ::std::collections::*;
use ::std::mem::ManuallyDrop;

use ::ping6_datacommon::*;

use ::stream::packet::parse_stream_client_packet;

pub struct DataOrderer<'a> {
    buffer: ManuallyDrop<TrimmingBuffer<'a>>,
    order: ManuallyDrop<BinaryHeap<OrderedTrimmingBufferSlice<'a>>>
}

struct OrderedTrimmingBufferSlice<'a>(TrimmingBufferSlice<'a>);

impl<'a> PartialEq for OrderedTrimmingBufferSlice<'a> {
    fn eq(&self, other: &OrderedTrimmingBufferSlice<'a>) -> bool {
        let l = parse_stream_client_packet(&self.0).seqno;
        let r = parse_stream_client_packet(&other.0).seqno;

        l == r
    }
}

impl<'a> Eq for OrderedTrimmingBufferSlice<'a> {}

impl<'a> PartialOrd for OrderedTrimmingBufferSlice<'a> {
    fn partial_cmp(&self, other: &OrderedTrimmingBufferSlice<'a>)
            -> Option<Ordering> {
        let l = parse_stream_client_packet(&self.0).seqno;
        let r = parse_stream_client_packet(&other.0).seqno;

        Reverse(l).partial_cmp(&Reverse(r))
    }
}

impl<'a> Ord for OrderedTrimmingBufferSlice<'a> {
    fn cmp(&self, other: &OrderedTrimmingBufferSlice<'a>) -> Ordering {
        self.partial_cmp(other).unwrap()
    }
}

impl<'a> From<TrimmingBufferSlice<'a>> for OrderedTrimmingBufferSlice<'a> {
    fn from(x: TrimmingBufferSlice<'a>) -> Self {
        OrderedTrimmingBufferSlice(x)
    }
}

impl<'a> Drop for DataOrderer<'a> {
    fn drop(&mut self) { unsafe {
        ManuallyDrop::drop(&mut self.order);
        ManuallyDrop::drop(&mut self.buffer);
    }}
}

impl<'a> DataOrderer<'a> {
    fn new(size: usize) -> DataOrderer<'a> {
        DataOrderer {
            buffer: ManuallyDrop::new(TrimmingBuffer::new(size)),
            order: ManuallyDrop::new(BinaryHeap::with_capacity(size))
        }
    }

    fn add<T>(&mut self, packet: T) where T: AsRef<[u8]> {
        let slice = self.buffer.add_slicing(packet);
        self.order.push(slice.into());
    }
}
