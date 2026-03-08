use std::sync::{Arc, Mutex, OnceLock, RwLock};
use std::time::Duration;

use super::*;
use bevy::prelude::*;
use bevy::tasks::{AsyncComputeTaskPool, TaskPoolBuilder};
use bevy::time::TimeUpdateStrategy;

/// Bevy's global thread pools (ComputeTaskPool, IoTaskPool) and MinimalPlugins
/// are not safe to initialize from multiple threads simultaneously. This mutex
/// serializes all integration tests that create a Bevy App.
static TEST_MUTEX: Mutex<()> = Mutex::new(());
use objc2_core_foundation::{CFRetained, CGPoint};
use objc2_core_graphics::CGDirectDisplayID;
use stdext::function_name;
use stdext::prelude::RwLockExt;
use tracing::{Level, debug, instrument};

use crate::commands::{Command, Direction, Operation};
use crate::config::Config;
use crate::ecs::{
    BProcess, ExistingMarker, FocusedMarker, PollForNotifications, SpawnWindowTrigger,
    state::{AppPhase, FocusContext, InteractionMode},
};
use crate::plugins::WindowPlugin;
use crate::errors::{Error, Result};
use crate::events::Event;
use crate::manager::{
    Application, ApplicationApi, Display, Origin, ProcessApi, Size, Window, WindowApi,
    WindowManager, WindowManagerApi,
};
use crate::platform::{ConnID, Pid, WinID, WorkspaceId};
use crate::{platform::ProcessSerialNumber, util::AXUIWrapper};

const TEST_PROCESS_ID: i32 = 1;
const TEST_DISPLAY_ID: u32 = 1;
const TEST_WORKSPACE_ID: u64 = 2;
const TEST_DISPLAY_WIDTH: i32 = 1024;
const TEST_DISPLAY_HEIGHT: i32 = 768;

const TEST_MENUBAR_HEIGHT: i32 = 20;
const TEST_WINDOW_WIDTH: i32 = 400;
const TEST_WINDOW_HEIGHT: i32 = 1000;

/// A mock implementation of the `ProcessApi` trait for testing purposes.
#[derive(Debug)]
struct MockProcess {
    psn: ProcessSerialNumber,
}

impl ProcessApi for MockProcess {
    /// Always returns `true`, indicating the mock process is observable.
    #[instrument(level = Level::DEBUG, ret)]
    fn is_observable(&mut self) -> bool {
        debug!("{}:", function_name!());
        true
    }

    /// Returns a static name for the mock process.
    #[instrument(level = Level::DEBUG, ret)]
    fn name(&self) -> &'static str {
        "test"
    }

    /// Returns a predefined PID for the mock process.
    #[instrument(level = Level::DEBUG, ret)]
    fn pid(&self) -> Pid {
        debug!("{}:", function_name!());
        TEST_PROCESS_ID
    }

    /// Returns the `ProcessSerialNumber` of the mock process.
    #[instrument(level = Level::TRACE, ret)]
    fn psn(&self) -> ProcessSerialNumber {
        debug!("{}: {:?}", function_name!(), self.psn);
        self.psn
    }

    /// Always returns `None` for the `NSRunningApplication`.
    #[instrument(level = Level::DEBUG, ret)]
    fn application(&self) -> Option<objc2::rc::Retained<objc2_app_kit::NSRunningApplication>> {
        debug!("{}:", function_name!());
        None
    }

    /// Always returns `true`, indicating the mock process is ready.
    #[instrument(level = Level::DEBUG, ret)]
    fn ready(&mut self) -> bool {
        debug!("{}:", function_name!());
        true
    }
}

/// A mock implementation of the `ApplicationApi` trait for testing purposes.
/// It internally holds an `InnerMockApplication` within an `Arc<RwLock>`.
#[derive(Clone, Debug)]
struct MockApplication {
    inner: Arc<RwLock<InnerMockApplication>>,
}

/// The inner state of `MockApplication`, containing process serial number, PID, and focused window ID.
#[derive(Debug)]
struct InnerMockApplication {
    psn: ProcessSerialNumber,
    pid: Pid,
    focused_id: Option<WinID>,
}

impl MockApplication {
    /// Creates a new `MockApplication` instance.
    ///
    /// # Arguments
    ///
    /// * `psn` - The `ProcessSerialNumber` for this mock application.
    /// * `pid` - The `Pid` for this mock application.
    #[instrument(level = Level::DEBUG, ret)]
    fn new(psn: ProcessSerialNumber, pid: Pid) -> Self {
        MockApplication {
            inner: Arc::new(RwLock::new(InnerMockApplication {
                psn,
                pid,
                focused_id: None,
            })),
        }
    }
}

impl ApplicationApi for MockApplication {
    /// Returns the PID of the mock application.
    #[instrument(level = Level::TRACE, skip(self), ret)]
    fn pid(&self) -> Pid {
        self.inner.force_read().pid
    }

    /// Returns the `ProcessSerialNumber` of the mock application.
    #[instrument(level = Level::TRACE, skip(self), ret)]
    fn psn(&self) -> ProcessSerialNumber {
        debug!("{}:", function_name!());
        self.inner.force_read().psn
    }

    /// Always returns `Some(0)` for the connection ID.
    #[instrument(level = Level::DEBUG, skip(self), ret)]
    fn connection(&self) -> Option<ConnID> {
        debug!("{}:", function_name!());
        Some(0)
    }

    /// Returns the currently focused window ID for the mock application.
    ///
    /// # Returns
    ///
    /// `Ok(WinID)` if a window is focused, otherwise `Err(Error::InvalidWindow)`.
    #[instrument(level = Level::DEBUG, skip(self), ret)]
    fn focused_window_id(&self) -> Result<WinID> {
        let id = self
            .inner
            .force_read()
            .focused_id
            .ok_or(Error::InvalidWindow);
        debug!("{}: {id:?}", function_name!());
        id
    }

    /// Always returns an empty vector of window lists for the mock application.
    fn window_list(&self) -> Vec<Window> {
        debug!("{}:", function_name!());
        vec![]
    }

    /// Always returns `Ok(true)` for observe operations on the mock application.
    #[instrument(level = Level::DEBUG, skip(self), ret)]
    fn observe(&mut self) -> Result<bool> {
        debug!("{}:", function_name!());
        Ok(true)
    }

    /// Always returns `Ok(true)` for observe window operations on the mock application.
    #[instrument(level = Level::DEBUG, skip_all, ret)]
    fn observe_window(&mut self, _window: &Window) -> Result<bool> {
        debug!("{}:", function_name!());
        Ok(true)
    }

    /// Does nothing for unobserve window operations on the mock application.
    #[instrument(level = Level::DEBUG, skip_all, ret)]
    fn unobserve_window(&mut self, _window: &Window) {
        debug!("{}:", function_name!());
    }

    /// Always returns `true`, indicating the mock application is frontmost.
    #[instrument(level = Level::DEBUG, skip(self), ret)]
    fn is_frontmost(&self) -> bool {
        debug!("{}:", function_name!());
        true
    }

    /// Always returns `Some("test")` for the bundle ID.
    #[instrument(level = Level::DEBUG, skip(self), ret)]
    fn bundle_id(&self) -> Option<&str> {
        debug!("{}:", function_name!());
        Some("test")
    }
}

/// A mock implementation of the `WindowManagerApi` trait for testing purposes.
struct MockWindowManager {
    windows: Box<dyn Fn(WorkspaceId) -> Vec<Window> + Send + Sync + 'static>,
}

impl std::fmt::Debug for MockWindowManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MockWindowManager")
            .field("windows", &"<closure>") // Placeholder text
            .finish()
    }
}

