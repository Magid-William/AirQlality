//! Tray -> widget commands (EXP22 refresh, EXP24 API keys).
//!
//! The tray lives on its own Win32 thread (EXP11) and may not touch gpui state:
//! it owns the icon, a static state cell and window messages. A menu item that
//! must *do something to the UI* therefore needs a way to wake a gpui task, and
//! a window message cannot carry it - the tray's callback lands on the tray's
//! own hidden window, whose procedure drops unknown `WM_APP` messages.
//!
//! So the widget publishes a sender here once and awaits the other end in a
//! single idle task; the tray thread calls [`request`]. Two commands share the
//! channel, so a queued Refresh can never swallow an "API keys..." click
//! (EXP24). Coalescing is per command: asking twice while one is still pending
//! leaves exactly one, and the widget calls [`done`] after handling it.
//!
//! `async-channel` is already in the dependency graph (`gpui-ce` depends on it),
//! so this adds no download - the same reasoning as `windows` in EXP11.

use std::sync::Mutex;

use async_channel::{Receiver, Sender};

/// What the tray can ask the widget to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    /// Re-run the whole real fetch round now (EXP22).
    Refresh,
    /// Open the first key prompt (EXP24).
    ApiKeys,
}

impl Command {
    fn slot(self) -> usize {
        match self {
            Command::Refresh => 0,
            Command::ApiKeys => 1,
        }
    }
}

/// The published sender plus the two per-command flags.
struct Hub {
    sender: Option<Sender<Command>>,
    /// A request of this kind is queued and not yet handled.
    pending: [bool; 2],
    /// Whether this command means anything right now (Refresh is off under the
    /// mock seams, where there is no real fetch to re-run).
    enabled: [bool; 2],
}

impl Hub {
    fn new() -> Self {
        Self {
            sender: None,
            pending: [false; 2],
            enabled: [true; 2],
        }
    }
}

static HUB: Mutex<Option<Hub>> = Mutex::new(None);

/// A fresh request channel: publish the sender, await the receiver.
pub fn channel() -> (Sender<Command>, Receiver<Command>) {
    async_channel::unbounded()
}

/// Publish the sender the tray menu should wake. `refresh_enabled` is false
/// under the mock seams, which greys the Refresh item (EXP22).
pub fn publish(sender: Sender<Command>, refresh_enabled: bool) {
    let mut slot = HUB.lock().unwrap_or_else(|e| e.into_inner());
    let hub = slot.get_or_insert_with(Hub::new);
    hub.sender = Some(sender);
    hub.pending = [false; 2];
    hub.enabled = [refresh_enabled, true];
}

/// Whether a [`request`] for this command would reach a live widget. Drives the
/// menu's greyed state.
pub fn available(command: Command) -> bool {
    let slot = HUB.lock().unwrap_or_else(|e| e.into_inner());
    slot.as_ref().is_some_and(|hub| reachable(hub, command))
}

/// Ask the widget to handle `command`. `false` when nothing is listening or the
/// command is off right now. Never blocks: the tray thread calls this from the
/// menu.
pub fn request(command: Command) -> bool {
    let mut slot = HUB.lock().unwrap_or_else(|e| e.into_inner());
    match slot.as_mut() {
        Some(hub) => deliver(hub, command),
        None => false,
    }
}

/// The widget has handled `command`; the next request of this kind may pass.
pub fn done(command: Command) {
    let mut slot = HUB.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(hub) = slot.as_mut() {
        hub.pending[command.slot()] = false;
    }
}

fn reachable(hub: &Hub, command: Command) -> bool {
    hub.enabled[command.slot()]
        && hub
            .sender
            .as_ref()
            .is_some_and(|sender| !sender.is_closed())
}

/// Hand one request to the listener, unless the same command is already queued.
/// Separate from the global so the contract is unit-testable without threads.
fn deliver(hub: &mut Hub, command: Command) -> bool {
    if !reachable(hub, command) {
        return false;
    }
    if hub.pending[command.slot()] {
        // Already queued: still a success, and it must not queue a second one.
        return true;
    }
    hub.pending[command.slot()] = true;
    let Some(sender) = hub.sender.as_ref() else {
        return false;
    };
    match sender.try_send(command) {
        Ok(()) => true,
        Err(_) => {
            hub.pending[command.slot()] = false;
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hub_with(sender: Sender<Command>) -> Hub {
        Hub {
            sender: Some(sender),
            pending: [false; 2],
            enabled: [true; 2],
        }
    }

    #[test]
    fn no_listener_is_a_no_op() {
        let mut hub = Hub::new();
        assert!(!reachable(&hub, Command::Refresh));
        assert!(!deliver(&mut hub, Command::Refresh));
        assert!(!deliver(&mut hub, Command::ApiKeys));
    }

    #[test]
    fn a_request_wakes_the_listener_once() {
        let (sender, receiver) = channel();
        let mut hub = hub_with(sender);
        assert!(deliver(&mut hub, Command::Refresh));
        assert_eq!(receiver.try_recv().ok(), Some(Command::Refresh));
        assert!(receiver.try_recv().is_err(), "one request, one wake");
    }

    #[test]
    fn a_repeat_while_pending_coalesces_but_another_command_still_gets_through() {
        let (sender, receiver) = channel();
        let mut hub = hub_with(sender);
        assert!(deliver(&mut hub, Command::Refresh));
        assert!(deliver(&mut hub, Command::Refresh), "queued is still a success");
        assert!(deliver(&mut hub, Command::ApiKeys), "a click must not be swallowed");
        assert_eq!(receiver.try_recv().ok(), Some(Command::Refresh));
        assert_eq!(receiver.try_recv().ok(), Some(Command::ApiKeys));
        assert!(receiver.try_recv().is_err(), "the repeat queued nothing");
    }

    #[test]
    fn handling_a_command_lets_the_next_one_in() {
        let (sender, receiver) = channel();
        let mut hub = hub_with(sender);
        assert!(deliver(&mut hub, Command::Refresh));
        hub.pending[Command::Refresh.slot()] = false; // what `done` does
        assert!(deliver(&mut hub, Command::Refresh));
        assert_eq!(receiver.try_recv().ok(), Some(Command::Refresh));
        assert_eq!(receiver.try_recv().ok(), Some(Command::Refresh));
    }

    #[test]
    fn a_disabled_command_is_not_reachable() {
        let (sender, receiver) = channel();
        let mut hub = hub_with(sender);
        hub.enabled[Command::Refresh.slot()] = false;
        assert!(!reachable(&hub, Command::Refresh));
        assert!(!deliver(&mut hub, Command::Refresh));
        assert!(deliver(&mut hub, Command::ApiKeys), "keys stay available");
        assert!(receiver.try_recv().is_ok());
    }

    #[test]
    fn a_dropped_listener_is_not_a_listener() {
        let (sender, receiver) = channel();
        let hub = Hub {
            sender: Some(sender),
            pending: [false; 2],
            enabled: [true; 2],
        };
        drop(receiver);
        assert!(!reachable(&hub, Command::Refresh));
    }
}
