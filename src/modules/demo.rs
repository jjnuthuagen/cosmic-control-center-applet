//! Demo-only: toggle the popup when a trigger file is touched.
//!
//! Gated on the `COSMIC_CC_DEMO_TOGGLE` environment variable, which names the
//! trigger file. Normal sessions never set it, so this contributes nothing —
//! no poll, no subscription. With it set, `touch $COSMIC_CC_DEMO_TOGGLE` does
//! what a panel-button click does, which is what lets screenshot/recording
//! harnesses drive the popup in a sandboxed compositor where no real pointer
//! exists.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use cosmic::iced::Subscription;

use super::poll_subscription;

/// mtime (ms since epoch) of the trigger file at the previous poll.
/// 0 means "not armed yet": the first sighting only arms, so a file that
/// already existed at startup does not fire a phantom toggle.
static LAST: AtomicU64 = AtomicU64::new(0);

/// Like [`subscription`], but for the Settings window: yields the trimmed
/// file *content* on each touch of `$COSMIC_CC_DEMO_CMD`, so a harness can
/// write "styling" into it to switch tabs. Separate file and env var from the
/// popup toggle so touching one never fires the other.
pub fn commands() -> Subscription<String> {
    static CMD_LAST: AtomicU64 = AtomicU64::new(0);
    poll_subscription("demo-commands", Duration::from_millis(200), || async {
        let path = std::env::var("COSMIC_CC_DEMO_CMD").ok()?;
        let mtime = tokio::fs::metadata(&path)
            .await
            .ok()?
            .modified()
            .ok()?
            .duration_since(std::time::UNIX_EPOCH)
            .ok()?;
        let stamp = mtime.as_millis() as u64 + 1;
        let prev = CMD_LAST.swap(stamp, Ordering::Relaxed);
        if prev != 0 && stamp != prev {
            let content = tokio::fs::read_to_string(&path).await.ok()?;
            Some(content.trim().to_string())
        } else {
            None
        }
    })
}

pub fn subscription() -> Subscription<()> {
    poll_subscription("demo-toggle", Duration::from_millis(200), || async {
        let path = std::env::var("COSMIC_CC_DEMO_TOGGLE").ok()?;
        let mtime = tokio::fs::metadata(&path)
            .await
            .ok()?
            .modified()
            .ok()?
            .duration_since(std::time::UNIX_EPOCH)
            .ok()?;
        // +1 so a real mtime can never collide with the "not armed" sentinel.
        let stamp = mtime.as_millis() as u64 + 1;
        let prev = LAST.swap(stamp, Ordering::Relaxed);
        (prev != 0 && stamp != prev).then_some(())
    })
}