impl WindowManagerApi for MockWindowManager {
    /// Creates a new mock application.
    fn new_application(&self, process: &dyn ProcessApi) -> Result<Application> {
        debug!("{}: from process {}", function_name!(), process.name());
        Ok(Application::new(Box::new(MockApplication {
            inner: Arc::new(RwLock::new(InnerMockApplication {
                psn: process.psn(),
                pid: process.pid(),
                focused_id: None,
            })),
        })))
    }

    /// Always returns an empty vector, as associated windows are not tested at this level.
    #[instrument(level = Level::DEBUG, skip(self), ret)]
    fn get_associated_windows(&self, window_id: WinID) -> Vec<WinID> {
        debug!("{}:", function_name!());
        vec![]
    }

    /// Always returns an empty vector, as present displays are mocked elsewhere.
    #[instrument(level = Level::DEBUG, skip(self), ret)]
    fn present_displays(&self) -> Vec<(Display, Vec<WorkspaceId>)> {
        let display = Display::new(
            TEST_DISPLAY_ID,
            IRect::new(0, 0, TEST_DISPLAY_WIDTH, TEST_DISPLAY_HEIGHT),
            TEST_MENUBAR_HEIGHT,
        );
        vec![(display, vec![TEST_WORKSPACE_ID])]
    }

    /// Returns a predefined active display ID.
    #[instrument(level = Level::DEBUG, skip(self), ret)]
    fn active_display_id(&self) -> Result<u32> {
        Ok(TEST_DISPLAY_ID)
    }

    /// Returns a predefined active display space ID.
    #[instrument(level = Level::DEBUG, skip(self), ret)]
    fn active_display_space(&self, display_id: CGDirectDisplayID) -> Result<WorkspaceId> {
        Ok(TEST_WORKSPACE_ID)
    }

    fn is_fullscreen_space(&self, _display_id: CGDirectDisplayID) -> bool {
        false
    }

    /// Does nothing, as mouse centering is not tested at this level.
    #[instrument(level = Level::DEBUG, skip_all, ret)]
    fn center_mouse(&self, _window: Option<&Window>, _display_bounds: &IRect) {
        debug!("{}:", function_name!());
    }

    /// Always returns an empty vector of windows.
    #[instrument(level = Level::DEBUG, skip_all)]
    fn find_existing_application_windows(
        &self,
        app: &mut Application,
        spaces: &[WorkspaceId],
    ) -> Result<(Vec<Window>, Vec<WinID>)> {
        debug!(
            "{}: app {} spaces {:?}",
            function_name!(),
            app.pid(),
            spaces
        );

        let windows = spaces
            .iter()
            .flat_map(|workspace_id| (self.windows)(*workspace_id))
            .collect::<Vec<_>>();
        Ok((windows, vec![]))
    }

    /// Always returns `Ok(0)`.
    #[instrument(level = Level::DEBUG, skip(self), ret)]
    fn find_window_at_point(&self, point: &CGPoint) -> Result<WinID> {
        debug!("{}:", function_name!());
        Ok(0)
    }

    /// Always returns an empty vector of window IDs.
    #[instrument(level = Level::DEBUG, skip(self), ret)]
    fn windows_in_workspace(&self, workspace_id: WorkspaceId) -> Result<Vec<WinID>> {
        debug!("{}:", function_name!());
        let ids = (self.windows)(workspace_id)
            .iter()
            .map(|window| window.id())
            .collect();
        Ok(ids)
    }

    /// Always returns `Ok(())`.
    #[instrument(level = Level::DEBUG, skip(self), ret)]
    fn quit(&self) -> Result<()> {
        debug!("{}:", function_name!());
        Ok(())
    }

    #[instrument(level = Level::DEBUG, skip(self))]
    fn setup_config_watcher(&self, path: &std::path::Path) -> Result<Box<dyn notify::Watcher>> {
        todo!()
    }

    fn cursor_position(&self) -> Option<CGPoint> {
        None
    }
}

/// A mock implementation of the `WindowApi` trait for testing purposes.
#[derive(Debug)]
struct MockWindow {
    id: WinID,
    frame: IRect,
    app: MockApplication,
    event_queue: EventQueue,
    pub minimized: bool,
}

impl WindowApi for MockWindow {
    /// Returns the ID of the mock window.
    #[instrument(level = Level::TRACE, skip(self), ret)]
    fn id(&self) -> WinID {
        self.id
    }

    /// Returns the frame (`CGRect`) of the mock window.
    #[instrument(level = Level::TRACE, skip(self), ret)]
    fn frame(&self) -> IRect {
        self.frame
    }

    /// Returns a dummy `CFRetained<AXUIWrapper>` for the mock window's accessibility element.
    #[instrument(level = Level::DEBUG, skip(self), ret)]
    fn element(&self) -> Option<CFRetained<AXUIWrapper>> {
        debug!("{}:", function_name!());
        None
    }

    /// Always returns an empty string for the window title.
    #[instrument(level = Level::TRACE, skip(self), ret)]
    fn title(&self) -> Result<String> {
        Ok(String::new())
    }

    /// Always returns `Ok(true)` for valid role.
    #[instrument(level = Level::DEBUG, skip(self), ret)]
    fn child_role(&self) -> Result<bool> {
        debug!("{}:", function_name!());
        Ok(true)
    }

    /// Always returns an empty string for the window role.
    #[instrument(level = Level::TRACE, skip(self), ret)]
    fn role(&self) -> Result<String> {
        Ok(String::new())
    }

    /// Always returns an empty string for the window subrole.
    #[instrument(level = Level::TRACE, skip(self), ret)]
    fn subrole(&self) -> Result<String> {
        Ok(String::new())
    }

    /// Always returns `true` for root status.
    #[instrument(level = Level::DEBUG, skip(self), ret)]
    fn is_root(&self) -> bool {
        debug!("{}:", function_name!());
        true
    }

    /// Repositions the mock window's frame to the given coordinates.
    #[instrument(level = Level::DEBUG, skip(self))]
    fn reposition(&mut self, origin: Origin) {
        debug!("{}: id {} to {origin}", function_name!(), self.id);
        let size = self.frame.size();
        self.frame.min = origin;
        self.frame.max = origin + size;
    }

    /// Resizes the mock window's frame to the given dimensions.
    #[instrument(level = Level::DEBUG, skip(self))]
    fn resize(&mut self, size: Size, display_width: i32) {
        debug!("{}: id {} to {size}", function_name!(), self.id);
        self.frame.max = self.frame.min + size;
    }

    /// Always returns `Ok(())` for updating the frame.
    #[instrument(level = Level::DEBUG, skip(self), ret)]
    fn update_frame(&mut self, bounds: &IRect) -> Result<()> {
        debug!("{}:", function_name!());
        Ok(())
    }

    /// Prints a debug message for focus without raise.
    #[instrument(level = Level::DEBUG, skip_all)]
    fn focus_without_raise(
        &self,
        _psn: ProcessSerialNumber,
        currently_focused: &Window,
        _ocused_psn: ProcessSerialNumber,
    ) {
        debug!(
            "{}: id {} {}",
            function_name!(),
            self.id,
            currently_focused.id()
        );
    }

    /// Prints a debug message for focus with raise and updates the mock application's focused ID.
    #[instrument(level = Level::DEBUG, skip(self), ret)]
    fn focus_with_raise(&self, psn: ProcessSerialNumber) {
        debug!("{}: id {}", function_name!(), self.id);
        self.event_queue
            .write()
            .unwrap()
            .push(Event::ApplicationFrontSwitched { psn });
        self.app.inner.force_write().focused_id = Some(self.id);
    }

    /// Does nothing for width ratio.
    #[instrument(level = Level::DEBUG, skip(self), ret)]
    fn width_ratio(&self) -> f64 {
        debug!("{}:", function_name!());
        0.5
    }

    #[instrument(level = Level::TRACE, skip(self), ret)]
    fn pid(&self) -> Result<Pid> {
        Ok(TEST_PROCESS_ID)
    }

