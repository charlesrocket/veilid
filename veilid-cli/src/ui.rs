use crate::command_processor::*;
use crate::peers_table_view::*;
use crate::settings::Settings;
use crossbeam_channel::Sender;
use cursive::align::*;
use cursive::event::*;
use cursive::theme::*;
use cursive::traits::*;
use cursive::utils::markup::StyledString;
use cursive::view::ScrollStrategy;
use cursive::views::*;
use cursive::Cursive;
use cursive::CursiveRunnable;
use cursive_flexi_logger_view::{CursiveLogWriter, FlexiLoggerView};
//use cursive_multiplex::*;
use log::*;
use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::rc::Rc;
use thiserror::Error;
use veilid_core::*;

//////////////////////////////////////////////////////////////
///
struct Dirty<T> {
    value: T,
    dirty: bool,
}

impl<T> Dirty<T> {
    pub fn new(value: T) -> Self {
        Self { value, dirty: true }
    }
    pub fn set(&mut self, value: T) {
        self.value = value;
        self.dirty = true;
    }
    pub fn get(&self) -> &T {
        &self.value
    }
    // pub fn get_mut(&mut self) -> &mut T {
    //     &mut self.value
    // }
    pub fn take_dirty(&mut self) -> bool {
        let is_dirty = self.dirty;
        self.dirty = false;
        is_dirty
    }
}

pub type UICallback = Box<dyn Fn(&mut Cursive) + Send>;

struct UIState {
    attachment_state: Dirty<AttachmentState>,
    network_started: Dirty<bool>,
    network_down_up: Dirty<(f32, f32)>,
    connection_state: Dirty<ConnectionState>,
    peers_state: Dirty<Vec<PeerTableData>>,
    node_id: Dirty<String>,
}

impl UIState {
    pub fn new() -> Self {
        Self {
            attachment_state: Dirty::new(AttachmentState::Detached),
            network_started: Dirty::new(false),
            network_down_up: Dirty::new((0.0, 0.0)),
            connection_state: Dirty::new(ConnectionState::Disconnected),
            peers_state: Dirty::new(Vec::new()),
            node_id: Dirty::new("".to_owned()),
        }
    }
}

//#[derive(Error, Debug)]
//#[error("???")]
//struct UIError;

pub struct UIInner {
    ui_state: UIState,
    log_colors: HashMap<Level, cursive::theme::Color>,
    cmdproc: Option<CommandProcessor>,
    cb_sink: Sender<Box<dyn FnOnce(&mut Cursive) + 'static + Send>>,
    cmd_history: VecDeque<String>,
    cmd_history_position: usize,
    cmd_history_max_size: usize,
    connection_dialog_state: Option<ConnectionState>,
}

type Handle<T> = Rc<RefCell<T>>;

#[derive(Clone)]
pub struct UI {
    siv: Handle<CursiveRunnable>,
    inner: Handle<UIInner>,
}

#[derive(Error, Debug)]
pub enum DumbError {
    // #[error("{0}")]
    // Message(String),
}

impl UI {
    /////////////////////////////////////////////////////////////////////////////////////
    // Private functions

    fn command_processor(s: &mut Cursive) -> CommandProcessor {
        let inner = Self::inner(s);
        inner.cmdproc.as_ref().unwrap().clone()
    }

