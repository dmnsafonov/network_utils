// TODO: verify correctness of using relaxed memory ordering everywhere

use ::std::mem;
use ::std::sync::{*, atomic::*};

use ::futures::prelude::*;
use ::futures::{task, task::Task};

use ::errors::Error;

// TODO: remove next_id, track free ids in a hashset
// necessary for actual proxying of ndp requests
struct Inner<T> {
    rx_tasks: RwLock<Vec<Option<Task>>>,
    tx_task: RwLock<Option<Task>>,

    next_id: AtomicUsize,
    rx_count: AtomicUsize,
    tx_present: AtomicBool,

    message: RwLock<Option<T>>
}

pub struct Receiver<T> {
    inner: Arc<Inner<T>>,
    id: usize
}

pub struct Sender<T> {
    inner: Arc<Inner<T>>
}

pub fn broadcaster<T>(max_receivers: usize) -> (Receiver<T>, Sender<T>) {
    let inner = Arc::new(Inner {
        rx_tasks: RwLock::new(vec![None; max_receivers + 1]),
        tx_task: RwLock::new(None),
        next_id: AtomicUsize::new(2),
        rx_count: AtomicUsize::new(1),
        tx_present: AtomicBool::new(true),
        message: RwLock::new(None)
    });

    let receiver = Receiver {
        inner: inner.clone(),
        id: 1
    };

    let sender = Sender {
        inner: inner
    };

    (receiver, sender)
}

impl<T> Clone for Receiver<T> {
    fn clone(&self) -> Self {
        let id = self.inner.next_id.fetch_add(1, Ordering::Relaxed);
        self.inner.rx_count.fetch_add(1, Ordering::Relaxed);
        debug_assert!(self.inner.rx_tasks.read().unwrap()[id].is_some());
        assert!(id < self.inner.rx_tasks.read().unwrap().len());

        Receiver {
            inner: self.inner.clone(),
            id
        }
    }
}

impl<T> Receiver<T> {
    fn fill_task_if_empty(&self) {
        if self.inner.rx_tasks.read().unwrap()[self.id].is_none() {
            mem::swap(
                &mut self.inner.rx_tasks.write().unwrap()[self.id],
                &mut Some(task::current())
            );
        }
    }

    fn notify_sender(&mut self) {
        if let Some(task) = &*self.inner.tx_task.read().unwrap() {
            task.notify();
        }
    }

    pub fn is_sender_present(&self) -> bool {
        self.fill_task_if_empty();
        self.inner.tx_present.load(Ordering::Relaxed)
    }
}

impl<T> Stream for Receiver<T> where T: Clone {
    type Item = T;
    type Error = Error;

    fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        self.fill_task_if_empty();

        if !self.inner.tx_present.load(Ordering::Relaxed) {
            return Ok(Async::Ready(None))
        }

        match *self.inner.message.read().unwrap() {
            Some(ref x) => Ok(Async::Ready(Some(x.clone()))),
            None => Ok(Async::NotReady)
        }
    }
}

impl<T> Drop for Receiver<T> {
    fn drop(&mut self) {
        self.inner.rx_tasks.write().unwrap()[self.id].take();
        let n_rx = self.inner.rx_count.fetch_sub(1, Ordering::Relaxed);
        debug_assert!(n_rx >= 1);
        if n_rx == 1 {
            self.notify_sender();
        }
    }
}

impl<T> Sender<T> {
    fn fill_task_if_empty(&self) {
        if self.inner.tx_task.read().unwrap().is_none() {
            mem::swap(
                &mut *self.inner.tx_task.write().unwrap(),
                &mut Some(task::current())
            );
        }
    }

    fn notify_receivers(&mut self) {
        let receivers = self.inner.rx_tasks.read().unwrap();
        for i in &*receivers {
            if let Some(task) = i {
                task.notify();
            }
        }
    }

    pub fn are_receivers_present(&self) -> bool {
        self.fill_task_if_empty();
        self.inner.rx_count.load(Ordering::Relaxed) > 0
    }
}

impl<T> Sink for Sender<T> {
    type SinkItem = T;
    type SinkError = Error;

    fn start_send(
        &mut self,
        item: Self::SinkItem
    ) -> StartSend<Self::SinkItem, Self::SinkError> {
        self.fill_task_if_empty();

        mem::swap(
            &mut *self.inner.message.write().unwrap(),
            &mut Some(item)
        );

        self.notify_receivers();

        Ok(AsyncSink::Ready)
    }

    fn poll_complete(&mut self) -> Poll<(), Self::SinkError> {
        self.fill_task_if_empty();
        Ok(Async::Ready(()))
    }
}

impl<T> Drop for Sender<T> {
    fn drop(&mut self) {
        self.inner.tx_task.write().unwrap().take();
        self.inner.message.write().unwrap().take();
        self.inner.tx_present.store(false, Ordering::Relaxed);
        self.notify_receivers();
    }
}