    #[instrument(level = Level::DEBUG, skip(self), ret)]
    fn set_padding(&mut self, padding: manager::WindowPadding) {
        debug!("{}:", function_name!());
    }

    fn horizontal_padding(&self) -> i32 {
        0
    }

    fn vertical_padding(&self) -> i32 {
        0
    }

    #[instrument(level = Level::TRACE, skip(self), ret)]
    fn is_minimized(&self) -> bool {
        self.minimized
    }

    fn is_full_screen(&self) -> bool {
        false
    }
}

impl MockWindow {
    /// Creates a new `MockWindow` instance.
    ///
    /// # Arguments
    ///
    /// * `id` - The `WinID` of the window.
    /// * `psn` - An `Option<ProcessSerialNumber>` for the owning process.
    /// * `frame` - The `CGRect` representing the window's initial frame.
    /// * `event_queue` - An optional reference to an `EventQueue` for simulating events.
    /// * `app` - A `MockApplication` instance associated with this window.
    fn new(id: WinID, frame: IRect, event_queue: EventQueue, app: MockApplication) -> Self {
        MockWindow {
            id,
            frame,
            app,
            event_queue,
            minimized: false,
        }
    }
}

fn setup_world() -> App {
    static DONE: OnceLock<()> = OnceLock::new();
    DONE.get_or_init(|| {
        tracing_subscriber::registry()
            .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
            .with(
                fmt::layer()
                    .with_level(true)
                    .with_line_number(true)
                    .with_file(true)
                    .with_target(true)
                    .with_thread_ids(false)
                    .with_writer(std::io::stderr)
                    .compact(),
            )
            .init();

        let _pool = AsyncComputeTaskPool::get_or_init(|| {
            TaskPoolBuilder::new()
                .num_threads(1) // Keep it light for tests
                .build()
        });
        assert!(AsyncComputeTaskPool::try_get().is_some());
    });
    let mut bevy_app = App::new();
    bevy_app
        .add_plugins(MinimalPlugins)
        .add_plugins(bevy::state::app::StatesPlugin)
        .init_resource::<Messages<Event>>()
        .insert_resource(PollForNotifications)
        .init_resource::<FocusContext>()
        .insert_resource(Config::default())
        .init_state::<AppPhase>()
        .init_state::<InteractionMode>()
        .add_plugins(WindowPlugin);

    bevy_app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
        100,
    )));

    bevy_app
}

fn setup_process(world: &mut World) -> MockApplication {
    let psn = ProcessSerialNumber { high: 1, low: 2 };
    let mock_process = MockProcess { psn };
    let process = world.spawn(BProcess(Box::new(mock_process))).id();

    let application = MockApplication::new(psn, TEST_PROCESS_ID);
    world.spawn((
        ExistingMarker,
        ChildOf(process),
        Application::new(Box::new(application.clone())),
    ));
    application
}

/// Type alias for a shared, thread-safe queue of `Event`s, used for simulating internal events in tests.
type EventQueue = Arc<RwLock<Vec<Event>>>;
// type WindowCreator = impl Fn(WorkspaceId) -> Vec<Window> + Send + Sync + 'static;

/// Runs the main test loop, simulating command dispatch and Bevy app updates.
/// For each command, the Bevy app is updated multiple times, and internal mock events are flushed.
/// A `verifier` closure is called after each command to assert the state of the world.
///
/// # Arguments
///
/// * `commands` - A slice of `Event`s representing commands to dispatch.
/// * `verifier` - A closure that takes the current iteration and a mutable reference to the `World` for assertions.
fn run_main_loop(
    bevy_app: &mut App,
    event_queue: &EventQueue,
    commands: &[Event],
    mut verifier: impl FnMut(usize, &mut World),
) {
    for (iteration, command) in commands.iter().enumerate() {
        bevy_app.world_mut().write_message::<Event>(command.clone());

        for _ in 0..5 {
            bevy_app.update();

            // Flush the event queue with internally generated mock events.
            while let Some(event) = event_queue.write().unwrap().pop() {
                bevy_app.world_mut().write_message::<Event>(event);
            }
        }

        verifier(iteration, bevy_app.world_mut());
    }
}

/// Verifies the positions of windows against a set of expected positions.
/// This function queries `Window` components from the world and asserts their `origin.x` and `origin.y` values.
///
/// # Arguments
///
/// * `expected_positions` - A slice of `(WinID, (i32, i32))` tuples, where `WinID` is the window ID and `(i32, i32)` are the expected (x, y) coordinates.
/// * `world` - A mutable reference to the Bevy `World` for querying window components.
fn verify_window_positions(expected_positions: &[(WinID, (i32, i32))], world: &mut World) {
    let mut query = world.query::<&Window>();

    for window in query.iter(world) {
        if let Some((window_id, (x, y))) = expected_positions.iter().find(|id| id.0 == window.id())
        {
            debug!("WinID: {window_id}");
            assert_eq!(*x, window.frame().min.x);
            assert_eq!(*y, window.frame().min.y);
        }
    }
}

fn verify_window_sizes(expected_sizes: &[(WinID, (i32, i32))], world: &mut World) {
    let mut query = world.query::<&Window>();

    for window in query.iter(world) {
        if let Some((window_id, (w, h))) = expected_sizes.iter().find(|id| id.0 == window.id()) {
            let frame = window.frame();
            assert_eq!(
                *w,
                frame.width(),
                "WinID {window_id}: expected width {w}, got {}",
                frame.width()
            );
            assert_eq!(
                *h,
                frame.height(),
                "WinID {window_id}: expected height {h}, got {}",
                frame.height()
            );
        }
    }
}

#[test]
#[allow(clippy::too_many_lines)]
fn test_window_shuffle() {
    let _lock = TEST_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
    let commands = vec![
        Event::MenuOpened { window_id: 0 }, // Noop allowing everything to settle
        Event::Command {
            command: Command::Window(Operation::Focus(Direction::Last)),
        },
        Event::Command {
            command: Command::Window(Operation::Focus(Direction::First)),
        },
        Event::Command {
            command: Command::Window(Operation::Focus(Direction::East)),
        },
        Event::Command {
            command: Command::Window(Operation::Stack(true)),
        },
        Event::Command {
            command: Command::Window(Operation::Center),
        },
        Event::Command {
            command: Command::Window(Operation::Focus(Direction::East)),
        },
        Event::Command {
            command: Command::Window(Operation::Stack(true)),
        },
        Event::Command {
            command: Command::Window(Operation::Center),
        },
    ];

    let offscreen_left = 0 - TEST_WINDOW_WIDTH + 5;
    let offscreen_right = TEST_DISPLAY_WIDTH - 5;

    let expected_positions_last = [
        (4, (offscreen_left, TEST_MENUBAR_HEIGHT)),
        (3, (offscreen_left, TEST_MENUBAR_HEIGHT)),
        (2, (-176, TEST_MENUBAR_HEIGHT)),
        (1, (224, TEST_MENUBAR_HEIGHT)),
        (0, (624, TEST_MENUBAR_HEIGHT)),
    ];
    let expected_positions_first = [
        (4, (0, TEST_MENUBAR_HEIGHT)),
        (3, (400, TEST_MENUBAR_HEIGHT)),
        (2, (800, TEST_MENUBAR_HEIGHT)),
        (1, (offscreen_right, TEST_MENUBAR_HEIGHT)),
        (0, (offscreen_right, TEST_MENUBAR_HEIGHT)),
    ];

    let centered = (TEST_DISPLAY_WIDTH - TEST_WINDOW_WIDTH) / 2;
    let expected_positions_stacked = [
        (4, (centered, TEST_MENUBAR_HEIGHT)),
        (3, (centered, 374 + TEST_MENUBAR_HEIGHT)),
        (2, (centered + TEST_WINDOW_WIDTH, TEST_MENUBAR_HEIGHT)),
        (1, (offscreen_right, TEST_MENUBAR_HEIGHT)),
        (0, (offscreen_right, TEST_MENUBAR_HEIGHT)),
    ];
    let expected_positions_stacked2 = [
        (4, (centered, TEST_MENUBAR_HEIGHT)),
        (3, (centered, 249 + TEST_MENUBAR_HEIGHT)),
        (2, (centered, 498 + TEST_MENUBAR_HEIGHT)),
        (1, (712, TEST_MENUBAR_HEIGHT)),
        (0, (offscreen_right, TEST_MENUBAR_HEIGHT)),
    ];

    let check = |iteration, world: &mut World| {
        let iterations = [
            None,
            Some(expected_positions_last.as_slice()),
            Some(expected_positions_first.as_slice()),
            None,
            None,
            Some(expected_positions_stacked.as_slice()),
            None,
            None,
            Some(expected_positions_stacked2.as_slice()),
        ];

        if let Some(positions) = iterations[iteration] {
            debug!("Iteration: {iteration}");
            verify_window_positions(positions, world);
        }
    };

    let mut bevy = setup_world();
    let mock_app = setup_process(bevy.world_mut());
    let internal_queue = Arc::new(RwLock::new(Vec::<Event>::new()));
    let event_queue = internal_queue.clone();

    let windows = Box::new(move |_| {
        (0..5)
            .map(|i| {
                let origin = Origin::new(100 * i, 0);
                let size = Size::new(TEST_WINDOW_WIDTH, TEST_WINDOW_HEIGHT);
                let window = MockWindow::new(
                    i,
                    IRect {
                        min: origin,
                        max: origin + size,
                    },
                    event_queue.clone(),
                    mock_app.clone(),
                );
                Window::new(Box::new(window))
            })
            .collect::<Vec<_>>()
    });
    let window_manager = MockWindowManager { windows };
    bevy.world_mut()
        .insert_resource(WindowManager(Box::new(window_manager)));

    run_main_loop(&mut bevy, &internal_queue, &commands, check);
}

