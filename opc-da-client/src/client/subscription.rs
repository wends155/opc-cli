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
    /// server and streams updates through a Tokio [`mpsc::Receiver`]. Dropping the receiver
    /// automatically terminates the background polling loop.
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
