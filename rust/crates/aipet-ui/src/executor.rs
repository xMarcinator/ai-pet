//! The shells' async runtime: iced's tokio runtime, with one worker thread instead of one per core.
//!
//! iced's default executor is `tokio::runtime::Runtime::new()`, a worker thread per core (24 on a 24-thread CPU), each
//! with its stack and, under glibc, a share of the malloc arenas. What the pet runs on it is light: the frame timer
//! (`time::every`), the shells' event listeners and the Wayland shell's broadcasts, each a future that waits. One
//! worker runs them all; the blocking work (the core, the hook server, the file dialogs) has threads of its own.

/// The executor both shells hand iced (`.executor::<Executor>()`).
pub struct Executor(tokio::runtime::Runtime);

impl iced::Executor for Executor {
    fn new() -> Result<Self, iced::futures::io::Error> {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()
            .map(Executor)
    }

    fn spawn(&self, future: impl Future<Output = ()> + Send + 'static) {
        // the task runs detached, as with iced's own tokio executor
        drop(self.0.spawn(future));
    }

    fn block_on<T>(&self, future: impl Future<Output = T>) -> T {
        self.0.block_on(future)
    }

    fn enter<R>(&self, f: impl FnOnce() -> R) -> R {
        let _guard = self.0.enter();
        f()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use iced::Executor as _;

    #[test]
    fn it_runs_spawned_futures_and_timers_on_its_worker() {
        let executor = Executor::new().unwrap();
        let (tx, rx) = std::sync::mpsc::channel();
        executor.enter(|| {
            executor.spawn(async move {
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
                tx.send(std::thread::current().name().map(str::to_owned)).unwrap();
            })
        });
        // spawned work goes on while this thread does something else
        let worker = rx.recv_timeout(std::time::Duration::from_secs(5)).unwrap();
        assert_eq!(worker.as_deref(), Some("tokio-rt-worker"));
        assert_eq!(executor.block_on(async { 7 }), 7);
    }
}