#[test]
fn test_startup_windows() {
    let _lock = TEST_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
    let commands = vec![
        Event::MenuOpened { window_id: 0 }, // Noop allowing everything to settle
        Event::Command {
            command: Command::Window(Operation::Focus(Direction::East)),
        },
        Event::Command {
            command: Command::Window(Operation::Focus(Direction::East)),
        },
        Event::Command {
            command: Command::Window(Operation::Focus(Direction::First)),
        },
        Event::Command {
            command: Command::PrintState,
        },
    ];

    let expected_positions = [
        (4, (0, TEST_MENUBAR_HEIGHT)),
        (3, (400, TEST_MENUBAR_HEIGHT)),
        (2, (800, TEST_MENUBAR_HEIGHT)),
    ];

    let check = |iteration, world: &mut World| {
        let iterations = [None, None, None, None, Some(expected_positions.as_slice())];

        if let Some(positions) = iterations[iteration] {
            debug!("Iteration: {iteration}");
            verify_window_positions(positions, world);
        }
    };

    let mut bevy = setup_world();
    let mock_app = setup_process(bevy.world_mut());
    let internal_queue = Arc::new(RwLock::new(Vec::<Event>::new()));
    let event_queue = internal_queue.clone();

    let windows = Box::new(move |_| {
        (0..5)
            .map(|i| {
                let origin = Origin::new(100 * i, 0);
                let size = Size::new(TEST_WINDOW_WIDTH, TEST_WINDOW_HEIGHT);
                let mut window = MockWindow::new(
                    i,
                    IRect {
                        min: origin,
                        max: origin + size,
                    },
                    event_queue.clone(),
                    mock_app.clone(),
                );
                if i < 2 {
                    window.minimized = true;
                }
                Window::new(Box::new(window))
            })
            .collect::<Vec<_>>()
    });
    let window_manager = MockWindowManager { windows };
    bevy.world_mut()
        .insert_resource(WindowManager(Box::new(window_manager)));

    run_main_loop(&mut bevy, &internal_queue, &commands, check);
}

#[test]
fn test_dont_focus() {
    let _lock = TEST_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
    let commands = vec![
        Event::MenuOpened { window_id: 0 }, // Noop allowing everything to settle
        Event::Command {
            command: Command::Window(Operation::Focus(Direction::Last)),
        },
        Event::Command {
            command: Command::Window(Operation::Focus(Direction::First)),
        },
        Event::Command {
            command: Command::PrintState,
        },
    ];

    let offscreen_right = TEST_DISPLAY_WIDTH - 5;
    let expected_positions = [
        (2, (0, TEST_MENUBAR_HEIGHT)),
        (1, (400, TEST_MENUBAR_HEIGHT)),
        (0, (800, TEST_MENUBAR_HEIGHT)),
        (3, (offscreen_right, TEST_MENUBAR_HEIGHT)),
    ];

    let mut bevy = setup_world();
    let app = setup_process(bevy.world_mut());
    let mock_app = app.clone();
    let internal_queue = Arc::new(RwLock::new(Vec::<Event>::new()));
    let event_queue = internal_queue.clone();

    let windows = Box::new(move |_| {
        (0..3)
            .map(|i| {
                let origin = Origin::new(100 * i, 0);
                let size = Size::new(TEST_WINDOW_WIDTH, TEST_WINDOW_HEIGHT);
                let window = MockWindow::new(
                    i,
                    IRect {
                        min: origin,
                        max: origin + size,
                    },
                    event_queue.clone(),
                    mock_app.clone(),
                );
                Window::new(Box::new(window))
            })
            .collect::<Vec<_>>()
    });
    let window_manager = MockWindowManager { windows };
    bevy.world_mut()
        .insert_resource(WindowManager(Box::new(window_manager)));

    let check_queue = internal_queue.clone();
    let check = |iteration, world: &mut World| {
        let iterations = [None, None, None, Some(expected_positions.as_slice())];

        if let Some(positions) = iterations[iteration] {
            debug!("Iteration: {iteration}");
            verify_window_positions(positions, world);

            let mut query = world.query::<(&Window, Has<FocusedMarker>)>();
            for (window, focused) in query.iter(world) {
                if focused {
                    // Check that focus stayed on the first window.
                    assert_eq!(window.id(), 2);
                }
            }
        }

        if iteration == 1 {
            let origin = Origin::new(0, 0);
            let size = Size::new(TEST_WINDOW_WIDTH, TEST_WINDOW_HEIGHT);
            let window = MockWindow::new(
                3,
                IRect {
                    min: origin,
                    max: origin + size,
                },
                check_queue.clone(),
                app.clone(),
            );
            let window = Window::new(Box::new(window));
            world.trigger(SpawnWindowTrigger(vec![window]));
        }
    };

    let config: Config = r#"
[options]
[bindings]
[windows]
[windows.skipfocus]
title = ".*"
dont_focus = true
index = 100
"#
    .try_into()
    .unwrap();
    bevy.insert_resource(config);
    run_main_loop(&mut bevy, &internal_queue, &commands, check);
}