    fn inner(s: &mut Cursive) -> std::cell::Ref<'_, UIInner> {
        s.user_data::<Handle<UIInner>>().unwrap().borrow()
    }
    fn inner_mut(s: &mut Cursive) -> std::cell::RefMut<'_, UIInner> {
        s.user_data::<Handle<UIInner>>().unwrap().borrow_mut()
    }

    fn setup_colors(siv: &mut CursiveRunnable, inner: &mut UIInner, settings: &Settings) {
        // Make colors
        let mut theme = cursive::theme::load_default();
        theme.shadow = settings.interface.theme.shadow;
        theme.borders = BorderStyle::from(&settings.interface.theme.borders);
        theme.palette.set_color(
            "background",
            Color::parse(settings.interface.theme.colors.background.as_str()).unwrap(),
        );
        theme.palette.set_color(
            "shadow",
            Color::parse(settings.interface.theme.colors.shadow.as_str()).unwrap(),
        );
        theme.palette.set_color(
            "view",
            Color::parse(settings.interface.theme.colors.view.as_str()).unwrap(),
        );
        theme.palette.set_color(
            "primary",
            Color::parse(settings.interface.theme.colors.primary.as_str()).unwrap(),
        );
        theme.palette.set_color(
            "secondary",
            Color::parse(settings.interface.theme.colors.secondary.as_str()).unwrap(),
        );
        theme.palette.set_color(
            "tertiary",
            Color::parse(settings.interface.theme.colors.tertiary.as_str()).unwrap(),
        );
        theme.palette.set_color(
            "title_primary",
            Color::parse(settings.interface.theme.colors.title_primary.as_str()).unwrap(),
        );
        theme.palette.set_color(
            "title_secondary",
            Color::parse(settings.interface.theme.colors.title_secondary.as_str()).unwrap(),
        );
        theme.palette.set_color(
            "highlight",
            Color::parse(settings.interface.theme.colors.highlight.as_str()).unwrap(),
        );
        theme.palette.set_color(
            "highlight_inactive",
            Color::parse(settings.interface.theme.colors.highlight_inactive.as_str()).unwrap(),
        );
        theme.palette.set_color(
            "highlight_text",
            Color::parse(settings.interface.theme.colors.highlight_text.as_str()).unwrap(),
        );
        siv.set_theme(theme);

        // Make log colors
        let mut colors = HashMap::<Level, cursive::theme::Color>::new();
        colors.insert(
            Level::Trace,
            Color::parse(settings.interface.theme.log_colors.trace.as_str()).unwrap(),
        );
        colors.insert(
            Level::Debug,
            Color::parse(settings.interface.theme.log_colors.debug.as_str()).unwrap(),
        );
        colors.insert(
            Level::Info,
            Color::parse(settings.interface.theme.log_colors.info.as_str()).unwrap(),
        );
        colors.insert(
            Level::Warn,
            Color::parse(settings.interface.theme.log_colors.warn.as_str()).unwrap(),
        );
        colors.insert(
            Level::Error,
            Color::parse(settings.interface.theme.log_colors.error.as_str()).unwrap(),
        );
        inner.log_colors = colors;
    }
    fn setup_quit_handler(siv: &mut Cursive) {
        siv.clear_global_callbacks(cursive::event::Event::CtrlChar('c'));

        siv.set_on_pre_event(cursive::event::Event::CtrlChar('c'), UI::quit_handler);
        siv.set_global_callback(cursive::event::Event::Key(Key::Esc), UI::quit_handler);
    }

    fn quit_handler(siv: &mut Cursive) {
        siv.add_layer(
            Dialog::text("Do you want to exit?")
                .button("Yes", |s| s.quit())
                .button("No", |s| {
                    s.pop_layer();
                    UI::setup_quit_handler(s);
                }),
        );
        siv.set_on_pre_event(cursive::event::Event::CtrlChar('c'), |s| {
            s.quit();
        });
        siv.set_global_callback(cursive::event::Event::Key(Key::Esc), |s| {
            s.pop_layer();
            UI::setup_quit_handler(s);
        });
    }
    fn clear_handler(siv: &mut Cursive) {
        cursive_flexi_logger_view::clear_log();
        UI::update_cb(siv);
    }
    fn node_events(s: &mut Cursive) -> ViewRef<FlexiLoggerView> {
        s.find_name("node-events").unwrap()
    }
    fn node_events_panel(
        s: &mut Cursive,
    ) -> ViewRef<Panel<ResizedView<NamedView<ScrollView<FlexiLoggerView>>>>> {
        s.find_name("node-events-panel").unwrap()
    }
    fn command_line(s: &mut Cursive) -> ViewRef<EditView> {
        s.find_name("command-line").unwrap()
    }
    fn button_attach(s: &mut Cursive) -> ViewRef<Button> {
        s.find_name("button-attach").unwrap()
    }
    fn status_bar(s: &mut Cursive) -> ViewRef<TextView> {
        s.find_name("status-bar").unwrap()
    }
    fn peers(s: &mut Cursive) -> ViewRef<PeersTableView> {
        s.find_name("peers").unwrap()
    }
    fn render_attachment_state<'a>(inner: &mut UIInner) -> &'a str {
        match inner.ui_state.attachment_state.get() {
            AttachmentState::Detached => " Detached [----]",
            AttachmentState::Attaching => "Attaching [/   ]",
            AttachmentState::AttachedWeak => " Attached [|   ]",
            AttachmentState::AttachedGood => " Attached [||  ]",
            AttachmentState::AttachedStrong => " Attached [||| ]",
            AttachmentState::FullyAttached => " Attached [||||]",
            AttachmentState::OverAttached => " Attached [++++]",
            AttachmentState::Detaching => "Detaching [////]",
        }
    }
    fn render_network_status(inner: &mut UIInner) -> String {
        match inner.ui_state.network_started.get() {
            false => "Down: ----KB/s Up: ----KB/s".to_owned(),
            true => {
                let (d, u) = inner.ui_state.network_down_up.get();
                format!("Down: {:.2}KB/s Up: {:.2}KB/s", d, u)
            }
        }
    }
    fn render_button_attach<'a>(inner: &mut UIInner) -> (&'a str, bool) {
        if let ConnectionState::Connected(_, _) = inner.ui_state.connection_state.get() {
            match inner.ui_state.attachment_state.get() {
                AttachmentState::Detached => ("Attach", true),
                AttachmentState::Attaching => ("Detach", true),
                AttachmentState::AttachedWeak => ("Detach", true),
                AttachmentState::AttachedGood => ("Detach", true),
                AttachmentState::AttachedStrong => ("Detach", true),
                AttachmentState::FullyAttached => ("Detach", true),
                AttachmentState::OverAttached => ("Detach", true),
                AttachmentState::Detaching => ("Detach", false),
            }
        } else {
            (" ---- ", false)
        }
    }

    fn on_command_line_edit(s: &mut Cursive, text: &str, _pos: usize) {
        let mut inner = Self::inner_mut(s);

        // save edited command to newest history slot
        let hlen = inner.cmd_history.len();
        inner.cmd_history_position = hlen - 1;
        inner.cmd_history[hlen - 1] = text.to_owned();
    }

    fn enable_command_ui(s: &mut Cursive, enabled: bool) {
        Self::command_line(s).set_enabled(enabled);
        Self::button_attach(s).set_enabled(enabled);
    }

    fn display_string_dialog_cb(
        s: &mut Cursive,
        title: String,
        contents: String,
        close_cb: UICallback,
    ) {
        // Creates a dialog around some text with a single button
        let close_cb = Rc::new(close_cb);
        let close_cb2 = close_cb.clone();
        s.add_layer(
            Dialog::around(TextView::new(contents).scrollable())
                .title(title)
                .button("Close", move |s| {
                    s.pop_layer();
                    close_cb(s);
                }), //.wrap_with(CircularFocus::new)
                    //.wrap_tab(),
        );
        s.set_global_callback(cursive::event::Event::Key(Key::Esc), move |s| {
            s.set_global_callback(cursive::event::Event::Key(Key::Esc), UI::quit_handler);
            s.pop_layer();
            close_cb2(s);
        });
    }

    fn run_command(s: &mut Cursive, text: &str) -> Result<(), String> {
        // disable ui
        Self::enable_command_ui(s, false);

        // run command
        s.set_global_callback(cursive::event::Event::Key(Key::Esc), |s| {
            let cmdproc = Self::command_processor(s);
            cmdproc.cancel_command();
        });

        let cmdproc = Self::command_processor(s);
        cmdproc.run_command(
            text,
            Box::new(|s| {
                s.set_global_callback(cursive::event::Event::Key(Key::Esc), UI::quit_handler);
                Self::enable_command_ui(s, true);
            }),
        )
    }

    fn on_command_line_entered(s: &mut Cursive, text: &str) {
        if text.trim().is_empty() {
            return;
        }
        // run command
        cursive_flexi_logger_view::push_to_log(StyledString::styled(
            format!("> {}", text),
            ColorStyle::primary(),
        ));
        match Self::run_command(s, text) {
            Ok(_) => {}
            Err(e) => {
                let color = *Self::inner_mut(s).log_colors.get(&Level::Error).unwrap();

                cursive_flexi_logger_view::push_to_log(StyledString::styled(
                    format!("> {}", text),
                    color,
                ));
                cursive_flexi_logger_view::push_to_log(StyledString::styled(
                    format!("  Error: {}", e),
                    color,
                ));
                return;
            }
        }
        // save to history unless it's a duplicate
        {
            let mut inner = Self::inner_mut(s);

            let hlen = inner.cmd_history.len();
            inner.cmd_history[hlen - 1] = text.to_owned();

            if hlen >= 2 && inner.cmd_history[hlen - 1] == inner.cmd_history[hlen - 2] {
                inner.cmd_history[hlen - 1] = "".to_string();
            } else {
                if hlen == inner.cmd_history_max_size {
                    inner.cmd_history.pop_front();
                }
                inner.cmd_history.push_back("".to_string());
            }
            let hlen = inner.cmd_history.len();
            inner.cmd_history_position = hlen - 1;
        }

        // Clear the edit field
        let mut cmdline = Self::command_line(s);
        cmdline.set_content("");
    }

    fn on_command_line_history(s: &mut Cursive, dir: bool) {
        let mut cmdline = Self::command_line(s);
        let mut inner = Self::inner_mut(s);
        // if at top of buffer or end of buffer, ignore
        if (!dir && inner.cmd_history_position == 0)
            || (dir && inner.cmd_history_position == (inner.cmd_history.len() - 1))
        {
            return;
        }

        // move the history position
        if dir {
            inner.cmd_history_position += 1;
        } else {
            inner.cmd_history_position -= 1;
        }

        // replace text with current line
        let hlen = inner.cmd_history_position;
        cmdline.set_content(inner.cmd_history[hlen].as_str());
    }

    fn on_button_attach_pressed(s: &mut Cursive) {
        let action: Option<bool> = match Self::inner_mut(s).ui_state.attachment_state.get() {
            AttachmentState::Detached => Some(true),
            AttachmentState::Attaching => Some(false),
            AttachmentState::AttachedWeak => Some(false),
            AttachmentState::AttachedGood => Some(false),
            AttachmentState::AttachedStrong => Some(false),
            AttachmentState::FullyAttached => Some(false),
            AttachmentState::OverAttached => Some(false),
            AttachmentState::Detaching => None,
        };
        let mut cmdproc = Self::command_processor(s);
        if let Some(a) = action {
            if a {
                cmdproc.attach();
            } else {
                cmdproc.detach();
            }
        }
    }

    fn refresh_button_attach(s: &mut Cursive) {
        let mut button_attach = UI::button_attach(s);
        let mut inner = Self::inner_mut(s);

        let (button_text, button_enable) = UI::render_button_attach(&mut inner);

        button_attach.set_label(button_text);
        button_attach.set_enabled(button_enable);
    }

    fn submit_connection_address(s: &mut Cursive) {
        let edit = s.find_name::<EditView>("connection-address").unwrap();
        let addr = (*edit.get_content()).clone();
        let sa = match addr.parse::<std::net::SocketAddr>() {
            Ok(sa) => Some(sa),
            Err(_) => {
                s.add_layer(Dialog::text("Invalid address").button("Close", |s| {
                    s.pop_layer();
                }));
                return;
            }
        };
        Self::command_processor(s).set_server_address(sa);
        Self::command_processor(s).start_connection();
    }

    fn show_connection_dialog(s: &mut Cursive, state: ConnectionState) -> bool {
        let mut inner = Self::inner_mut(s);

        let mut show: bool = false;
        let mut hide: bool = false;
        let mut reset: bool = false;
        match state {
            ConnectionState::Disconnected => {
                if inner.connection_dialog_state == None
                    || inner
                        .connection_dialog_state
                        .as_ref()
                        .unwrap()
                        .is_connected()
                {
                    show = true;
                } else if inner
                    .connection_dialog_state
                    .as_ref()
                    .unwrap()
                    .is_retrying()
                {
                    reset = true;
                }
            }
            ConnectionState::Connected(_, _) => {
                if inner.connection_dialog_state != None
                    && !inner
                        .connection_dialog_state
                        .as_ref()
                        .unwrap()
                        .is_connected()
                {
                    hide = true;
                }
            }
            ConnectionState::Retrying(_, _) => {
                if inner.connection_dialog_state == None
                    || inner
                        .connection_dialog_state
                        .as_ref()
                        .unwrap()
                        .is_connected()
                {
                    show = true;
                } else if inner
                    .connection_dialog_state
                    .as_ref()
                    .unwrap()
                    .is_disconnected()
                {
                    reset = true;
                }
            }
        }
        inner.connection_dialog_state = Some(state);
        drop(inner);
        if hide {
            s.pop_layer();
            s.pop_layer();
            return true;
        }
        if show {
            s.add_fullscreen_layer(Layer::with_color(
                ResizedView::with_full_screen(DummyView {}),
                ColorStyle::new(PaletteColor::Background, PaletteColor::Background),
            ));
            s.add_layer(
                Dialog::around(
                    LinearLayout::vertical().child(
                        LinearLayout::horizontal()
                            .child(TextView::new("Address:"))
                            .child(
                                EditView::new()
                                    .on_submit(|s, _| Self::submit_connection_address(s))
                                    .with_name("connection-address")
                                    .fixed_height(1)
                                    .min_width(40),
                            ),
                    ),
                )
                .title("Connect to server")
                .with_name("connection-dialog"),
            );

            return true;
        }
        if reset {
            let mut dlg = s.find_name::<Dialog>("connection-dialog").unwrap();
            dlg.clear_buttons();
            return true;
        }

        false
    }

    fn refresh_connection_dialog(s: &mut Cursive) {
        let new_state = Self::inner(s).ui_state.connection_state.get().clone();

        if !Self::show_connection_dialog(s, new_state.clone()) {
            return;
        }

        match new_state {
            ConnectionState::Disconnected => {
                let addr = match Self::command_processor(s).get_server_address() {
                    None => "".to_owned(),
                    Some(addr) => addr.to_string(),
                };
                debug!("address is {}", addr);
                let mut edit = s.find_name::<EditView>("connection-address").unwrap();
                edit.set_content(addr);
                edit.set_enabled(true);
                let mut dlg = s.find_name::<Dialog>("connection-dialog").unwrap();
                dlg.add_button("Connect", Self::submit_connection_address);
            }
            ConnectionState::Connected(_, _) => {}
            ConnectionState::Retrying(addr, _) => {
                //
                let mut edit = s.find_name::<EditView>("connection-address").unwrap();
                debug!("address is {}", addr);
                edit.set_content(addr.to_string());
                edit.set_enabled(false);
                let mut dlg = s.find_name::<Dialog>("connection-dialog").unwrap();
                dlg.add_button("Cancel", |s| {
                    Self::command_processor(s).cancel_reconnect();
                });
            }
        }
    }

    fn refresh_main_titlebar(s: &mut Cursive) {
        let mut main_window = UI::node_events_panel(s);
        let inner = Self::inner_mut(s);
        main_window.set_title(format!("Node: {}", inner.ui_state.node_id.get()));
    }

    fn refresh_statusbar(s: &mut Cursive) {
        let mut statusbar = UI::status_bar(s);

        let mut inner = Self::inner_mut(s);

        let mut status = StyledString::new();

        match inner.ui_state.connection_state.get() {
            ConnectionState::Disconnected => {
                status.append_styled(
                    "Disconnected ".to_string(),
                    ColorStyle::highlight_inactive(),
                );
                status.append_styled("|", ColorStyle::highlight_inactive());
            }
            ConnectionState::Retrying(addr, _) => {
                status.append_styled(
                    format!("Reconnecting to {} ", addr),
                    ColorStyle::highlight_inactive(),
                );
                status.append_styled("|", ColorStyle::highlight_inactive());
            }
            ConnectionState::Connected(addr, _) => {
                status.append_styled(
                    format!("Connected to {} ", addr),
                    ColorStyle::highlight_inactive(),
                );
                status.append_styled("|", ColorStyle::highlight_inactive());
                // Add attachment state
                status.append_styled(
                    format!(" {} ", UI::render_attachment_state(&mut inner)),
                    ColorStyle::highlight_inactive(),
                );
                status.append_styled("|", ColorStyle::highlight_inactive());
                // Add bandwidth status
                status.append_styled(
                    format!(" {} ", UI::render_network_status(&mut inner)),
                    ColorStyle::highlight_inactive(),
                );
                status.append_styled("|", ColorStyle::highlight_inactive());
                // Add tunnel status
                status.append_styled(" No Tunnels ", ColorStyle::highlight_inactive());
                status.append_styled("|", ColorStyle::highlight_inactive());
            }
        };

        statusbar.set_content(status);
    }

    fn refresh_peers(s: &mut Cursive) {
        let mut peers = UI::peers(s);
        let inner = Self::inner_mut(s);
        peers.set_items_stable(inner.ui_state.peers_state.get().clone());
    }

    fn update_cb(s: &mut Cursive) {
        let mut inner = Self::inner_mut(s);

        let mut refresh_statusbar = false;
        let mut refresh_button_attach = false;
        let mut refresh_connection_dialog = false;
        let mut refresh_peers = false;
        let mut refresh_main_titlebar = false;
        if inner.ui_state.attachment_state.take_dirty() {
            refresh_statusbar = true;
            refresh_button_attach = true;
            refresh_peers = true;
        }
        if inner.ui_state.network_started.take_dirty() {
            refresh_statusbar = true;
        }
        if inner.ui_state.network_down_up.take_dirty() {
            refresh_statusbar = true;
        }
        if inner.ui_state.connection_state.take_dirty() {
            refresh_statusbar = true;
            refresh_button_attach = true;
            refresh_connection_dialog = true;
            refresh_peers = true;
        }
        if inner.ui_state.peers_state.take_dirty() {
            refresh_peers = true;
        }
        if inner.ui_state.node_id.take_dirty() {
            refresh_main_titlebar = true;
        }

        drop(inner);

        if refresh_statusbar {
            Self::refresh_statusbar(s);
        }
        if refresh_button_attach {
            Self::refresh_button_attach(s);
        }
        if refresh_connection_dialog {
            Self::refresh_connection_dialog(s);
        }
        if refresh_peers {
            Self::refresh_peers(s);
        }
        if refresh_main_titlebar {
            Self::refresh_main_titlebar(s);
        }
    }

    ////////////////////////////////////////////////////////////////////////////
    // Public functions

    pub fn new(node_log_scrollback: usize, settings: &Settings) -> Self {
        cursive_flexi_logger_view::resize(node_log_scrollback);

        // Instantiate the cursive runnable
        let runnable = CursiveRunnable::new(
            || -> Result<Box<dyn cursive_buffered_backend::Backend>, Box<DumbError>> {
                let backend = cursive::backends::crossterm::Backend::init().unwrap();
                let buffered_backend = cursive_buffered_backend::BufferedBackend::new(backend);
                Ok(Box::new(buffered_backend))
            },
        );

        // Make the callback mechanism easily reachable
        let cb_sink = runnable.cb_sink().clone();

        // Create the UI object
        let this = Self {
            siv: Rc::new(RefCell::new(runnable)),
            inner: Rc::new(RefCell::new(UIInner {
                ui_state: UIState::new(),
                log_colors: Default::default(),
                cmdproc: None,
                cmd_history: {
                    let mut vd = VecDeque::new();
                    vd.push_back("".to_string());
                    vd
                },
                cmd_history_position: 0,
                cmd_history_max_size: settings.interface.command_line.history_size,
                connection_dialog_state: None,
                cb_sink,
            })),
        };

        let mut siv = this.siv.borrow_mut();
        let mut inner = this.inner.borrow_mut();

        // Make the inner object accessible in callbacks easily
        siv.set_user_data(this.inner.clone());

        // Create layouts

        let node_events_view = Panel::new(
            FlexiLoggerView::new()
                .with_name("node-events")
                .scrollable()
                .scroll_x(true)
                .scroll_y(true)
                .scroll_strategy(ScrollStrategy::StickToBottom)
                .full_screen(),
        )
        .title_position(HAlign::Left)
        .title("Node Events")
        .with_name("node-events-panel");

        let peers_table_view = PeersTableView::new()
            .column(PeerTableColumn::NodeId, "Node Id", |c| c.width(43))
            .column(PeerTableColumn::Address, "Address", |c| c)
            .column(PeerTableColumn::LatencyAvg, "Ping", |c| c.width(8))
            .column(PeerTableColumn::TransferDownAvg, "Down", |c| c.width(8))
            .column(PeerTableColumn::TransferUpAvg, "Up", |c| c.width(8))
            .with_name("peers")
            .full_width()
            .min_height(8);

        // attempt at using Mux. Mux has bugs, like resizing problems.
        // let mut mux = Mux::new();
        // let node_node_events_view = mux
        //     .add_below(node_events_view, mux.root().build().unwrap())
        //     .unwrap();
        // let node_peers_table_view = mux
        //     .add_below(peers_table_view, node_node_events_view)
        //     .unwrap();
        // mux.set_container_split_ratio(node_peers_table_view, 0.75)
        //     .unwrap();
        // let mut mainlayout = LinearLayout::vertical();
        // mainlayout.add_child(mux);

        // Back to fixed layout
        let mut mainlayout = LinearLayout::vertical();
        mainlayout.add_child(node_events_view);
        mainlayout.add_child(peers_table_view);
        // ^^^ fixed layout

        let mut command = StyledString::new();
        command.append_styled("Command> ", ColorStyle::title_primary());
        //
        mainlayout.add_child(
            LinearLayout::horizontal()
                .child(TextView::new(command))
                .child(
                    EditView::new()
                        .on_submit(UI::on_command_line_entered)
                        .on_edit(UI::on_command_line_edit)
                        .on_up_down(UI::on_command_line_history)
                        .style(ColorStyle::new(
                            PaletteColor::Background,
                            PaletteColor::Secondary,
                        ))
                        .with_name("command-line")
                        .full_screen()
                        .fixed_height(1),
                )
                .child(
                    Button::new("Attach", |s| {
                        UI::on_button_attach_pressed(s);
                    })
                    .with_name("button-attach"),
                ),
        );
        let mut version = StyledString::new();
        version.append_styled(
            concat!(" | veilid-cli v", env!("CARGO_PKG_VERSION")),
            ColorStyle::highlight_inactive(),
        );

        mainlayout.add_child(
            LinearLayout::horizontal()
                .color(Some(ColorStyle::highlight_inactive()))
                .child(
                    TextView::new("")
                        .with_name("status-bar")
                        .full_screen()
                        .fixed_height(1),
                )
                .child(TextView::new(version)),
        );

        siv.add_fullscreen_layer(mainlayout);

        UI::setup_colors(&mut siv, &mut inner, settings);
        UI::setup_quit_handler(&mut siv);
        siv.set_global_callback(cursive::event::Event::Ctrl(Key::K), UI::clear_handler);

        drop(inner);
        drop(siv);

        this
    }
    pub fn cursive_flexi_logger(&self) -> Box<CursiveLogWriter> {
        let mut flv =
            cursive_flexi_logger_view::cursive_flexi_logger(self.siv.borrow().cb_sink().clone());
        flv.set_colors(self.inner.borrow().log_colors.clone());
        flv
    }
    pub fn set_command_processor(&mut self, cmdproc: CommandProcessor) {
        let mut inner = self.inner.borrow_mut();
        inner.cmdproc = Some(cmdproc);
        let _ = inner.cb_sink.send(Box::new(UI::update_cb));
    }
    pub fn set_attachment_state(&mut self, state: AttachmentState) {
        let mut inner = self.inner.borrow_mut();
        inner.ui_state.attachment_state.set(state);
        let _ = inner.cb_sink.send(Box::new(UI::update_cb));
    }
    pub fn set_network_status(
        &mut self,
        started: bool,
        bps_down: u64,
        bps_up: u64,
        peers: Vec<PeerTableData>,
    ) {
        let mut inner = self.inner.borrow_mut();
        inner.ui_state.network_started.set(started);
        inner.ui_state.network_down_up.set((
            ((bps_down as f64) / 1000.0f64) as f32,
            ((bps_up as f64) / 1000.0f64) as f32,
        ));
        inner.ui_state.peers_state.set(peers);
        let _ = inner.cb_sink.send(Box::new(UI::update_cb));
    }
    pub fn set_config(&mut self, config: VeilidConfigInner) {
        let mut inner = self.inner.borrow_mut();
        inner.ui_state.node_id.set(
            config
                .network
                .node_id
                .map(|x| x.encode())
                .unwrap_or("<unknown>".to_owned()),
        );
    }
    pub fn set_connection_state(&mut self, state: ConnectionState) {
        let mut inner = self.inner.borrow_mut();
        inner.ui_state.connection_state.set(state);
        let _ = inner.cb_sink.send(Box::new(UI::update_cb));
    }

    pub fn add_node_event(&self, event: String) {
        let inner = self.inner.borrow();
        let color = *inner.log_colors.get(&Level::Info).unwrap();
        for line in event.lines() {
            cursive_flexi_logger_view::push_to_log(StyledString::styled(line, color));
        }
        let _ = inner.cb_sink.send(Box::new(UI::update_cb));
    }

    pub fn display_string_dialog<T: ToString, S: ToString>(
        &self,
        title: T,
        text: S,
        close_cb: UICallback,
    ) {
        let title = title.to_string();
        let text = text.to_string();
        let inner = self.inner.borrow();
        let _ = inner.cb_sink.send(Box::new(move |s| {
            UI::display_string_dialog_cb(s, title, text, close_cb)
        }));
    }

    pub fn quit(&self) {
        let inner = self.inner.borrow();
        let _ = inner.cb_sink.send(Box::new(|s| {
            s.quit();
        }));
    }

    pub fn send_callback(&self, callback: UICallback) {
        let inner = self.inner.borrow();
        let _ = inner.cb_sink.send(Box::new(move |s| callback(s)));
    }

    // Note: Cursive is not re-entrant, can't borrow_mut self.siv again after this
    pub async fn run_async(&mut self) {
        let mut siv = self.siv.borrow_mut();
        siv.run_async().await;
    }
    // pub fn run(&mut self) {
    //     let mut siv = self.siv.borrow_mut();
    //     siv.run();
    // }
}
