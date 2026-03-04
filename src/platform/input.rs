use core::ptr::NonNull;
use objc2::rc::Retained;
use objc2_app_kit::{NSEvent, NSEventType, NSTouch, NSTouchPhase};
use objc2_core_foundation::{CFMachPort, CFRetained, CFRunLoop, CGPoint, CGRect, kCFRunLoopCommonModes};
use objc2_core_graphics::{
    CGDirectDisplayID, CGDisplayBounds, CGEvent, CGEventField, CGEventFlags,
    CGEventTapLocation, CGEventTapOptions, CGEventTapPlacement, CGEventTapProxy, CGEventType,
    CGGetActiveDisplayList,
};
use objc2_foundation::NSSet;
use scopeguard::ScopeGuard;
use std::ffi::c_void;
use std::marker::PhantomPinned;
use std::pin::Pin;
use std::ptr::null_mut;
use std::time::Instant;
use stdext::function_name;
use tracing::{error, info};

use crate::config::Config;
use crate::errors::{Error, Result};
use crate::events::{Event, EventSender};
use crate::platform::Modifiers;

/// Which edge of a display the cursor is stuck to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum StickyEdge {
    Left,
    Right,
    Top,
    Bottom,
}

/// Tracks an active sticky-edge state during a drag near a display boundary.
struct StickyEdgeState {
    /// Which edge.
    edge: StickyEdge,
    /// When the cursor first hit this edge.
    entered_at: Instant,
}

/// Query active display bounds via Core Graphics.
fn get_active_display_bounds() -> Vec<(CGDirectDisplayID, CGRect)> {
    let mut count = 0u32;
    unsafe { CGGetActiveDisplayList(0, null_mut(), &raw mut count) };
    if count == 0 {
        return vec![];
    }
    let mut ids = Vec::with_capacity(count as usize);
    unsafe {
        CGGetActiveDisplayList(count, ids.as_mut_ptr(), &raw mut count);
        ids.set_len(count as usize);
    }
    ids.into_iter()
        .map(|id| (id, CGDisplayBounds(id)))
        .collect()
}

/// `InputHandler` manages low-level input events from the macOS `CGEventTap`.
/// It intercepts keyboard and mouse events, processes gestures, and dispatches them as higher-level `Event`s.
pub(super) struct InputHandler {
    /// The `EventSender` for dispatching input events.
    events: Option<EventSender>,
    /// The application `Config` for looking up keybindings.
    config: Config,
    /// Stores the previous touch positions for swipe gesture detection.
    finger_position: Option<Retained<NSSet<NSTouch>>>,
    /// The `CFMachPort` representing the `CGEventTap`.
    tap_port: Option<CFRetained<CFMachPort>>,
    /// Active sticky edge state during drag near a display boundary.
    sticky_edge: Option<StickyEdgeState>,
    // Prevents from being Unpin automatically
    _pin: PhantomPinned,
}

pub(super) type PinnedInputHandler =
    ScopeGuard<Pin<Box<InputHandler>>, Box<dyn FnOnce(Pin<Box<InputHandler>>)>>;

impl InputHandler {
    /// Creates a new `InputHandler` instance.
    ///
    /// # Arguments
    ///
    /// * `events` - An `EventSender` to send input-related events.
    /// * `config` - The `Config` object for looking up keybindings.
    ///
    /// # Returns
    ///
    /// A new `InputHandler`.
    pub(super) fn new(events: EventSender, config: Config) -> Self {
        InputHandler {
            events: Some(events),
            config,
            finger_position: None,
            tap_port: None,
            sticky_edge: None,
            _pin: PhantomPinned,
        }
    }

