//! Background polling-based streaming subscription for [`OpcDaClient<C, Bound>`].

use crate::client::OpcDaClient;
use crate::client::typestate::Bound;
use crate::connector::ServerBackend;
use crate::types::{IntoTags, TagValues};
use std::time::Duration;
use tokio::sync::mpsc::Receiver;

impl<C: ServerBackend + 'static> OpcDaClient<C, Bound> {
    /// Subscribes to a stream of tag value updates polled at the specified interval.
    ///
    /// Spawns a background Tokio task that periodically polls the configured tags on the bound
    /// server and streams updates through a Tokio [`Receiver`]. Dropping the receiver
    /// automatically terminates the background polling loop.
    ///
    /// # Arguments
    ///
    /// * `tags` - Tag batch or convertible source to poll. Accepts any type implementing [`IntoTags`]
    ///   (e.g., `&str`, `[&str; N]`, `&[&str]`, `&TagBatch`, or `Vec<String>`).
    /// * `interval` - Polling interval duration. Clamped to a minimum of 10 milliseconds to prevent
    ///   accidental worker starvation.
    ///
    /// # Returns
    ///
    /// A Tokio [`Receiver<TagValues>`] streaming polled tag updates on each tick.
    ///
    /// # Panics
    ///
    /// This function does not panic.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use opc_da_client::{Bound, DefaultOpcDaClient};
    /// # use std::time::Duration;
    /// # async fn run(client: &DefaultOpcDaClient<Bound>) {
    /// let mut rx = client.subscribe(["Sensor.1", "Sensor.2"], Duration::from_millis(500));
    /// if let Some(values) = rx.recv().await {
    ///     if let Some(value) = values.get_value("Sensor.1") {
    ///         let _ = value;
    ///     }
    /// }
    /// // Dropping `rx` shuts down the background polling loop.
    /// drop(rx);
    /// # }
    /// ```
    #[tracing::instrument(level = "info", skip(self, tags))]
    pub fn subscribe(&self, tags: impl IntoTags, interval: Duration) -> Receiver<TagValues> {
        let interval = interval.max(Duration::from_millis(10));
        let (tx, rx) = tokio::sync::mpsc::channel(16);
        let tags_batch = tags.into_tag_batch().into_shareable();
        let client = self.clone();

        tokio::spawn(async move {
            let mut timer = tokio::time::interval(interval);
            timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

            loop {
                timer.tick().await;
                if tx.is_closed() {
                    break;
                }
                match client.read_tags(tags_batch.clone()).await {
                    Ok(values) => {
                        if tx.send(values).await.is_err() {
                            break;
                        }
                    }
                    Err(err) => {
                        tracing::warn!(error = ?err, "Subscription polling tick failed");
                        if tx.is_closed() || err.is_connection_error() {
                            break;
                        }
                    }
                }
            }
        });

        rx
    }
}