/// Off-screen windows should keep the same height as on-screen windows
/// when `sliver_height` is 1.0 (the default). A previous bug subtracted
/// `menubar_height` from off-screen window heights, causing a visible
/// resize when they came into focus.
#[test]
fn test_offscreen_windows_preserve_height() {
    let _lock = TEST_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
    let expected_height = TEST_DISPLAY_HEIGHT - TEST_MENUBAR_HEIGHT;

    let commands = vec![
        Event::MenuOpened { window_id: 0 }, // Settle
        Event::Command {
            command: Command::Window(Operation::Focus(Direction::First)),
        },
    ];

    let expected_sizes = [
        (4, (TEST_WINDOW_WIDTH, expected_height)),
        (3, (TEST_WINDOW_WIDTH, expected_height)),
        (2, (TEST_WINDOW_WIDTH, expected_height)),
        (1, (TEST_WINDOW_WIDTH, expected_height)),
        (0, (TEST_WINDOW_WIDTH, expected_height)),
    ];

    let check = |iteration, world: &mut World| {
        if iteration == 1 {
            verify_window_sizes(&expected_sizes, world);
        }
    };

    let mut bevy = setup_world();
    let mock_app = setup_process(bevy.world_mut());
    let internal_queue = Arc::new(RwLock::new(Vec::<Event>::new()));
    let event_queue = internal_queue.clone();

    let windows = Box::new(move |_| {
        (0..5)
            .map(|i| {
                let origin = Origin::new(100 * i, 0);
                let size = Size::new(TEST_WINDOW_WIDTH, TEST_WINDOW_HEIGHT);
                let window = MockWindow::new(
                    i,
                    IRect {
                        min: origin,
                        max: origin + size,
                    },
                    event_queue.clone(),
                    mock_app.clone(),
                );
                Window::new(Box::new(window))
            })
            .collect::<Vec<_>>()
    });
    let window_manager = MockWindowManager { windows };
    bevy.world_mut()
        .insert_resource(WindowManager(Box::new(window_manager)));

    run_main_loop(&mut bevy, &internal_queue, &commands, check);
}

/// When `focus_follows_mouse` and `mouse_follows_focus` are both disabled,
/// mouse events (move, click, drag) must not cause any window reshuffle
/// or position change. Keyboard navigation must still work normally.
#[test]
fn test_mouse_disconnected_no_reshuffle() {
    let _lock = TEST_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
    // First settle the layout, then fire mouse events and verify no change.
    let commands = vec![
        Event::MenuOpened { window_id: 0 }, // Settle initial layout
        Event::Command {
            command: Command::Window(Operation::Focus(Direction::First)),
        },
        // Mouse events that should be no-ops when disconnected:
        Event::MouseMoved {
            point: CGPoint::new(50.0, 50.0),
        },
        Event::MouseDown {
            point: CGPoint::new(50.0, 50.0),
        },
        Event::MouseDragged {
            point: CGPoint::new(100.0, 100.0),
        },
        // Fire another focus command to verify keyboard still works.
        Event::Command {
            command: Command::Window(Operation::Focus(Direction::East)),
        },
    ];

    let expected_after_settle = [
        (4, (0, TEST_MENUBAR_HEIGHT)),
        (3, (400, TEST_MENUBAR_HEIGHT)),
        (2, (800, TEST_MENUBAR_HEIGHT)),
    ];

    let expected_after_east = [
        (4, (-176, TEST_MENUBAR_HEIGHT)),
        (3, (224, TEST_MENUBAR_HEIGHT)),
        (2, (624, TEST_MENUBAR_HEIGHT)),
    ];

    let settled_positions: std::cell::RefCell<Vec<(WinID, (i32, i32))>> =
        std::cell::RefCell::new(Vec::new());

    let check = |iteration, world: &mut World| {
        match iteration {
            1 => {
                // After Focus(First) — capture settled positions.
                verify_window_positions(&expected_after_settle, world);
                let mut query = world.query::<&Window>();
                let mut positions = Vec::new();
                for window in query.iter(world) {
                    positions.push((window.id(), (window.frame().min.x, window.frame().min.y)));
                }
                *settled_positions.borrow_mut() = positions;
            }
            2 | 3 | 4 => {
                // After MouseMoved, MouseDown, MouseDragged — positions must not change.
                let settled = settled_positions.borrow();
                verify_window_positions(&settled, world);
            }
            5 => {
                // After Focus(East) — keyboard navigation still works.
                verify_window_positions(&expected_after_east, world);
            }
            _ => {}
        }
    };

    let mut bevy = setup_world();
    let mock_app = setup_process(bevy.world_mut());
    let internal_queue = Arc::new(RwLock::new(Vec::<Event>::new()));
    let event_queue = internal_queue.clone();

    let windows = Box::new(move |_| {
        (0..5)
            .map(|i| {
                let origin = Origin::new(100 * i, 0);
                let size = Size::new(TEST_WINDOW_WIDTH, TEST_WINDOW_HEIGHT);
                let window = MockWindow::new(
                    i,
                    IRect {
                        min: origin,
                        max: origin + size,
                    },
                    event_queue.clone(),
                    mock_app.clone(),
                );
                Window::new(Box::new(window))
            })
            .collect::<Vec<_>>()
    });
    let window_manager = MockWindowManager { windows };
    bevy.world_mut()
        .insert_resource(WindowManager(Box::new(window_manager)));

    // Mouse fully disconnected: both FFM and MFF disabled.
    let config: Config = r#"
[options]
focus_follows_mouse = false
mouse_follows_focus = false
auto_center = false
[bindings]
"#
    .try_into()
    .unwrap();
    bevy.insert_resource(config);

    run_main_loop(&mut bevy, &internal_queue, &commands, check);
}

/// Verify that `MouseDown` on a partially off-screen window does NOT
/// trigger a reshuffle when mouse is disconnected from tiling.
#[test]
fn test_mouse_disconnected_click_offscreen_no_reshuffle() {
    let _lock = TEST_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
    let offscreen_left = 0 - TEST_WINDOW_WIDTH + 5;

    let commands = vec![
        Event::MenuOpened { window_id: 0 }, // Settle
        Event::Command {
            command: Command::Window(Operation::Focus(Direction::Last)),
        },
        // Click on the off-screen region — should NOT reshuffle.
        Event::MouseDown {
            point: CGPoint::new(2.0, f64::from(TEST_MENUBAR_HEIGHT + 10)),
        },
    ];

    let expected_positions_last = [
        (4, (offscreen_left, TEST_MENUBAR_HEIGHT)),
        (3, (offscreen_left, TEST_MENUBAR_HEIGHT)),
        (2, (-176, TEST_MENUBAR_HEIGHT)),
        (1, (224, TEST_MENUBAR_HEIGHT)),
        (0, (624, TEST_MENUBAR_HEIGHT)),
    ];

    let check = |iteration, world: &mut World| {
        if iteration >= 1 {
            // Positions should remain at Focus(Last) layout — click must not reshuffle.
            verify_window_positions(&expected_positions_last, world);
        }
    };

    let mut bevy = setup_world();
    let mock_app = setup_process(bevy.world_mut());
    let internal_queue = Arc::new(RwLock::new(Vec::<Event>::new()));
    let event_queue = internal_queue.clone();

    let windows = Box::new(move |_| {
        (0..5)
            .map(|i| {
                let origin = Origin::new(100 * i, 0);
                let size = Size::new(TEST_WINDOW_WIDTH, TEST_WINDOW_HEIGHT);
                let window = MockWindow::new(
                    i,
                    IRect {
                        min: origin,
                        max: origin + size,
                    },
                    event_queue.clone(),
                    mock_app.clone(),
                );
                Window::new(Box::new(window))
            })
            .collect::<Vec<_>>()
    });
    let window_manager = MockWindowManager { windows };
    bevy.world_mut()
        .insert_resource(WindowManager(Box::new(window_manager)));

    let config: Config = r#"
[options]
focus_follows_mouse = false
mouse_follows_focus = false
auto_center = false
[bindings]
"#
    .try_into()
    .unwrap();
    bevy.insert_resource(config);

    run_main_loop(&mut bevy, &internal_queue, &commands, check);
}

// -----------------------------------------------------------------------
// Regression tests for atomicity & parallelism refactor
// -----------------------------------------------------------------------

