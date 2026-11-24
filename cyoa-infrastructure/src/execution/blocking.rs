//! Scoped one-shot execution for startup and repository queries.
use cyoa_application::{
    cancellation::{CancellationSource, CancellationToken},
    execution::BlockingExecution,
};
use std::{io, time::Duration};

pub struct ScopedJobs;
impl BlockingExecution for ScopedJobs {
    fn run<T: Send, E: std::error::Error + Send + 'static>(
        &self,
        operation: &str,
        job: impl FnOnce(&CancellationToken) -> Result<T, E> + Send,
        mut cancel_requested: impl FnMut() -> bool,
    ) -> Result<T, Box<dyn std::error::Error>> {
        let source = CancellationSource::default();
        let token = source.token();
        std::thread::scope(|scope| {
            let worker = scope.spawn(|| job(&token));
            while !worker.is_finished() {
                if cancel_requested() {
                    source.cancel();
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            let value = worker
                .join()
                .map_err(|_| io::Error::other(format!("{operation} worker panicked")))??;
            if cancel_requested() || token.is_cancelled() {
                return Err(
                    io::Error::other(format!("{operation} cancelled after cleanup")).into(),
                );
            }
            Ok(value)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn run_interruptible<T: Send, E: std::error::Error + Send + 'static>(
        operation: &str,
        job: impl FnOnce(&CancellationToken) -> Result<T, E> + Send,
    ) -> Result<T, Box<dyn std::error::Error>> {
        ScopedJobs.run(operation, job, || false)
    }
    #[test]
    fn interruptible_jobs_return_values_and_name_their_operation_in_failures() {
        assert_eq!(
            run_interruptible("save listing", |_| Ok::<_, io::Error>(7)).unwrap(),
            7
        );
        let error = run_interruptible("save listing", |_| {
            Err::<(), _>(io::Error::other("helper failed"))
        })
        .unwrap_err();
        assert_eq!(
            error.to_string(),
            "helper failed",
            "the job's own error is kept"
        );
        let panicked = run_interruptible("startup load", |_| -> Result<(), io::Error> {
            panic!("simulated worker fault")
        })
        .unwrap_err();
        assert_eq!(panicked.to_string(), "startup load worker panicked");
    }

    #[test]
    fn scoped_job_cancellation_rejects_result_after_joined_cleanup() {
        let (ready, observed) = std::sync::mpsc::channel();
        let result = ScopedJobs.run(
            "save listing",
            move |token| {
                let (wake, wait) = std::sync::mpsc::channel();
                let _registration = token.subscribe(move || {
                    let _ = wake.send(());
                });
                ready.send(()).unwrap();
                wait.recv_timeout(Duration::from_secs(30)).unwrap();
                Ok::<_, io::Error>(7)
            },
            || observed.try_recv().is_ok(),
        );
        assert_eq!(
            result.unwrap_err().to_string(),
            "save listing cancelled after cleanup"
        );
    }
}
