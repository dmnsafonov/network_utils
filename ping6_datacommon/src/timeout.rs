use ::std::time::*;

use ::tokio::prelude::*;
use ::tokio::timer::*;

use ::errors::Error;

pub struct TimeoutResultStream<S> {
    stream: Option<S>,
    duration: Duration,
    sleep: Option<Delay>
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
        TimeoutResultStream {
            stream: Some(stream),
            duration,
            sleep: None
        }
    }
}

impl<S,E> Stream for TimeoutResultStream<S> where
        S: Stream<Error = E>,
        E: From<Error> {
    type Item = TimedResult<S::Item>;
    type Error = E;

    fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        let instant = Instant::now() + self.duration;
        let sleep = self.sleep.get_or_insert_with(|| {
            Delay::new(instant)
        });

        match self.stream.as_mut()
                .expect("not consumed TimeoutResultStream").poll() {
            Ok(Async::NotReady) => (),
            Ok(Async::Ready(x)) => {
                let instant = Delay::deadline(sleep) + self.duration;
                sleep.reset(instant);

                return Ok(Async::Ready(x.map(TimedResult::InTime)))
            },
            Err(e) => return Err(e)
        }

        match sleep.poll() {
            Ok(Async::NotReady) => Ok(Async::NotReady),
            Ok(Async::Ready(_)) => {
                let instant = Delay::deadline(sleep) + self.duration;
                sleep.reset(instant);

                Ok(Async::Ready(Some(TimedResult::TimedOut)))
            },
            Err(e) => {
                self.stream.take().unwrap();
                Err(Error::TimerError(e).into())
            }
        }
    }
}
