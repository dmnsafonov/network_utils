use ::std::time::Duration;

use ::futures::prelude::*;
use ::tokio_timer::*;

use ::errors::Error;

pub struct TimeoutResultStream<S> {
    stream: Option<S>,
    duration: Duration,
    sleep: Sleep
}

pub enum TimedResult<T> {
    InTime(T),
    TimedOut
}

impl<S,E> TimeoutResultStream<S> where
        S: Stream<Error = E>,
        E: From<Error> {
    pub fn new(timer: &Timer, stream: S, duration: Duration)
            -> TimeoutResultStream<S> {
        TimeoutResultStream {
            stream: Some(stream),
            duration: duration,
            sleep: timer.sleep(duration)
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
                let timer = self.sleep.timer().clone();
                self.sleep = timer.sleep(self.duration);

                return Ok(Async::Ready(x.map(TimedResult::InTime)))
            },
            Err(e) => return Err(e.into())
        }

        match self.sleep.poll() {
            Ok(Async::NotReady) => Ok(Async::NotReady),
            Ok(Async::Ready(_)) => {
                let timer = self.sleep.timer().clone();
                self.sleep = timer.sleep(self.duration);

                Ok(Async::Ready(Some(TimedResult::TimedOut)))
            },
            Err(e) => {
                self.stream.take().unwrap();
                Err(Error::from(e).into())
            }
        }
    }
}
