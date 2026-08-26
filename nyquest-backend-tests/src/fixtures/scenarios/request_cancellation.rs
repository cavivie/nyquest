#[cfg(test)]
mod tests {
    use std::future::pending;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use hyper::Response;
    use nyquest::Request as NyquestRequest;
    use tokio::sync::oneshot;

    use crate::*;

    struct NotifyOnDrop(Option<oneshot::Sender<()>>);

    impl Drop for NotifyOnDrop {
        fn drop(&mut self) {
            if let Some(sender) = self.0.take() {
                let _ = sender.send(());
            }
        }
    }

    #[test]
    fn test_dropping_pending_request_cancels_native_task() {
        const PATH: &str = "scenarios/request_cancellation/pending_headers";

        let (started_tx, started_rx) = oneshot::channel();
        let started_tx = Arc::new(Mutex::new(Some(started_tx)));
        let (dropped_tx, dropped_rx) = oneshot::channel();
        let dropped_tx = Arc::new(Mutex::new(Some(dropped_tx)));
        let _handle = crate::add_hyper_fixture(PATH, {
            let started_tx = Arc::clone(&started_tx);
            let dropped_tx = Arc::clone(&dropped_tx);
            move |_req| {
                let started_tx = Arc::clone(&started_tx);
                let dropped_tx = Arc::clone(&dropped_tx);
                async move {
                    started_tx.lock().unwrap().take().unwrap().send(()).ok();
                    let _notify_on_drop = NotifyOnDrop(dropped_tx.lock().unwrap().take());
                    pending::<()>().await;
                    (Response::new(Full::default()), Ok(()))
                }
            }
        });

        TOKIO_RT.block_on(async {
            let client = crate::init_builder()
                .await
                .unwrap()
                .build_async()
                .await
                .unwrap();
            let mut request = Box::pin(client.request(NyquestRequest::get(PATH)));

            tokio::select! {
                response = &mut request => panic!("request completed before cancellation: {response:?}"),
                started = started_rx => started.unwrap(),
            }
            drop(request);

            tokio::time::timeout(Duration::from_secs(5), dropped_rx)
                .await
                .expect("native request was not cancelled")
                .unwrap();
        });
    }
}
