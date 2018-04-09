use ::std::time::*;

use ::tokio::prelude::*;
use ::tokio::timer::*;

use ::errors::Error;

pub struct TimeoutResultStream<S> {
    stream: Option<S>,
    duration: Duration,
    next_tick: Instant,
    sleep: Delay
}

pub enum TimedResult<T> {
    InTime(T),
    TimedOut
}

impl<S,E> TimeoutResultStream<S> where
        S: Stream<Error = E>,
        E: From<Error> {
    pub fn new(stream: S, duration: Duration)
            -> TimeoutResultStream<S> {
        let next_tick = Instant::now() + duration;
        TimeoutResultStream {
            stream: Some(stream),
            duration: duration,
            next_tick,
            sleep: Delay::new(next_tick)
        }
    }
}

impl<S,E> Stream for TimeoutResultStream<S> where
        S: Stream<Error = E>,
        E: From<Error> {
    type Item = TimedResult<S::Item>;
    type Error = E;

    fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        match self.stream.as_mut()
                .expect("not consumed TimeoutResultStream").poll() {
            Ok(Async::NotReady) => (),
            Ok(Async::Ready(x)) => {
                self.next_tick += self.duration;
                self.sleep = Delay::new(self.next_tick);

                return Ok(Async::Ready(x.map(TimedResult::InTime)))
            },
            Err(e) => return Err(e.into())
        }

        match self.sleep.poll() {
            Ok(Async::NotReady) => Ok(Async::NotReady),
            Ok(Async::Ready(_)) => {
                self.next_tick += self.duration;
                self.sleep = Delay::new(self.next_tick);

                Ok(Async::Ready(Some(TimedResult::TimedOut)))
            },
            Err(e) => {
                self.stream.take().unwrap();
                Err(Error::from(e).into())
            }
        }
    }
}
