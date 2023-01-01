#[derive(Debug)]
pub struct TaskControl(
    tokio::sync::oneshot::Sender<()>,
    tokio::task::JoinHandle<()>,
);

impl TaskControl {
    pub fn new(
        stop: tokio::sync::oneshot::Sender<()>,
        handle: tokio::task::JoinHandle<()>,
    ) -> TaskControl {
        TaskControl(stop, handle)
    }

    pub async fn stop(self) {
        if !self.1.is_finished() && self.0.send(()).is_ok() {
            let _ = self.1.await;
        }
    }

    pub async fn wait(self) {
        let _ = self.1.await;
    }
}

pub fn upgrade_oneshot<T: Clone + Send + 'static>(
    rx: tokio::sync::oneshot::Receiver<T>,
) -> tokio::sync::broadcast::Receiver<T> {
    let (broadcast_tx, broadcast_rx) = tokio::sync::broadcast::channel(1);
    tokio::spawn(relay(rx, broadcast_tx));
    broadcast_rx
}

async fn relay<T>(rx: tokio::sync::oneshot::Receiver<T>, tx: tokio::sync::broadcast::Sender<T>) {
    let _ = tx.send(rx.await.unwrap());
}