/// Regression: FocusContext must be mutable only through `ConfigurationMut`,
/// not through `Configuration` (read-only). This tests the state machine
/// transitions deterministically — no pipeline dependencies, no race conditions.
///
/// State machine under test:
///   FocusContext { source: Keyboard, ffm_window: None }
///     → set_skip_reshuffle(true) → { source: Mouse, ffm_window: None }
///     → set_ffm_flag(Some(42))   → { source: Mouse, ffm_window: Some(42) }
///     → set_skip_reshuffle(false) → { source: Keyboard, ffm_window: Some(42) }
///     → set_ffm_flag(None)        → { source: Keyboard, ffm_window: None }  (back to initial)
#[test]
fn test_configuration_read_write_split() {
    use bevy::ecs::system::RunSystemOnce;
    use crate::ecs::params::{Configuration, ConfigurationMut};
    use crate::ecs::state::{AppPhase, FocusContext, FocusSource, InteractionMode};

    let _lock = TEST_MUTEX.lock().unwrap_or_else(|e| e.into_inner());

    // Minimal app: only the resources needed for Configuration/ConfigurationMut.
    let mut app = App::new();
    app.add_plugins(bevy::state::app::StatesPlugin);
    app.init_resource::<FocusContext>();
    app.insert_resource(Config::default());
    app.init_state::<AppPhase>();
    app.init_state::<InteractionMode>();
    // One update to finalize state initialization.
    app.update();

    // Initial state: Keyboard, no FFM window.
    let ctx = app.world().resource::<FocusContext>();
    assert_eq!(ctx.source, FocusSource::Keyboard);
    assert_eq!(ctx.ffm_window, None);

    // Transition 1: ConfigurationMut sets skip_reshuffle(true) → source becomes Mouse.
    app.world_mut()
        .run_system_once(|mut config: ConfigurationMut| {
            assert_eq!(config.ffm_flag(), None);
            assert!(!config.skip_reshuffle());
            config.set_skip_reshuffle(true);
        })
        .unwrap();
    let ctx = app.world().resource::<FocusContext>();
    assert_eq!(ctx.source, FocusSource::Mouse, "set_skip_reshuffle(true) → Mouse");

    // Transition 2: ConfigurationMut sets ffm_flag.
    app.world_mut()
        .run_system_once(|mut config: ConfigurationMut| {
            config.set_ffm_flag(Some(42));
        })
        .unwrap();
    let ctx = app.world().resource::<FocusContext>();
    assert_eq!(ctx.ffm_window, Some(42), "set_ffm_flag(Some(42))");
    assert_eq!(ctx.source, FocusSource::Mouse, "source unchanged");

    // Transition 3: ConfigurationMut sets skip_reshuffle(false) → source becomes Keyboard.
    app.world_mut()
        .run_system_once(|mut config: ConfigurationMut| {
            config.set_skip_reshuffle(false);
        })
        .unwrap();
    let ctx = app.world().resource::<FocusContext>();
    assert_eq!(ctx.source, FocusSource::Keyboard, "set_skip_reshuffle(false) → Keyboard");
    assert_eq!(ctx.ffm_window, Some(42), "ffm_window unchanged");

    // Transition 4: Reset ffm_flag → back to initial state.
    app.world_mut()
        .run_system_once(|mut config: ConfigurationMut| {
            config.set_ffm_flag(None);
        })
        .unwrap();
    let ctx = app.world().resource::<FocusContext>();
    assert_eq!(ctx.source, FocusSource::Keyboard);
    assert_eq!(ctx.ffm_window, None, "back to initial state");

    // Read-only Configuration can observe but not mutate (compile-time guarantee).
    // Verify that read-only Configuration sees the same state.
    app.world_mut()
        .run_system_once(|config: Configuration| {
            assert!(!config.skip_reshuffle());
            assert_eq!(config.ffm_flag(), None);
            assert!(config.focus_follows_mouse());
            assert!(config.mouse_follows_focus());
            assert!(!config.auto_center());
            assert!(!config.mission_control_active());
            assert!(config.initializing());
        })
        .unwrap();

    // Verify state was not mutated by the read-only system.
    let ctx = app.world().resource::<FocusContext>();
    assert_eq!(ctx.source, FocusSource::Keyboard, "read-only Configuration did not mutate source");
    assert_eq!(ctx.ffm_window, None, "read-only Configuration did not mutate ffm_window");
}

/// Regression: dispatch_toplevel_triggers must route events written as
/// messages to WMEventTrigger observers. This test verifies the event
/// dispatch state machine deterministically using a marker to confirm
/// the trigger fired.
///
/// State machine under test:
///   Event written as Message
///     → dispatch_toplevel_triggers reads it
///     → fires WMEventTrigger(event)
///     → observer receives WMEventTrigger
///
/// In tests, pump_events is a no-op (no platform/receiver), so events
/// are injected directly via write_message. The chain() ordering between
/// pump_events and dispatch_toplevel_triggers ensures that in production,
/// pump_events writes before dispatch reads — this is a structural guarantee
/// that cannot be tested in the mock environment. Instead, we verify
/// that the dispatch→observer pipeline works correctly.
#[test]
fn test_event_dispatch_pipeline() {
    use bevy::ecs::observer::On;
    use crate::ecs::WMEventTrigger;

    let _lock = TEST_MUTEX.lock().unwrap_or_else(|e| e.into_inner());

    // Marker resource to verify trigger fired.
    #[derive(Resource, Default)]
    struct TriggerFired(Vec<String>);

    let mut bevy = setup_world();

    // Insert the trigger-fired marker and a mock WindowManager (required by systems).
    bevy.world_mut().insert_resource(TriggerFired::default());
    let event_queue: EventQueue = Arc::new(RwLock::new(Vec::new()));
    let eq = event_queue.clone();
    let mock_app = MockApplication::new(ProcessSerialNumber { high: 1, low: 2 }, TEST_PROCESS_ID);
    let windows: Box<dyn Fn(WorkspaceId) -> Vec<Window> + Send + Sync> =
        Box::new(move |_| {
            let origin = Origin::new(0, 0);
            let size = Size::new(TEST_WINDOW_WIDTH, TEST_WINDOW_HEIGHT);
            vec![Window::new(Box::new(MockWindow::new(
                0,
                IRect { min: origin, max: origin + size },
                eq.clone(),
                mock_app.clone(),
            )))]
        });
    bevy.world_mut()
        .insert_resource(WindowManager(Box::new(MockWindowManager { windows })));

    // Add an observer that records when WMEventTrigger fires for MenuOpened.
    bevy.world_mut().add_observer(
        |trigger: On<WMEventTrigger>, mut fired: ResMut<TriggerFired>| {
            if let Event::MenuOpened { window_id } = &trigger.event().0 {
                fired.0.push(format!("MenuOpened:{window_id}"));
            }
        },
    );

    // Before any event: marker should be empty.
    assert!(bevy.world().resource::<TriggerFired>().0.is_empty());

    // Write a MenuOpened event as a message.
    bevy.world_mut()
        .write_message::<Event>(Event::MenuOpened { window_id: 99 });

    // Run one update cycle: dispatch_toplevel_triggers should read the
    // message and fire WMEventTrigger, which our observer should catch.
    bevy.update();

    let fired = &bevy.world().resource::<TriggerFired>().0;
    assert_eq!(
        fired.len(),
        1,
        "Expected exactly one trigger fire, got {fired:?}"
    );
    assert_eq!(fired[0], "MenuOpened:99");

    // Write two more events in sequence.
    bevy.world_mut()
        .write_message::<Event>(Event::MenuOpened { window_id: 1 });
    bevy.world_mut()
        .write_message::<Event>(Event::MenuOpened { window_id: 2 });
    bevy.update();

    let fired = &bevy.world().resource::<TriggerFired>().0;
    assert_eq!(
        fired.len(),
        3,
        "Expected three total trigger fires, got {fired:?}"
    );
    assert_eq!(fired[1], "MenuOpened:1");
    assert_eq!(fired[2], "MenuOpened:2");
}