    /// Starts the input handler by creating and enabling a `CGEventTap`. It also sets up a cleanup hook.
    ///
    /// # Returns
    ///
    /// `Ok(())` if the event tap is created and started successfully, otherwise `Err(Error)`.
    pub(super) fn start(self) -> Result<PinnedInputHandler> {
        let mouse_event_mask = (1 << CGEventType::MouseMoved.0)
            | (1 << CGEventType::LeftMouseDown.0)
            | (1 << CGEventType::LeftMouseUp.0)
            | (1 << CGEventType::LeftMouseDragged.0)
            | (1 << CGEventType::RightMouseDown.0)
            | (1 << CGEventType::RightMouseUp.0)
            | (1 << CGEventType::RightMouseDragged.0)
            | (1 << NSEventType::Gesture.0)
            | (1 << CGEventType::KeyDown.0);

        let mut pinned = Box::pin(self);
        let this = unsafe { NonNull::new_unchecked(pinned.as_mut().get_unchecked_mut()) }.as_ptr();
        unsafe {
            (*this).tap_port = CGEvent::tap_create(
                CGEventTapLocation::HIDEventTap,
                CGEventTapPlacement::HeadInsertEventTap,
                CGEventTapOptions::Default,
                mouse_event_mask,
                Some(Self::callback),
                this.cast(),
            );
        };
        if pinned.tap_port.is_none() {
            return Err(Error::PermissionDenied(format!(
                "{}: Can not create EventTap.",
                function_name!()
            )));
        }

        let (run_loop_source, main_loop) =
            CFMachPort::new_run_loop_source(None, pinned.tap_port.as_deref(), 0)
                .zip(CFRunLoop::main())
                .ok_or(Error::PermissionDenied(format!(
                    "{}: Unable to create run loop source",
                    function_name!()
                )))?;
        let loop_mode = unsafe { kCFRunLoopCommonModes };
        CFRunLoop::add_source(&main_loop, Some(&run_loop_source), loop_mode);

        let port = pinned
            .tap_port
            .clone()
            .ok_or(Error::PermissionDenied(format!(
                "{}: invalid tap port.",
                function_name!()
            )))?;
        Ok(scopeguard::guard(
            pinned,
            Box::new(move |_: Pin<Box<Self>>| {
                info!("Unregistering event_handler");
                CFRunLoop::remove_source(&main_loop, Some(&run_loop_source), loop_mode);
                CFMachPort::invalidate(&port);
                CGEvent::tap_enable(&port, false);
            }),
        ))
    }

    /// The C-callback function for the `CGEventTap`. It dispatches to the `input_handler` method.
    /// This function is declared as `extern "C-unwind"`.
    ///
    /// # Arguments
    ///
    /// * `_` - The `CGEventTapProxy` (unused).
    /// * `event_type` - The `CGEventType` of the event.
    /// * `event_ref` - A mutable `NonNull` pointer to the `CGEvent`.
    /// * `this` - A raw pointer to the `InputHandler` instance.
    ///
    /// # Returns
    ///
    /// A raw mutable pointer to `CGEvent`. Returns `null_mut()` if the event is intercepted.
    extern "C-unwind" fn callback(
        _: CGEventTapProxy,
        event_type: CGEventType,
        mut event_ref: NonNull<CGEvent>,
        this: *mut c_void,
    ) -> *mut CGEvent {
        if let Some(this) =
            NonNull::new(this).map(|this| unsafe { this.cast::<InputHandler>().as_mut() })
        {
            let intercept = this.input_handler(event_type, unsafe { event_ref.as_ref() });
            if intercept {
                return null_mut();
            }
        } else {
            error!("Zero passed to Event Handler.");
        }
        unsafe { event_ref.as_mut() }
    }

    /// Handles various input events received from the `CGEventTap` callback. It sends corresponding `Event`s.
    ///
    /// # Arguments
    ///
    /// * `event_type` - The `CGEventType` of the event.
    /// * `event` - A reference to the `CGEvent`.
    ///
    /// # Returns
    ///
    /// `true` if the event should be intercepted (not passed further), `false` otherwise.
    fn input_handler(&mut self, event_type: CGEventType, event: &CGEvent) -> bool {
        // Handle sticky edge state mutations before borrowing self.events.
        match event_type {
            CGEventType::LeftMouseUp | CGEventType::RightMouseUp => {
                self.sticky_edge = None;
            }
            CGEventType::LeftMouseDragged | CGEventType::RightMouseDragged => {
                self.clamp_sticky_edge(event);
            }
            _ => {}
        }

        let Some(events) = &self.events else {
            return false;
        };
        let result = match event_type {
            CGEventType::TapDisabledByTimeout | CGEventType::TapDisabledByUserInput => {
                info!("Tap Disabled");
                if let Some(port) = &self.tap_port {
                    CGEvent::tap_enable(port, true);
                }
                Ok(())
            }
            CGEventType::LeftMouseDown | CGEventType::RightMouseDown => {
                let point = CGEvent::location(Some(event));
                events.send(Event::MouseDown { point })
            }
            CGEventType::LeftMouseUp | CGEventType::RightMouseUp => {
                let point = CGEvent::location(Some(event));
                events.send(Event::MouseUp { point })
            }
            CGEventType::LeftMouseDragged | CGEventType::RightMouseDragged => {
                let point = CGEvent::location(Some(event));
                events.send(Event::MouseDragged { point })
            }
            CGEventType::MouseMoved => {
                let point = CGEvent::location(Some(event));
                events.send(Event::MouseMoved { point })
            }
            CGEventType::KeyDown => {
                let keycode =
                    CGEvent::integer_value_field(Some(event), CGEventField::KeyboardEventKeycode);
                let eventflags = CGEvent::flags(Some(event));
                // handle_keypress can intercept the event, so it may return true.
                return self.handle_keypress(keycode, eventflags);
            }
            _ => {
                if self.should_suppress_gesture(event) {
                    return true;
                }
                self.handle_swipe(event)
            }
        };
        if let Err(err) = result {
            error!("error sending event: {err}");
            // The socket is dead, so no use trying to send to it.
            // Trigger cleanup destructor, unregistering the handler.
            self.events = None;
        }
        // Do not intercept this event, let it fall through.
        false
    }

