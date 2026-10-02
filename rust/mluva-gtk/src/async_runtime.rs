//! Poll non-Send desktop futures on GLib while Tokio owns background I/O.

use std::{
    future::{Future, poll_fn},
    pin::pin,
    rc::Rc,
};

pub struct DesktopRuntime {
    runtime: Option<tokio::runtime::Runtime>,
}
impl DesktopRuntime {
    pub fn new() -> std::io::Result<Rc<Self>> {
        Ok(Rc::new(Self {
            runtime: Some(
                tokio::runtime::Builder::new_multi_thread()
                    .worker_threads(2)
                    .thread_name("mluva-provider-io")
                    .enable_all()
                    .build()?,
            ),
        }))
    }

    /// GTK snapshots and delivery remain on their owner. Enter Tokio separately
    /// for each poll; never keep a runtime guard across an await or block GLib.
    pub fn spawn<F: Future + 'static>(self: &Rc<Self>, future: F) -> glib::JoinHandle<F::Output>
    where
        F::Output: 'static,
    {
        let owner = self.clone();
        glib::MainContext::default().spawn_local(async move {
            let mut future = pin!(future);
            poll_fn(|context| {
                let _entered = owner
                    .runtime
                    .as_ref()
                    .expect("live desktop runtime")
                    .enter();
                future.as_mut().poll(context)
            })
            .await
        })
    }

    /// Owned downloads and hashing run away from the GTK context; their task
    /// retains cleanup even after the corresponding page is gone.
    pub fn spawn_background<F>(&self, future: F) -> tokio::task::JoinHandle<F::Output>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        self.runtime
            .as_ref()
            .expect("live desktop runtime")
            .spawn(future)
    }
}
impl Drop for DesktopRuntime {
    fn drop(&mut self) {
        if let Some(runtime) = self.runtime.take() {
            runtime.shutdown_background();
        }
    }
}