/// Regression: animate_resize_windows must run after animate_windows.
/// Verifies that combined reposition+resize produces valid window positions.
#[test]
fn test_animation_ordering_resize_after_reposition() {
    let _lock = TEST_MUTEX.lock().unwrap_or_else(|e| e.into_inner());

    let mut bevy = setup_world();
    let mock_app = setup_process(bevy.world_mut());
    let internal_queue = Arc::new(RwLock::new(Vec::<Event>::new()));
    let event_queue = internal_queue.clone();

    let windows = Box::new(move |_| {
        (0..3)
            .map(|i| {
                let origin = Origin::new(100 * i, 0);
                let size = Size::new(TEST_WINDOW_WIDTH, TEST_WINDOW_HEIGHT);
                Window::new(Box::new(MockWindow::new(
                    i,
                    IRect {
                        min: origin,
                        max: origin + size,
                    },
                    event_queue.clone(),
                    mock_app.clone(),
                )))
            })
            .collect()
    });
    bevy.world_mut()
        .insert_resource(WindowManager(Box::new(MockWindowManager { windows })));

    let commands = vec![
        Event::MenuOpened { window_id: 0 },
        Event::Command {
            command: Command::Window(Operation::Focus(Direction::First)),
        },
        Event::Command {
            command: Command::Window(Operation::Resize),
        },
    ];

    let check = |iteration, world: &mut World| {
        if iteration == 2 {
            let mut query = world.query::<&Window>();
            let windows: Vec<_> = query.iter(world).collect();
            for window in &windows {
                assert!(
                    window.frame().min.x >= -TEST_WINDOW_WIDTH,
                    "Window {} has invalid x={} after resize+reposition",
                    window.id(),
                    window.frame().min.x
                );
            }
        }
    };

    run_main_loop(&mut bevy, &internal_queue, &commands, check);
}

// -----------------------------------------------------------------------
// Reload guard state machine tests
// -----------------------------------------------------------------------

/// Verify the ReloadGuard state machine: new → bump resets counter → tick
/// decrements → settled() transitions from false to true exactly when
/// settle_frames reaches zero.
#[test]
fn test_reload_guard_state_machine() {
    use crate::ecs::state::ReloadGuard;
    use std::collections::HashMap;

    let mut guard = ReloadGuard::new(HashMap::new());
    assert!(!guard.settled(), "fresh guard must not be settled");
    assert_eq!(guard.settle_frames, 2);

    // First tick: 2 → 1, not yet settled.
    assert!(!guard.tick(), "tick from 2 should not signal settled");
    assert!(!guard.settled());

    // Bump resets to 2.
    guard.bump();
    assert_eq!(guard.settle_frames, 2, "bump must reset to SETTLE_FRAMES");
    assert!(!guard.settled());

    // Tick twice to settle.
    assert!(!guard.tick());
    assert!(guard.tick(), "second tick should signal settled");
    assert!(guard.settled());

    // Extra ticks after settling are no-ops.
    assert!(!guard.tick(), "tick after settled should return false");
    assert!(guard.settled());
}

/// Verify that multiple bump() calls keep resetting the settle counter,
/// simulating cascading OS events.
#[test]
fn test_reload_guard_cascading_bumps() {
    use crate::ecs::state::ReloadGuard;
    use std::collections::HashMap;

    let mut guard = ReloadGuard::new(HashMap::new());

    // Simulate 5 rapid cascading events.
    for _ in 0..5 {
        guard.tick();
        guard.bump();
    }
    // Guard should not have settled during the cascade.
    assert!(!guard.settled());

    // Now let it settle.
    guard.tick();
    guard.tick();
    assert!(guard.settled());
}

// -----------------------------------------------------------------------
// Window swap integration tests
// -----------------------------------------------------------------------

/// Verify that swapping a window east and then west returns it to its
/// original position — swap must be its own inverse.
#[test]
fn test_swap_east_then_west_roundtrip() {
    let _lock = TEST_MUTEX.lock().unwrap_or_else(|e| e.into_inner());

    let commands = vec![
        Event::MenuOpened { window_id: 0 }, // Settle
        Event::Command {
            command: Command::Window(Operation::Focus(Direction::First)),
        },
        // Capture initial positions via Focus(First).
        Event::Command {
            command: Command::Window(Operation::Swap(Direction::East)),
        },
        Event::Command {
            command: Command::Window(Operation::Swap(Direction::West)),
        },
    ];

    let expected_after_first = [
        (4, (0, TEST_MENUBAR_HEIGHT)),
        (3, (400, TEST_MENUBAR_HEIGHT)),
        (2, (800, TEST_MENUBAR_HEIGHT)),
    ];

    let check = |iteration, world: &mut World| {
        match iteration {
            1 => verify_window_positions(&expected_after_first, world),
            // After swap east + swap west, positions should return to first state.
            3 => verify_window_positions(&expected_after_first, world),
            _ => {}
        }
    };

    let mut bevy = setup_world();
    let mock_app = setup_process(bevy.world_mut());
    let internal_queue = Arc::new(RwLock::new(Vec::<Event>::new()));
    let event_queue = internal_queue.clone();

    let windows = Box::new(move |_| {
        (0..5)
            .map(|i| {
                let origin = Origin::new(100 * i, 0);
                let size = Size::new(TEST_WINDOW_WIDTH, TEST_WINDOW_HEIGHT);
                Window::new(Box::new(MockWindow::new(
                    i,
                    IRect { min: origin, max: origin + size },
                    event_queue.clone(),
                    mock_app.clone(),
                )))
            })
            .collect()
    });
    bevy.world_mut()
        .insert_resource(WindowManager(Box::new(MockWindowManager { windows })));

    run_main_loop(&mut bevy, &internal_queue, &commands, check);
}

// -----------------------------------------------------------------------
// Resize cycle test
// -----------------------------------------------------------------------

/// Verify that cycling through resize presets produces valid window
/// widths — no window should ever have a width <= 0 or exceed the display.
#[test]
fn test_resize_cycle_produces_valid_widths() {
    let _lock = TEST_MUTEX.lock().unwrap_or_else(|e| e.into_inner());

    let commands = vec![
        Event::MenuOpened { window_id: 0 }, // Settle
        Event::Command {
            command: Command::Window(Operation::Focus(Direction::First)),
        },
        Event::Command {
            command: Command::Window(Operation::Resize),
        },
        Event::Command {
            command: Command::Window(Operation::Resize),
        },
        Event::Command {
            command: Command::Window(Operation::Resize),
        },
    ];

    let check = |iteration, world: &mut World| {
        if iteration >= 2 {
            let mut query = world.query::<&Window>();
            for window in query.iter(world) {
                let frame = window.frame();
                assert!(
                    frame.width() > 0,
                    "Window {} has non-positive width {} after resize cycle {}",
                    window.id(),
                    frame.width(),
                    iteration
                );
                assert!(
                    frame.width() <= TEST_DISPLAY_WIDTH,
                    "Window {} width {} exceeds display {} after resize cycle {}",
                    window.id(),
                    frame.width(),
                    TEST_DISPLAY_WIDTH,
                    iteration
                );
            }
        }
    };

    let mut bevy = setup_world();
    let mock_app = setup_process(bevy.world_mut());
    let internal_queue = Arc::new(RwLock::new(Vec::<Event>::new()));
    let event_queue = internal_queue.clone();

    let windows = Box::new(move |_| {
        (0..3)
            .map(|i| {
                let origin = Origin::new(100 * i, 0);
                let size = Size::new(TEST_WINDOW_WIDTH, TEST_WINDOW_HEIGHT);
                Window::new(Box::new(MockWindow::new(
                    i,
                    IRect { min: origin, max: origin + size },
                    event_queue.clone(),
                    mock_app.clone(),
                )))
            })
            .collect()
    });
    bevy.world_mut()
        .insert_resource(WindowManager(Box::new(MockWindowManager { windows })));

    run_main_loop(&mut bevy, &internal_queue, &commands, check);
}

// -----------------------------------------------------------------------
// Full-width toggle test
// -----------------------------------------------------------------------

