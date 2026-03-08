use std::collections::VecDeque;
use std::time::Duration;

use bevy::app::{App, Plugin, Update};
use bevy::ecs::message::{Message, Messages, MessageWriter};
use bevy::ecs::schedule::IntoScheduleConfigs;
use bevy::ecs::system::NonSendMut;
use bevy::time::common_conditions::on_timer;
use objc2_app_kit::NSPasteboard;
use tracing::debug;

const CLIPBOARD_POLL_MS: u64 = 500;
const DEFAULT_HISTORY_SIZE: usize = 50;

/// Message fired when the clipboard content changes.
#[derive(Clone, Debug, Message)]
#[allow(dead_code)] // Fields read by clipboard consumer systems (scripting, MCP).
pub struct ClipboardChanged {
    pub content: String,
}

/// Non-Send resource wrapping the NSPasteboard reference and tracking change count.
pub struct ClipboardState {
    pasteboard: objc2::rc::Retained<NSPasteboard>,
    last_change_count: isize,
    history: VecDeque<String>,
    max_history: usize,
}

impl ClipboardState {
    fn new() -> Self {
        let pasteboard = NSPasteboard::generalPasteboard();
        let last_change_count = pasteboard.changeCount();
        Self {
            pasteboard,
            last_change_count,
            history: VecDeque::new(),
            max_history: DEFAULT_HISTORY_SIZE,
        }
    }
}

/// Plugin for clipboard monitoring and automation.
/// Polls `NSPasteboard.generalPasteboard()` to detect changes and maintains
/// an in-memory clipboard history.
pub struct ClipboardPlugin;

impl Plugin for ClipboardPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Messages<ClipboardChanged>>();
        app.insert_non_send_resource(ClipboardState::new());
        app.add_systems(
            Update,
            poll_clipboard.run_if(on_timer(Duration::from_millis(CLIPBOARD_POLL_MS))),
        );
    }
}

fn poll_clipboard(
    mut state: NonSendMut<ClipboardState>,
    mut events: MessageWriter<ClipboardChanged>,
) {
    let current_count = state.pasteboard.changeCount();
    if current_count == state.last_change_count {
        return;
    }
    state.last_change_count = current_count;

    // Read string content from the pasteboard.
    let content = {
        use objc2_foundation::ns_string;
        state
            .pasteboard
            .stringForType(ns_string!("public.utf8-plain-text"))
    };

    if let Some(content) = content {
        let text = content.to_string();
        debug!("clipboard changed: {} chars", text.len());

        // Add to history.
        if state.history.len() >= state.max_history {
            state.history.pop_back();
        }
        state.history.push_front(text.clone());

        events.write(ClipboardChanged { content: text });
    }
}