    /// Returns true if this gesture should be suppressed based on finger count.
    fn should_suppress_gesture(&self, event: &CGEvent) -> bool {
        if !self.config.suppress_gestures() {
            return false;
        }
        let Some(ns_event) = NSEvent::eventWithCGEvent(event) else {
            return false;
        };
        if ns_event.r#type() != NSEventType::Gesture {
            return false;
        }
        let finger_count = ns_event.allTouches().len();
        let opts = self.config.options();
        match finger_count {
            4 => opts.gesture_suppress.four_finger.unwrap_or(false),
            5 => {
                opts.gesture_suppress.five_finger_pinch.unwrap_or(false)
                    || opts.gesture_suppress.five_finger_spread.unwrap_or(false)
            }
            _ => false,
        }
    }

    /// Handles swipe gesture events.
    /// It calculates the delta of the swipe and sends a `Swipe` event.
    ///
    /// # Arguments
    ///
    /// * `event` - A reference to the `CGEvent`.
    ///
    /// # Returns
    ///
    /// `Ok(())` if the event is processed successfully, otherwise `Err(Error)`.
    fn handle_swipe(&mut self, event: &CGEvent) -> Result<()> {
        const SWIPE_THRESHOLD: f64 = 0.001;
        const GESTURE_MINIMAL_FINGERS: usize = 3;
        let Some(ns_event) = NSEvent::eventWithCGEvent(event) else {
            return Err(Error::InvalidInput(format!(
                "{}: Unable to convert {event:?} to NSEvent.",
                function_name!()
            )));
        };
        if ns_event.r#type() != NSEventType::Gesture {
            return Ok(());
        }
        let fingers = ns_event.allTouches();
        if fingers.len() < GESTURE_MINIMAL_FINGERS {
            return Ok(());
        }

        if fingers.iter().all(|f| f.phase() != NSTouchPhase::Began)
            && let Some(prev) = &self.finger_position
        {
            let deltas = prev
                .iter()
                .zip(&fingers)
                .map(|(p, c)| p.normalizedPosition().x - c.normalizedPosition().x)
                .collect::<Vec<_>>();
            if deltas.iter().all(|p| p.abs() > SWIPE_THRESHOLD)
                && let Some(events) = &self.events
            {
                _ = events.send(Event::Swipe { deltas });
            }
        }
        self.finger_position = Some(fingers);
        Ok(())
    }

    /// Handles key press events. It determines the modifier mask and attempts to find a matching keybinding in the configuration.
    /// If a binding is found, it sends a `Command` event and intercepts the key press.
    ///
    /// # Arguments
    ///
    /// * `keycode` - The key code of the pressed key.
    /// * `eventflags` - The `CGEventFlags` representing active modifiers.
    ///
    /// # Returns
    ///
    /// `true` if the key press was handled and should be intercepted, `false` otherwise.
    fn handle_keypress(&self, keycode: i64, eventflags: CGEventFlags) -> bool {
        const MODIFIER_MASKS: [(Modifiers, [u64; 3]); 4] = [
            (Modifiers::ALT, [0x0008_0000, 0x0000_0020, 0x0000_0040]),
            (Modifiers::SHIFT, [0x0002_0000, 0x0000_0002, 0x0000_0004]),
            (Modifiers::CMD, [0x0010_0000, 0x0000_0008, 0x0000_0010]),
            (Modifiers::CTRL, [0x0004_0000, 0x0000_0001, 0x0000_2000]),
        ];
        let Some(events) = &self.events else {
            return false;
        };

        let mut mask = Modifiers::empty();
        for (modifier, masks) in MODIFIER_MASKS {
            #[allow(clippy::manual_contains)]
            if masks.iter().any(|&m| m == (eventflags.0 & m)) {
                mask |= modifier;
            }
        }

        // On a native fullscreen space, keybindings are still intercepted so
        // that karakuri can actively switch back to the previous workspace.
        // Non-karakuri keys pass through naturally (find_keybind returns None).

        let keycode = keycode.try_into().ok();
        keycode
            .and_then(|keycode| self.config.find_keybind(keycode, &mask))
            .and_then(|command| {
                events
                    .send(Event::Command { command })
                    .inspect_err(|err| error!("Error sending command: {err}"))
                    .ok()
            })
            .is_some()
    }

    /// During `MouseDragged`, if the cursor is at a display edge with a snap
    /// zone enabled and another display is adjacent, clamp the event position
    /// to prevent crossing for the configured dwell period.
    fn clamp_sticky_edge(&mut self, event: &CGEvent) {
        if !self.config.edge_snap_any_enabled() {
            self.sticky_edge = None;
            return;
        }
        let dwell = self.config.edge_snap_sticky_dwell();
        if dwell.is_zero() {
            self.sticky_edge = None;
            return;
        }

        let point = CGEvent::location(Some(event));
        let threshold = f64::from(self.config.edge_snap_threshold());
        let displays = get_active_display_bounds();

        if displays.len() < 2 {
            self.sticky_edge = None;
            return;
        }

        // Find which display contains the cursor.
        let current = displays.iter().find(|(_, b)| {
            point.x >= b.origin.x
                && point.x < b.origin.x + b.size.width
                && point.y >= b.origin.y
                && point.y < b.origin.y + b.size.height
        });

        let Some(&(_, current_bounds)) = current else {
            self.sticky_edge = None;
            return;
        };

        let snap_cfg = self.config.edge_snap();

        // Check each edge: is the cursor near it, is a snap zone enabled for
        // that edge, and is there an adjacent display on that side?
        let near_right = snap_cfg.right.unwrap_or(false)
            && (current_bounds.origin.x + current_bounds.size.width - point.x) < threshold
            && displays.iter().any(|(_, b)| {
                (b.origin.x - (current_bounds.origin.x + current_bounds.size.width)).abs() < 2.0
            });

        let near_left = snap_cfg.left.unwrap_or(false)
            && (point.x - current_bounds.origin.x) < threshold
            && displays.iter().any(|(_, b)| {
                ((b.origin.x + b.size.width) - current_bounds.origin.x).abs() < 2.0
            });

        let near_bottom = snap_cfg.bottom.unwrap_or(false)
            && (current_bounds.origin.y + current_bounds.size.height - point.y) < threshold
            && displays.iter().any(|(_, b)| {
                (b.origin.y - (current_bounds.origin.y + current_bounds.size.height)).abs() < 2.0
            });

        let near_top = snap_cfg.top.unwrap_or(false)
            && (point.y - current_bounds.origin.y) < threshold
            && displays.iter().any(|(_, b)| {
                ((b.origin.y + b.size.height) - current_bounds.origin.y).abs() < 2.0
            });

        let detected_edge = if near_right {
            Some(StickyEdge::Right)
        } else if near_left {
            Some(StickyEdge::Left)
        } else if near_bottom {
            Some(StickyEdge::Bottom)
        } else if near_top {
            Some(StickyEdge::Top)
        } else {
            None
        };

        let Some(edge) = detected_edge else {
            self.sticky_edge = None;
            return;
        };

        // Start or continue the sticky state.
        let state = self.sticky_edge.get_or_insert_with(|| StickyEdgeState {
            edge,
            entered_at: Instant::now(),
        });

        // If the edge changed, reset.
        if state.edge != edge {
            *state = StickyEdgeState {
                edge,
                entered_at: Instant::now(),
            };
        }

        // If dwell has expired, let through.
        if state.entered_at.elapsed() >= dwell {
            self.sticky_edge = None;
            return;
        }

        // Clamp the cursor to stay within the current display.
        let clamped = match edge {
            StickyEdge::Right => CGPoint::new(
                current_bounds.origin.x + current_bounds.size.width - 1.0,
                point.y,
            ),
            StickyEdge::Left => CGPoint::new(current_bounds.origin.x, point.y),
            StickyEdge::Bottom => CGPoint::new(
                point.x,
                current_bounds.origin.y + current_bounds.size.height - 1.0,
            ),
            StickyEdge::Top => CGPoint::new(point.x, current_bounds.origin.y),
        };

        CGEvent::set_location(Some(event), clamped);
    }
}