/// Verify that full-width mode makes a window span the entire display
/// width minus edge padding.
#[test]
fn test_fullwidth_toggle() {
    let _lock = TEST_MUTEX.lock().unwrap_or_else(|e| e.into_inner());

    let commands = vec![
        Event::MenuOpened { window_id: 0 }, // Settle
        Event::Command {
            command: Command::Window(Operation::Focus(Direction::First)),
        },
        Event::Command {
            command: Command::Window(Operation::FullWidth),
        },
    ];

    let check = |iteration, world: &mut World| {
        if iteration == 2 {
            let mut focused_query = world.query::<(&Window, &FocusedMarker)>();
            for (window, _) in focused_query.iter(world) {
                // Full-width window should occupy the entire display width.
                assert_eq!(
                    window.frame().width(),
                    TEST_DISPLAY_WIDTH,
                    "Focused window should span full display width after FullWidth toggle"
                );
            }
        }
    };

    let mut bevy = setup_world();
    let mock_app = setup_process(bevy.world_mut());
    let internal_queue = Arc::new(RwLock::new(Vec::<Event>::new()));
    let event_queue = internal_queue.clone();

    let windows = Box::new(move |_| {
        (0..3)
            .map(|i| {
                let origin = Origin::new(100 * i, 0);
                let size = Size::new(TEST_WINDOW_WIDTH, TEST_WINDOW_HEIGHT);
                Window::new(Box::new(MockWindow::new(
                    i,
                    IRect { min: origin, max: origin + size },
                    event_queue.clone(),
                    mock_app.clone(),
                )))
            })
            .collect()
    });
    bevy.world_mut()
        .insert_resource(WindowManager(Box::new(MockWindowManager { windows })));

    run_main_loop(&mut bevy, &internal_queue, &commands, check);
}

// -----------------------------------------------------------------------
// Stack and unstack test
// -----------------------------------------------------------------------
// Rapid focus cycling stress test
// -----------------------------------------------------------------------

/// Stress test: rapidly cycling focus east 20 times should never produce
/// an invalid window position (all x values must be within reasonable
/// bounds).
#[test]
fn test_rapid_focus_cycling_no_invalid_positions() {
    let _lock = TEST_MUTEX.lock().unwrap_or_else(|e| e.into_inner());

    let mut commands = vec![Event::MenuOpened { window_id: 0 }];
    for _ in 0..20 {
        commands.push(Event::Command {
            command: Command::Window(Operation::Focus(Direction::East)),
        });
    }

    let check = |_iteration, world: &mut World| {
        let mut query = world.query::<&Window>();
        for window in query.iter(world) {
            let x = window.frame().min.x;
            // Windows can be off-screen (slivers) but shouldn't be absurdly far.
            // A reasonable bound: no more than 2 display widths away.
            assert!(
                x.abs() < TEST_DISPLAY_WIDTH * 3,
                "Window {} has unreasonable x={} — possible layout divergence",
                window.id(),
                x
            );
        }
    };

    let mut bevy = setup_world();
    let mock_app = setup_process(bevy.world_mut());
    let internal_queue = Arc::new(RwLock::new(Vec::<Event>::new()));
    let event_queue = internal_queue.clone();

    let windows = Box::new(move |_| {
        (0..10)
            .map(|i| {
                let origin = Origin::new(50 * i, 0);
                let size = Size::new(TEST_WINDOW_WIDTH, TEST_WINDOW_HEIGHT);
                Window::new(Box::new(MockWindow::new(
                    i,
                    IRect { min: origin, max: origin + size },
                    event_queue.clone(),
                    mock_app.clone(),
                )))
            })
            .collect()
    });
    bevy.world_mut()
        .insert_resource(WindowManager(Box::new(MockWindowManager { windows })));

    run_main_loop(&mut bevy, &internal_queue, &commands, check);
}

// -----------------------------------------------------------------------
// Window height consistency after focus changes
// -----------------------------------------------------------------------

/// Verify that all windows maintain consistent heights after multiple
/// focus changes. This catches the off-screen menubar height subtraction
/// bug from a different angle.
#[test]
fn test_window_heights_consistent_after_focus_changes() {
    let _lock = TEST_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
    let expected_height = TEST_DISPLAY_HEIGHT - TEST_MENUBAR_HEIGHT;

    let commands = vec![
        Event::MenuOpened { window_id: 0 },
        Event::Command {
            command: Command::Window(Operation::Focus(Direction::First)),
        },
        Event::Command {
            command: Command::Window(Operation::Focus(Direction::East)),
        },
        Event::Command {
            command: Command::Window(Operation::Focus(Direction::East)),
        },
        Event::Command {
            command: Command::Window(Operation::Focus(Direction::West)),
        },
        Event::Command {
            command: Command::Window(Operation::Focus(Direction::Last)),
        },
        Event::Command {
            command: Command::Window(Operation::Focus(Direction::First)),
        },
    ];

    let check = |iteration, world: &mut World| {
        if iteration >= 1 {
            let mut query = world.query::<&Window>();
            for window in query.iter(world) {
                assert_eq!(
                    window.frame().height(),
                    expected_height,
                    "Window {} height {} != expected {} at iteration {}",
                    window.id(),
                    window.frame().height(),
                    expected_height,
                    iteration,
                );
            }
        }
    };

    let mut bevy = setup_world();
    let mock_app = setup_process(bevy.world_mut());
    let internal_queue = Arc::new(RwLock::new(Vec::<Event>::new()));
    let event_queue = internal_queue.clone();

    let windows = Box::new(move |_| {
        (0..5)
            .map(|i| {
                let origin = Origin::new(100 * i, 0);
                let size = Size::new(TEST_WINDOW_WIDTH, TEST_WINDOW_HEIGHT);
                Window::new(Box::new(MockWindow::new(
                    i,
                    IRect { min: origin, max: origin + size },
                    event_queue.clone(),
                    mock_app.clone(),
                )))
            })
            .collect()
    });
    bevy.world_mut()
        .insert_resource(WindowManager(Box::new(MockWindowManager { windows })));

    run_main_loop(&mut bevy, &internal_queue, &commands, check);
}

// -----------------------------------------------------------------------
// Center command test
// -----------------------------------------------------------------------

/// Verify that the center command places the focused window at the
/// horizontal center of the display.
#[test]
fn test_center_positions_window_correctly() {
    let _lock = TEST_MUTEX.lock().unwrap_or_else(|e| e.into_inner());

    let commands = vec![
        Event::MenuOpened { window_id: 0 },
        Event::Command {
            command: Command::Window(Operation::Focus(Direction::First)),
        },
        Event::Command {
            command: Command::Window(Operation::Center),
        },
    ];

    let centered_x = (TEST_DISPLAY_WIDTH - TEST_WINDOW_WIDTH) / 2;

    let check = |iteration, world: &mut World| {
        if iteration == 2 {
            let mut query = world.query::<(&Window, &FocusedMarker)>();
            for (window, _) in query.iter(world) {
                assert_eq!(
                    window.frame().min.x, centered_x,
                    "Centered window x={} should be {}",
                    window.frame().min.x, centered_x
                );
            }
        }
    };

    let mut bevy = setup_world();
    let mock_app = setup_process(bevy.world_mut());
    let internal_queue = Arc::new(RwLock::new(Vec::<Event>::new()));
    let event_queue = internal_queue.clone();

    let windows = Box::new(move |_| {
        (0..3)
            .map(|i| {
                let origin = Origin::new(100 * i, 0);
                let size = Size::new(TEST_WINDOW_WIDTH, TEST_WINDOW_HEIGHT);
                Window::new(Box::new(MockWindow::new(
                    i,
                    IRect { min: origin, max: origin + size },
                    event_queue.clone(),
                    mock_app.clone(),
                )))
            })
            .collect()
    });
    bevy.world_mut()
        .insert_resource(WindowManager(Box::new(MockWindowManager { windows })));

    run_main_loop(&mut bevy, &internal_queue, &commands, check);
}
