use crate::cdn::CacheManager;
use std::sync::Arc;
use futures::StreamExt;
use tracing::info;

pub async fn purge_loop(
    mut subscription: async_nats::Subscriber,
    cache: Arc<CacheManager>,
) {
    info!("CDN invalidator: listening for index.invalidated.>");

    while let Some(msg) = subscription.next().await {
        let subject = msg.subject.clone();
        info!(%subject, "CDN invalidation received");

        if let Some(path) = subject.strip_prefix("index.invalidated.") {
            let prefix = path.replace('.', "/");
            cache.purge_prefix(&prefix).await;
            info!(prefix, "CDN cache invalidated");
        }
    }
}
