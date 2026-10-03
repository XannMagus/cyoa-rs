//! Finite nonblocking writes: temporary backpressure must not stop input polling.
use std::{
    collections::VecDeque,
    io::{self, Write},
    time::{Duration, Instant},
};
pub(super) const STALL_LIMIT: Duration = Duration::from_secs(1);
const CAPACITY: usize = 1024 * 1024;
const PUMP_BYTES: usize = 64 * 1024;
const PUMP_CALLS: usize = 32;

pub(super) struct QueuedOutput<'a> {
    sink: &'a mut dyn Write,
    bytes: VecDeque<u8>,
    last_progress: Option<Instant>,
}
impl<'a> QueuedOutput<'a> {
    pub(super) fn new(sink: &'a mut dyn Write) -> Self {
        Self {
            sink,
            bytes: VecDeque::new(),
            last_progress: None,
        }
    }
    pub(super) fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }
    pub(super) fn pump(&mut self, now: Instant) -> io::Result<()> {
        let mut remaining = PUMP_BYTES;
        for _ in 0..PUMP_CALLS {
            if self.bytes.is_empty() || remaining == 0 {
                break;
            }
            let (first, _) = self.bytes.as_slices();
            match self.sink.write(&first[..first.len().min(remaining)]) {
                Ok(0) => return Err(io::ErrorKind::WriteZero.into()),
                Ok(n) => {
                    self.bytes.drain(..n);
                    remaining -= n;
                    self.last_progress = Some(now);
                }
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => break,
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(e),
            }
        }
        if self.bytes.is_empty() {
            self.last_progress = None;
            self.sink.flush()?;
        } else if now.duration_since(*self.last_progress.get_or_insert(now)) >= STALL_LIMIT {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "terminal output stalled for one second",
            ));
        }
        Ok(())
    }
}
impl Write for QueuedOutput<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > CAPACITY - self.bytes.len() {
            return Err(io::Error::other("terminal output queue exceeds 1 MiB"));
        }
        if !bytes.is_empty() && self.bytes.is_empty() {
            self.last_progress = Some(Instant::now());
        }
        self.bytes.extend(bytes);
        Ok(bytes.len())
    }
    // Rendering flushes enqueue only; the loop owns I/O and cancellation fairness.
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    enum Step {
        Bytes(usize),
        Block,
        Interrupt,
        Zero,
        Broken,
    }
    struct Sink {
        steps: VecDeque<Step>,
        written: Vec<u8>,
        fallback: usize,
    }
    impl Write for Sink {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            match self.steps.pop_front().unwrap_or(Step::Bytes(self.fallback)) {
                Step::Bytes(n) => {
                    let n = n.min(bytes.len());
                    self.written.extend_from_slice(&bytes[..n]);
                    Ok(n)
                }
                Step::Block => Err(io::ErrorKind::WouldBlock.into()),
                Step::Interrupt => Err(io::ErrorKind::Interrupted.into()),
                Step::Zero => Ok(0),
                Step::Broken => Err(io::ErrorKind::BrokenPipe.into()),
            }
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    fn make_sink(steps: impl IntoIterator<Item = Step>, fallback: usize) -> Sink {
        Sink {
            steps: steps.into_iter().collect(),
            written: vec![],
            fallback,
        }
    }
    #[test]
    fn partial_writes_and_backpressure_retain_exact_unicode_bytes_once() {
        let text = "雨\nquoted \"story\"\r\n";
        let mut sink = make_sink([Step::Bytes(1), Step::Interrupt, Step::Block], usize::MAX);
        let mut output = QueuedOutput::new(&mut sink);
        output.write_all(text.as_bytes()).unwrap();
        output.pump(Instant::now()).unwrap();
        assert_eq!(output.bytes.len(), text.len() - 1);
        output.write_all(b"tail").unwrap();
        output.pump(Instant::now()).unwrap();
        assert!(output.is_empty());
        drop(output);
        assert_eq!(sink.written, [text.as_bytes(), b"tail"].concat());
    }
    #[test]
    fn permanent_stall_and_trickling_progress_have_distinct_clock_behavior() {
        let mut sink = make_sink([Step::Block, Step::Bytes(1), Step::Block, Step::Block], 1);
        let mut output = QueuedOutput::new(&mut sink);
        output.write_all(b"abc").unwrap();
        let start = output.last_progress.unwrap();
        output.pump(start).unwrap();
        output.write_all(b"d").unwrap();
        assert_eq!(
            output.last_progress,
            Some(start),
            "enqueue must not reset stall clock"
        );
        output.pump(start + STALL_LIMIT / 2).unwrap();
        assert_eq!(
            output
                .pump(start + STALL_LIMIT + STALL_LIMIT / 2)
                .unwrap_err()
                .kind(),
            io::ErrorKind::TimedOut
        );
    }
    #[test]
    fn queue_overflow_write_zero_and_broken_pipe_are_explicit_errors() {
        let mut sink = make_sink([], usize::MAX);
        let mut output = QueuedOutput::new(&mut sink);
        assert!(
            output.write_all(&vec![b'x'; CAPACITY + 1]).is_err(),
            "oversized render burst must be rejected even with a writable sink"
        );
        assert!(output.is_empty());
        output.write_all(&vec![b'x'; CAPACITY]).unwrap();
        assert!(output.write_all(b"extra").is_err());
        assert_eq!(output.bytes.len(), CAPACITY);
        for (step, expected) in [
            (Step::Zero, io::ErrorKind::WriteZero),
            (Step::Broken, io::ErrorKind::BrokenPipe),
        ] {
            let mut sink = make_sink([step], usize::MAX);
            let mut output = QueuedOutput::new(&mut sink);
            output.write_all(b"bytes").unwrap();
            assert_eq!(output.pump(Instant::now()).unwrap_err().kind(), expected);
            assert_eq!(output.bytes.len(), 5);
        }
    }
    #[test]
    fn continuously_writable_output_and_interrupted_calls_have_finite_pump_budgets() {
        let mut sink = make_sink([], usize::MAX);
        let mut output = QueuedOutput::new(&mut sink);
        output.write_all(&vec![b'x'; CAPACITY]).unwrap();
        output.pump(Instant::now()).unwrap();
        assert_eq!(output.bytes.len(), CAPACITY - PUMP_BYTES);
        let mut sink = make_sink(
            std::iter::repeat_with(|| Step::Interrupt).take(PUMP_CALLS + 1),
            usize::MAX,
        );
        let mut output = QueuedOutput::new(&mut sink);
        output.write_all(b"a").unwrap();
        output.pump(Instant::now()).unwrap();
        assert_eq!(output.bytes.len(), 1);
    }
}
