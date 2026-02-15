use anyhow::Result;
use async_trait::async_trait;
#[cfg(test)]
use mockall::automock;
use std::sync::atomic::AtomicUsize;
use std::sync::Arc;

#[cfg_attr(test, automock)]
#[async_trait]
pub trait OpcProvider: Send + Sync {
    /// List available OPC DA servers on the given host.
    async fn list_servers(&self, host: &str) -> Result<Vec<String>>;

    /// Browse all tags on the specified server by recursively walking branches.
    ///
    /// Handles both flat and hierarchical address spaces automatically.
    /// Returns fully-qualified item IDs up to `max_tags` entries.
    /// Increments `progress` atomically as tags are discovered.
    async fn browse_tags(
        &self,
        server: &str,
        max_tags: usize,
        progress: Arc<AtomicUsize>,
    ) -> Result<Vec<String>>;
}
