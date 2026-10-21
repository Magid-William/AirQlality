//! Tray -> widget refresh requests (EXP22).
//!
//! The tray lives on its own Win32 thread (EXP11) and may not touch gpui state:
//! it owns the icon, a static state cell and window messages. A menu item that
//! must *do something to the UI* therefore needs a way to wake a gpui task, and
//! a window message cannot carry it - the tray's callback lands on the tray's
//! own hidden window, whose procedure drops unknown `WM_APP` messages.
//!
//! So the widget publishes a sender here once (real mode only) and awaits the
//! other end in a single idle task; the tray thread calls [`request`]. The
//! channel is `bounded(1)`, which is the whole coalescing policy: clicking
//! Refresh ten times during one slow fetch leaves exactly one pending refresh.
//!
//! `async-channel` is already in the dependency graph (`gpui-ce` depends on it),
//! so this adds no download - the same reasoning as `windows` in EXP11.

use std::sync::Mutex;

use async_channel::{Receiver, Sender, TrySendError};

/// The sender the tray menu wakes; `None` until a widget publishes one, and
/// never in mock mode, where there is no real fetch to re-run.
static REQUESTS: Mutex<Option<Sender<()>>> = Mutex::new(None);

/// A fresh request channel: publish the sender, await the receiver.
pub fn channel() -> (Sender<()>, Receiver<()>) {
    async_channel::bounded(1)
}

/// Publish the sender the tray menu should wake.
pub fn publish(sender: Sender<()>) {
    let mut slot = REQUESTS.lock().unwrap_or_else(|e| e.into_inner());
    *slot = Some(sender);
}

/// Whether a [`request`] would reach a live widget (drives the menu's greyed
/// state).
pub fn available() -> bool {
    let slot = REQUESTS.lock().unwrap_or_else(|e| e.into_inner());
    listening(&slot)
}

/// Ask the widget to fetch now. `false` when nothing is listening (mock mode, or
/// the widget is gone). Never blocks: the tray thread calls this from the menu.
pub fn request() -> bool {
    let slot = REQUESTS.lock().unwrap_or_else(|e| e.into_inner());
    deliver(&slot)
}

fn listening(slot: &Option<Sender<()>>) -> bool {
    slot.as_ref().is_some_and(|sender| !sender.is_closed())
}

/// Hand one request to the listener, if there is one. `Full` already means a
/// refresh is queued, which is still a success. Separate from the global so the
/// contract is unit-testable without threads.
fn deliver(slot: &Option<Sender<()>>) -> bool {
    let Some(sender) = slot else {
        return false;
    };
    match sender.try_send(()) {
        Ok(()) => true,
        Err(TrySendError::Full(_)) => true,
        Err(TrySendError::Closed(_)) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_listener_is_a_no_op() {
        assert!(!deliver(&None));
        assert!(!listening(&None));
    }

    #[test]
    fn a_request_wakes_the_listener_once() {
        let (sender, receiver) = channel();
        assert!(listening(&Some(sender.clone())));
        assert!(deliver(&Some(sender)));
        assert!(receiver.try_recv().is_ok());
        assert!(receiver.try_recv().is_err(), "one request, one wake");
    }

    #[test]
    fn rapid_requests_coalesce_into_one_pending_refresh() {
        let (sender, receiver) = channel();
        for _ in 0..5 {
            assert!(deliver(&Some(sender.clone())));
        }
        assert!(receiver.try_recv().is_ok());
        assert!(
            receiver.try_recv().is_err(),
            "five clicks must not queue five refreshes"
        );
    }

    #[test]
    fn a_dropped_listener_is_not_a_listener() {
        let (sender, receiver) = channel();
        drop(receiver);
        assert!(!listening(&Some(sender.clone())));
        assert!(!deliver(&Some(sender)), "a send to nobody is a failure");
    }

    #[test]
    fn available_follows_the_published_sender() {
        // The global is shared by the whole test binary, so drive `listening`
        // through a local channel instead of mutating it: same predicate.
        let (sender, receiver) = channel();
        assert!(listening(&Some(sender.clone())));
        drop(receiver);
        assert!(!listening(&Some(sender)));
    }
}
