mod meter;
mod obs;
use crossbeam_channel::{Receiver, Sender};
use eframe::egui::{
    self, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Frame, Margin,
    RichText, Stroke,
};
use iconflow::{Pack, Size, Style, try_icon};
use meter::Meter;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

const SETTINGS_FILE: &str = "connection.json";
const APP_TITLE: &str = concat!("OBS Remote Volume Meter v", env!("CARGO_PKG_VERSION"));
fn main() -> eframe::Result {
    eframe::run_native(
        APP_TITLE,
        eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_inner_size([760.0, 480.0])
                .with_min_inner_size([0.0, 280.0]),
            ..Default::default()
        },
        Box::new(|cc| Ok(Box::new(App::new(cc)))),
    )
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
struct Saved {
    connections: Vec<Connection>,
    selected_connection: usize,
    theme: Theme,
    orientation: Orientation,
    large_mode: bool,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
struct Connection {
    name: String,
    host: String,
    port: u16,
    password: String,
    auto_start_connection: bool,
    auto_reconnect: bool,
    hidden_channels: BTreeSet<String>,
}

#[derive(Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
enum Theme {
    #[default]
    Dark,
    Light,
}

#[derive(Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
enum Orientation {
    #[default]
    Horizontal,
    Vertical,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct LegacySaved {
    host: String,
    port: u16,
    password: String,
    auto_connect: bool,
    theme: Theme,
    orientation: Orientation,
    large_mode: bool,
}
impl Default for Saved {
    fn default() -> Self {
        Self {
            connections: Vec::new(),
            selected_connection: 0,
            theme: Theme::Dark,
            orientation: Orientation::Horizontal,
            large_mode: false,
        }
    }
}

impl Default for Connection {
    fn default() -> Self {
        Self {
            name: String::new(),
            host: "127.0.0.1".into(),
            port: 4455,
            password: String::new(),
            auto_start_connection: true,
            auto_reconnect: true,
            hidden_channels: BTreeSet::new(),
        }
    }
}

impl Connection {
    fn display_name(&self) -> &str {
        let name = self.name.trim();
        if name.is_empty() {
            self.host.trim()
        } else {
            name
        }
    }

    fn address(&self) -> String {
        format!("{}:{}", self.host.trim(), self.port)
    }

    fn has_name(&self) -> bool {
        !self.name.trim().is_empty()
    }
}
enum Status {
    Disconnected,
    Connecting,
    Connected,
    Error(String),
}
struct App {
    saved: Saved,
    settings_path: PathBuf,
    settings: bool,
    channels: bool,
    selected_connection: usize,
    confirm_delete: Option<usize>,
    runtimes: Vec<Runtime>,
    last: Instant,
}

struct Runtime {
    status: Status,
    latest_connection_error: Option<String>,
    sources: BTreeMap<String, Vec<Meter>>,
    tx: Sender<obs::Command>,
    rx: Receiver<obs::Event>,
    desired_connected: bool,
    retry_at: Option<Instant>,
}

impl Runtime {
    fn new(ctx: egui::Context) -> Self {
        let (tx, rx) = obs::start(ctx);
        Self {
            status: Status::Disconnected,
            latest_connection_error: None,
            sources: BTreeMap::new(),
            tx,
            rx,
            desired_connected: false,
            retry_at: None,
        }
    }
}

struct DisplayedSource<'a> {
    connection: String,
    address: String,
    named_connection: bool,
    channel: String,
    meters: &'a Vec<Meter>,
}

fn source_header(ui: &mut egui::Ui, source: &DisplayedSource<'_>, vertical: bool) {
    if vertical {
        let response = ui.label(RichText::new(&source.connection).strong());
        if source.named_connection {
            response.on_hover_text(&source.address);
        }
        ui.scope(|ui| {
            ui.style_mut().interaction.tooltip_delay = 0.0;
            ui.add(egui::Label::new(&source.channel).truncate())
                .on_hover_text(&source.channel);
        });
    } else {
        let response =
            ui.label(RichText::new(format!("{} · {}", source.connection, source.channel)).strong());
        if source.named_connection {
            response.on_hover_text(&source.address);
        }
    }
}
impl App {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        install_icon_fonts(&cc.egui_ctx);
        let settings_path = settings_path();
        let saved = load_settings(&settings_path);
        apply_theme(&cc.egui_ctx, saved.theme);
        let runtimes = saved
            .connections
            .iter()
            .map(|_| Runtime::new(cc.egui_ctx.clone()))
            .collect();
        let open_settings = !saved.connections.is_empty()
            && !saved
                .connections
                .iter()
                .any(|connection| connection.auto_start_connection);
        let mut a = Self {
            selected_connection: saved
                .selected_connection
                .min(saved.connections.len().saturating_sub(1)),
            saved,
            settings_path,
            settings: open_settings,
            channels: false,
            confirm_delete: None,
            runtimes,
            last: Instant::now(),
        };
        let auto_start: Vec<usize> = a
            .saved
            .connections
            .iter()
            .enumerate()
            .filter_map(|(index, connection)| connection.auto_start_connection.then_some(index))
            .collect();
        for index in auto_start {
            a.connect(index);
        }
        a
    }
    fn connect(&mut self, index: usize) {
        self.selected_connection = index;
        self.saved.selected_connection = index;
        if let Err(error) = self.save_settings() {
            if let Some(runtime) = self.runtimes.get_mut(index) {
                runtime.status = Status::Error(error);
            }
            return;
        }
        if let Some(runtime) = self.runtimes.get_mut(index) {
            runtime.latest_connection_error = None;
        }
        self.launch_connection(index);
    }
    fn launch_connection(&mut self, index: usize) {
        let Some(connection) = self.saved.connections.get(index).cloned() else {
            return;
        };
        if let Some(runtime) = self.runtimes.get_mut(index) {
            runtime.sources.clear();
            runtime.status = Status::Connecting;
            runtime.desired_connected = true;
            runtime.retry_at = None;
            let _ = runtime.tx.send(obs::Command::Connect(obs::Settings {
                host: connection.host.trim().into(),
                port: connection.port,
                password: connection.password,
            }));
        }
    }
    fn disconnect(&mut self, index: usize) {
        if let Some(runtime) = self.runtimes.get_mut(index) {
            runtime.desired_connected = false;
            runtime.retry_at = None;
            let _ = runtime.tx.send(obs::Command::Disconnect);
            runtime.sources.clear();
            runtime.status = Status::Disconnected;
            runtime.latest_connection_error = None;
        }
    }
    fn add_connection(&mut self, ctx: &egui::Context) {
        self.saved.connections.push(Connection::default());
        self.runtimes.push(Runtime::new(ctx.clone()));
        self.selected_connection = self.saved.connections.len() - 1;
        self.saved.selected_connection = self.selected_connection;
        if let Err(error) = self.save_settings()
            && let Some(runtime) = self.runtimes.get_mut(self.selected_connection)
        {
            runtime.status = Status::Error(error);
        }
    }
    fn save_settings(&self) -> Result<(), String> {
        let json = serde_json::to_string_pretty(&self.saved)
            .map_err(|e| format!("Could not encode settings: {e}"))?;
        fs::write(&self.settings_path, format!("{json}\n"))
            .map_err(|e| format!("Could not save {}: {e}", self.settings_path.display()))
    }
    fn events(&mut self) {
        let now = Instant::now();
        for (index, runtime) in self.runtimes.iter_mut().enumerate() {
            let auto_reconnect = self
                .saved
                .connections
                .get(index)
                .is_some_and(|connection| connection.auto_reconnect);
            while let Ok(e) = runtime.rx.try_recv() {
                match e {
                    obs::Event::Connecting if runtime.desired_connected => {
                        runtime.status = Status::Connecting;
                    }
                    obs::Event::Connecting => {}
                    obs::Event::Connected if runtime.desired_connected => {
                        runtime.status = Status::Connected;
                        runtime.latest_connection_error = None;
                        runtime.retry_at = None;
                    }
                    obs::Event::Connected => {
                        let _ = runtime.tx.send(obs::Command::Disconnect);
                    }
                    obs::Event::Disconnected(error) => {
                        runtime.sources.clear();
                        if runtime.desired_connected {
                            let error = if error.is_empty() {
                                "Connection lost".into()
                            } else {
                                error
                            };
                            runtime.latest_connection_error = Some(error.clone());
                            if auto_reconnect {
                                runtime.status = Status::Connecting;
                                runtime.retry_at = Some(now + Duration::from_secs(2));
                            } else {
                                runtime.status = Status::Error(error);
                                runtime.desired_connected = false;
                                runtime.retry_at = None;
                            }
                        }
                    }
                    obs::Event::Meters(inputs) if runtime.desired_connected => {
                        for input in inputs {
                            let channels = runtime.sources.entry(input.name).or_default();
                            channels.resize_with(input.channels.len(), Meter::default);
                            for (meter, levels) in channels.iter_mut().zip(input.channels) {
                                meter.set(levels.peak, levels.magnitude);
                            }
                        }
                    }
                    obs::Event::Meters(_) => {}
                }
            }
        }
        let retries: Vec<usize> = self
            .runtimes
            .iter()
            .enumerate()
            .filter_map(|(index, runtime)| {
                (self.saved.connections[index].auto_reconnect
                    && runtime.desired_connected
                    && runtime.retry_at.is_some_and(|retry| retry <= now))
                .then_some(index)
            })
            .collect();
        for index in retries {
            self.launch_connection(index);
        }
    }
    fn dialog(&mut self, ctx: &egui::Context) {
        if !self.settings {
            return;
        }
        self.selected_connection = self
            .selected_connection
            .min(self.saved.connections.len().saturating_sub(1));
        let mut connect = false;
        let mut disconnect = false;
        let mut add = false;
        let mut changed = false;
        let mut reconnect_disabled = false;
        let mut open = self.settings;
        egui::Window::new("OBS connections")
            .collapsible(false)
            .default_width(440.0)
            .open(&mut open)
            .show(ctx, |ui| {
                ui.horizontal_wrapped(|ui| {
                    for (index, connection) in self.saved.connections.iter().enumerate() {
                        let response = ui.selectable_label(
                            index == self.selected_connection,
                            connection.display_name(),
                        );
                        let response = if connection.has_name() {
                            response.on_hover_text(connection.address())
                        } else {
                            response
                        };
                        if response.clicked() {
                            self.selected_connection = index;
                        }
                    }
                    if ui.button("Add").clicked() {
                        add = true;
                    }
                });
                ui.separator();
                if self.saved.connections.is_empty() {
                    ui.vertical_centered(|ui| {
                        ui.add_space(12.0);
                        ui.label("No connections configured yet");
                        ui.add_space(8.0);
                        if ui.button("Create a new connection").clicked() {
                            add = true;
                        }
                    });
                    return;
                }
                let connection = &mut self.saved.connections[self.selected_connection];
                egui::Grid::new("connection")
                    .num_columns(2)
                    .spacing([14.0, 10.0])
                    .show(ui, |ui| {
                        ui.label("Name");
                        changed |= ui
                            .text_edit_singleline(&mut connection.name)
                            .on_hover_text("Optional")
                            .changed();
                        ui.end_row();
                        ui.label("Host");
                        changed |= ui.text_edit_singleline(&mut connection.host).changed();
                        ui.end_row();
                        ui.label("Port");
                        changed |= ui
                            .add(egui::DragValue::new(&mut connection.port).range(1..=u16::MAX))
                            .changed();
                        ui.end_row();
                        ui.label("Password");
                        changed |= ui
                            .add(
                                egui::TextEdit::singleline(&mut connection.password)
                                    .password(true),
                            )
                            .changed();
                        ui.end_row();
                    });
                ui.label(
                    RichText::new("Connection data, including the password, is saved as plain text in connection.json beside the app.")
                        .small()
                        .color(ui.visuals().weak_text_color()),
                );
                ui.end_row();
                ui.add_space(8.0);
                changed |= ui
                    .checkbox(
                        &mut connection.auto_start_connection,
                        "Auto start connection",
                    )
                    .changed();
                ui.label(
                    RichText::new("Establish the connection on app startup.")
                        .small()
                        .color(ui.visuals().weak_text_color()),
                );
                let was_auto_reconnect = connection.auto_reconnect;
                let reconnect_changed = ui
                    .checkbox(&mut connection.auto_reconnect, "Auto reconnect")
                    .changed();
                changed |= reconnect_changed;
                reconnect_disabled =
                    reconnect_changed && was_auto_reconnect && !connection.auto_reconnect;
                ui.label(
                    RichText::new(
                        "Continuously try to establish a connection if it got lost or wasn't available at all.",
                    )
                    .small()
                    .color(ui.visuals().weak_text_color()),
                );
                ui.add_space(8.0);
                let status = &self.runtimes[self.selected_connection].status;
                ui.label(RichText::new(match status {
                    Status::Connecting => "Connecting...",
                    Status::Connected => "Connected",
                    Status::Disconnected | Status::Error(_) => "Disconnected",
                }).strong());
                ui.horizontal(|ui| {
                    let connecting = matches!(status, Status::Connecting);
                    let connected = matches!(status, Status::Connected);
                    if ui
                        .button(if connecting {
                            "Cancel connection"
                        } else if connected {
                                "Disconnect"
                            } else {
                                "Connect"
                            })
                        .clicked()
                    {
                        if connecting || connected {
                            disconnect = true;
                        } else {
                            connect = true;
                        }
                    }
                    if ui
                        .add(
                            egui::Button::new("Delete connection")
                                .fill(Color32::from_rgb(170, 45, 45)),
                        )
                        .clicked()
                    {
                        self.confirm_delete = Some(self.selected_connection);
                    }
                });
                let latest_error = match status {
                    Status::Error(error) => Some(error.as_str()),
                    _ => self.runtimes[self.selected_connection]
                        .latest_connection_error
                        .as_deref(),
                };
                if let Some(error) = latest_error {
                    ui.colored_label(
                        Color32::from_rgb(235, 93, 93),
                        format!("Latest connection error: {error}"),
                    );
                }
            });
        self.settings = open;
        if add {
            self.add_connection(ctx);
        }
        self.saved.selected_connection = self.selected_connection;
        if reconnect_disabled {
            let runtime = &mut self.runtimes[self.selected_connection];
            let was_reconnecting = matches!(runtime.status, Status::Connecting)
                && runtime.latest_connection_error.is_some();
            runtime.retry_at = None;
            if was_reconnecting || matches!(runtime.status, Status::Error(_)) {
                runtime.desired_connected = false;
                runtime.status = Status::Disconnected;
                let _ = runtime.tx.send(obs::Command::Disconnect);
            }
        }
        if changed && let Err(error) = self.save_settings() {
            self.runtimes[self.selected_connection].status = Status::Error(error);
        }
        if connect {
            self.connect(self.selected_connection);
        }
        if disconnect {
            self.disconnect(self.selected_connection);
        }
        if let Some(index) = self.confirm_delete {
            let label = self
                .saved
                .connections
                .get(index)
                .map(|connection| {
                    if connection.has_name() {
                        format!("{} ({})", connection.display_name(), connection.address())
                    } else {
                        connection.address()
                    }
                })
                .unwrap_or_default();
            let mut confirm = false;
            let mut cancel = false;
            egui::Window::new("Delete connection?")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    ui.label(format!("Permanently delete {label}?"));
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if ui
                            .add(egui::Button::new("Delete").fill(Color32::from_rgb(170, 45, 45)))
                            .clicked()
                        {
                            confirm = true;
                        }
                        if ui.button("Cancel").clicked() {
                            cancel = true;
                        }
                    });
                });
            if confirm {
                self.disconnect(index);
                self.saved.connections.remove(index);
                self.runtimes.remove(index);
                self.selected_connection = self
                    .selected_connection
                    .min(self.saved.connections.len().saturating_sub(1));
                self.saved.selected_connection = self.selected_connection;
                if self.saved.connections.is_empty() {
                    self.settings = false;
                    self.saved.large_mode = false;
                }
                if let Err(error) = self.save_settings() {
                    if let Some(runtime) = self.runtimes.get_mut(self.selected_connection) {
                        runtime.status = Status::Error(error);
                    } else {
                        eprintln!("{error}");
                    }
                }
                self.confirm_delete = None;
            } else if cancel {
                self.confirm_delete = None;
            }
        }
    }

    fn channels_dialog(&mut self, ctx: &egui::Context) {
        if !self.channels {
            return;
        }

        let mut open = self.channels;
        let mut changed = false;
        let no_channels = self
            .runtimes
            .iter()
            .all(|runtime| runtime.sources.is_empty());
        egui::Window::new("Channels")
            .open(&mut open)
            .resizable(true)
            .default_width(360.0)
            .min_width(360.0)
            .min_height(220.0)
            .show(ctx, |ui| {
                if no_channels {
                    ui.centered_and_justified(|ui| {
                        ui.label(
                            RichText::new(
                                "Create a connection first, then connect to it to receive channels.",
                            )
                            .color(ui.visuals().weak_text_color()),
                        );
                    });
                } else {
                    for index in 0..self.runtimes.len() {
                        let Some(connection) = self.saved.connections.get(index) else {
                            continue;
                        };
                        let title = if connection.has_name() {
                            format!("{} ({})", connection.display_name(), connection.address())
                        } else {
                            connection.address()
                        };
                        let sources: Vec<String> =
                            self.runtimes[index].sources.keys().cloned().collect();
                        egui::CollapsingHeader::new(title)
                            .default_open(true)
                            .show(ui, |ui| {
                                if sources.is_empty() {
                                    ui.label(
                                        RichText::new("No channels received yet")
                                            .color(ui.visuals().weak_text_color()),
                                    );
                                }
                                for source in sources {
                                    let hidden = self.saved.connections[index]
                                        .hidden_channels
                                        .contains(&source);
                                    ui.horizontal(|ui| {
                                        let color = if hidden {
                                            ui.visuals().weak_text_color()
                                        } else {
                                            ui.visuals().text_color()
                                        };
                                        ui.label(RichText::new(&source).color(color));
                                        if ui.button(if hidden { "Show" } else { "Hide" }).clicked() {
                                            let hidden_channels =
                                                &mut self.saved.connections[index].hidden_channels;
                                            if hidden {
                                                hidden_channels.remove(&source);
                                            } else {
                                                hidden_channels.insert(source);
                                            }
                                            changed = true;
                                        }
                                    });
                                }
                            });
                    }
                }
            });
        self.channels = open;
        if changed && let Err(error) = self.save_settings() {
            eprintln!("{error}");
        }
    }
}
impl eframe::App for App {
    fn clear_color(&self, _: &egui::Visuals) -> [f32; 4] {
        match self.saved.theme {
            Theme::Dark => Color32::from_rgb(27, 29, 31),
            Theme::Light => Color32::from_rgb(244, 245, 247),
        }
        .to_normalized_gamma_f32()
    }

    fn save(&mut self, _: &mut dyn eframe::Storage) {
        if let Err(error) = self.save_settings() {
            eprintln!("{error}");
        }
    }
    fn ui(&mut self, ui: &mut egui::Ui, _: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        ui.set_min_size(ui.available_size());
        if ctx.input(|input| input.key_pressed(egui::Key::F11)) {
            self.saved.large_mode = !self.saved.large_mode;
            if let Err(error) = self.save_settings() {
                eprintln!("{error}");
            }
        }
        self.events();
        if let Some(retry_at) = self
            .runtimes
            .iter()
            .filter_map(|runtime| runtime.retry_at)
            .min()
        {
            ctx.request_repaint_after(retry_at.saturating_duration_since(Instant::now()));
        }
        let now = Instant::now();
        let dt = (now - self.last).as_secs_f32().min(0.1);
        self.last = now;
        for runtime in &mut self.runtimes {
            for meters in runtime.sources.values_mut() {
                for meter in meters {
                    meter.update(dt);
                }
            }
        }
        let dark = self.saved.theme == Theme::Dark;
        let toolbar = if dark {
            Color32::from_rgb(35, 37, 40)
        } else {
            Color32::from_rgb(230, 232, 235)
        };
        let background = if dark {
            Color32::from_rgb(27, 29, 31)
        } else {
            Color32::from_rgb(244, 245, 247)
        };
        ui.painter().rect_filled(ui.max_rect(), 0.0, background);
        if !self.saved.large_mode {
            Frame::new()
                .fill(toolbar)
                .inner_margin(Margin::same(10))
                .show(ui, |ui| {
                    let show_title = ui.available_width() >= 620.0;
                    ui.horizontal(|ui| {
                        if show_title {
                            ui.label(RichText::new("OBS Remote Volume Meter").strong().size(16.0));
                        }
                        let connected = self
                            .runtimes
                            .iter()
                            .filter(|runtime| matches!(runtime.status, Status::Connected))
                            .count();
                        let connecting = self
                            .runtimes
                            .iter()
                            .filter(|runtime| matches!(runtime.status, Status::Connecting))
                            .count();
                        let reconnecting = self
                            .runtimes
                            .iter()
                            .zip(&self.saved.connections)
                            .filter(|(runtime, connection)| {
                                connection.auto_reconnect && runtime.desired_connected
                            })
                            .count();
                        let lost = self
                            .runtimes
                            .iter()
                            .zip(&self.saved.connections)
                            .filter(|(runtime, connection)| {
                                connection.auto_reconnect
                                    && runtime.desired_connected
                                    && !matches!(runtime.status, Status::Connected)
                                    && runtime.latest_connection_error.is_some()
                            })
                            .count();
                        let c = if lost > 0 && lost == reconnecting {
                            Color32::RED
                        } else if lost > 0 {
                            Color32::YELLOW
                        } else if connected > 0 {
                            Color32::GREEN
                        } else if connecting > 0 {
                            Color32::YELLOW
                        } else {
                            Color32::GRAY
                        };
                        let (indicator, _) =
                            ui.allocate_exact_size(egui::vec2(9.0, 9.0), egui::Sense::hover());
                        ui.painter().circle_filled(indicator.center(), 4.5, c);
                        ui.label(
                            RichText::new(format!("{connected} connected"))
                                .small()
                                .color(ui.visuals().weak_text_color()),
                        );
                        if lost > 0 {
                            ui.label(
                                RichText::new(format!("{lost} lost"))
                                    .small()
                                    .color(Color32::RED),
                            );
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.button("Connections").clicked() {
                                self.settings = true;
                            }
                            if ui
                                .button(lucide("sliders-vertical"))
                                .on_hover_text("Channels")
                                .clicked()
                            {
                                self.channels = true;
                            }
                            let (orientation_icon, orientation_tip) = match self.saved.orientation {
                                Orientation::Horizontal => {
                                    ("columns-3", "Switch to vertical meters")
                                }
                                Orientation::Vertical => ("rows-3", "Switch to horizontal meters"),
                            };
                            if ui
                                .button(lucide(orientation_icon))
                                .on_hover_text(orientation_tip)
                                .clicked()
                            {
                                self.saved.orientation = match self.saved.orientation {
                                    Orientation::Horizontal => Orientation::Vertical,
                                    Orientation::Vertical => Orientation::Horizontal,
                                };
                                if let Err(error) = self.save_settings() {
                                    eprintln!("{error}");
                                }
                            }
                            let (theme_icon, theme_tip) = match self.saved.theme {
                                Theme::Dark => ("sun", "Switch to light theme"),
                                Theme::Light => ("moon", "Switch to dark theme"),
                            };
                            if ui
                                .button(lucide(theme_icon))
                                .on_hover_text(theme_tip)
                                .clicked()
                            {
                                self.saved.theme = match self.saved.theme {
                                    Theme::Dark => Theme::Light,
                                    Theme::Light => Theme::Dark,
                                };
                                apply_theme(&ctx, self.saved.theme);
                                if let Err(error) = self.save_settings() {
                                    eprintln!("{error}");
                                }
                            }
                            if ui.button("Large  ·  F11").clicked() {
                                self.saved.large_mode = true;
                            }
                        });
                    });
                });
        }
        let mut create_connection = false;
        let displayed_sources: Vec<DisplayedSource<'_>> = self
            .runtimes
            .iter()
            .enumerate()
            .flat_map(|(index, runtime)| {
                let connection = &self.saved.connections[index];
                runtime
                    .sources
                    .iter()
                    .filter(move |(name, _)| !connection.hidden_channels.contains(*name))
                    .map(move |(name, channels)| DisplayedSource {
                        connection: connection.display_name().to_owned(),
                        address: connection.address(),
                        named_connection: connection.has_name(),
                        channel: name.clone(),
                        meters: channels,
                    })
            })
            .collect();
        Frame::new()
            .fill(background)
            .inner_margin(Margin::same(if self.saved.large_mode { 8 } else { 14 }))
            .show(ui, |ui| {
                let vertical_height = (ui.available_height()
                    - if self.saved.large_mode { 44.0 } else { 66.0 })
                .max(160.0);
                if displayed_sources.is_empty() {
                    if !self.saved.large_mode {
                        if self.saved.connections.is_empty() {
                            ui.vertical_centered(|ui| {
                                ui.add_space(((ui.available_height() - 60.0) / 2.0).max(0.0));
                                ui.label("No connections configured yet");
                                ui.add_space(8.0);
                                if ui.button("Create a new connection").clicked() {
                                    create_connection = true;
                                }
                            });
                        } else {
                            ui.centered_and_justified(|ui| {
                                ui.label(
                                    if self
                                        .runtimes
                                        .iter()
                                        .any(|runtime| matches!(runtime.status, Status::Connected))
                                    {
                                        "Waiting for active OBS audio sources…"
                                    } else {
                                        "Connect to OBS to monitor its audio levels"
                                    },
                                );
                            });
                        }
                    }
                } else if self.saved.large_mode {
                    if self.saved.orientation == Orientation::Vertical {
                        egui::ScrollArea::horizontal().show(ui, |ui| {
                            ui.with_layout(egui::Layout::left_to_right(egui::Align::TOP), |ui| {
                                for source in &displayed_sources {
                                    let channels = source.meters;
                                    let width = meter::vertical_group_width(
                                        channels.len(),
                                        true,
                                        ui.spacing().item_spacing.x,
                                    );
                                    ui.allocate_ui_with_layout(
                                        egui::vec2(width, vertical_height + 44.0),
                                        egui::Layout::top_down(egui::Align::Center),
                                        |ui| {
                                            source_header(ui, source, true);
                                            ui.with_layout(
                                                egui::Layout::left_to_right(egui::Align::TOP),
                                                |ui| {
                                                    for (index, meter) in
                                                        channels.iter().enumerate()
                                                    {
                                                        meter::draw(
                                                            ui,
                                                            meter,
                                                            index,
                                                            channels.len(),
                                                            true,
                                                            true,
                                                            Some(vertical_height),
                                                        );
                                                    }
                                                },
                                            );
                                        },
                                    );
                                    ui.add_space(12.0);
                                }
                            });
                        });
                    } else {
                        egui::ScrollArea::vertical().show(ui, |ui| {
                            for source in &displayed_sources {
                                let channels = source.meters;
                                source_header(ui, source, false);
                                for (index, meter) in channels.iter().enumerate() {
                                    meter::draw(
                                        ui,
                                        meter,
                                        index,
                                        channels.len(),
                                        false,
                                        true,
                                        None,
                                    );
                                }
                                ui.add_space(12.0);
                            }
                        });
                    }
                } else if self.saved.orientation == Orientation::Vertical {
                    egui::ScrollArea::horizontal().show(ui, |ui| {
                        ui.with_layout(egui::Layout::left_to_right(egui::Align::TOP), |ui| {
                            for source in &displayed_sources {
                                let channels = source.meters;
                                let card = if dark {
                                    Color32::from_rgb(39, 41, 44)
                                } else {
                                    Color32::WHITE
                                };
                                let width = meter::vertical_group_width(
                                    channels.len(),
                                    false,
                                    ui.spacing().item_spacing.x,
                                );
                                ui.allocate_ui_with_layout(
                                    egui::vec2(width + 20.0, vertical_height + 66.0),
                                    egui::Layout::top_down(egui::Align::Min),
                                    |ui| {
                                        Frame::new()
                                            .fill(card)
                                            .stroke(Stroke::new(
                                                1.0,
                                                ui.visuals().widgets.noninteractive.bg_stroke.color,
                                            ))
                                            .corner_radius(CornerRadius::same(4))
                                            .inner_margin(Margin::same(10))
                                            .show(ui, |ui| {
                                                ui.set_width(width);
                                                ui.vertical_centered(|ui| {
                                                    source_header(ui, source, true);
                                                    ui.add_space(5.0);
                                                    ui.with_layout(
                                                        egui::Layout::left_to_right(
                                                            egui::Align::TOP,
                                                        ),
                                                        |ui| {
                                                            for (i, meter) in
                                                                channels.iter().enumerate()
                                                            {
                                                                meter::draw(
                                                                    ui,
                                                                    meter,
                                                                    i,
                                                                    channels.len(),
                                                                    true,
                                                                    false,
                                                                    Some(vertical_height),
                                                                );
                                                            }
                                                        },
                                                    );
                                                });
                                            });
                                    },
                                );
                                ui.add_space(8.0);
                            }
                        });
                    });
                } else {
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        for source in &displayed_sources {
                            let channels = source.meters;
                            let card = if dark {
                                Color32::from_rgb(39, 41, 44)
                            } else {
                                Color32::WHITE
                            };
                            Frame::new()
                                .fill(card)
                                .stroke(Stroke::new(
                                    1.0,
                                    ui.visuals().widgets.noninteractive.bg_stroke.color,
                                ))
                                .corner_radius(CornerRadius::same(4))
                                .inner_margin(Margin::same(10))
                                .show(ui, |ui| {
                                    source_header(ui, source, false);
                                    ui.add_space(5.0);
                                    for (i, m) in channels.iter().enumerate() {
                                        meter::draw(ui, m, i, channels.len(), false, false, None);
                                    }
                                });
                            ui.add_space(8.0);
                        }
                    });
                }
            });
        if create_connection {
            self.add_connection(&ctx);
            self.settings = true;
        }
        if !self.saved.large_mode {
            self.dialog(&ctx);
            self.channels_dialog(&ctx);
        }
        ui.response().context_menu(|ui| {
            if ui
                .add_enabled(self.saved.large_mode, egui::Button::new("Exit Large Mode"))
                .clicked()
            {
                self.saved.large_mode = false;
                ui.close();
            }
            if ui.button("Exit App").clicked() {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        });
        if self
            .runtimes
            .iter()
            .flat_map(|runtime| runtime.sources.values().flatten())
            .any(Meter::moving)
        {
            ctx.request_repaint_after(std::time::Duration::from_millis(16));
        }
    }
}

fn apply_theme(ctx: &egui::Context, theme: Theme) {
    ctx.set_visuals(match theme {
        Theme::Dark => egui::Visuals::dark(),
        Theme::Light => egui::Visuals::light(),
    });
    #[cfg(debug_assertions)]
    ctx.all_styles_mut(|style| {
        style.debug.warn_if_rect_changes_id = false;
    });
}

fn install_icon_fonts(ctx: &egui::Context) {
    let mut definitions = FontDefinitions::default();
    let fallback_fonts: Vec<String> = definitions.font_data.keys().cloned().collect();
    for font in iconflow::fonts() {
        definitions.font_data.insert(
            font.family.to_owned(),
            Arc::new(FontData::from_static(font.bytes)),
        );
        let family = definitions
            .families
            .entry(FontFamily::Name(font.family.into()))
            .or_default();
        family.push(font.family.to_owned());
        family.extend(fallback_fonts.iter().cloned());
    }
    ctx.set_fonts(definitions);
}

fn lucide(name: &str) -> RichText {
    let icon = try_icon(Pack::Lucide, name, Style::Regular, Size::Regular)
        .expect("built-in Lucide icon is missing");
    let glyph = char::from_u32(icon.codepoint).expect("invalid Lucide codepoint");
    RichText::new(glyph.to_string()).font(FontId::new(16.0, FontFamily::Name(icon.family.into())))
}

fn settings_path() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(PathBuf::from))
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_else(|| PathBuf::from("."))
        .join(SETTINGS_FILE)
}

fn load_settings(path: &PathBuf) -> Saved {
    match fs::read_to_string(path) {
        Ok(json) => {
            let mut value: serde_json::Value = serde_json::from_str(&json).unwrap_or_default();
            if value.get("connections").is_some() {
                if let Some(connections) = value
                    .get_mut("connections")
                    .and_then(serde_json::Value::as_array_mut)
                {
                    for connection in connections {
                        let Some(connection) = connection.as_object_mut() else {
                            continue;
                        };
                        let previous = connection
                            .get("auto_connect")
                            .and_then(serde_json::Value::as_bool)
                            .unwrap_or(true);
                        connection
                            .entry("auto_start_connection")
                            .or_insert(previous.into());
                        connection
                            .entry("auto_reconnect")
                            .or_insert(previous.into());
                        connection.remove("auto_connect");
                    }
                }
                serde_json::from_value(value).unwrap_or_else(|error| {
                    eprintln!("Could not read {}: {error}", path.display());
                    Saved::default()
                })
            } else {
                let legacy: LegacySaved = serde_json::from_value(value).unwrap_or_default();
                Saved {
                    connections: vec![Connection {
                        name: String::new(),
                        host: if legacy.host.is_empty() {
                            "127.0.0.1".into()
                        } else {
                            legacy.host
                        },
                        port: if legacy.port == 0 { 4455 } else { legacy.port },
                        password: legacy.password,
                        auto_start_connection: legacy.auto_connect,
                        auto_reconnect: legacy.auto_connect,
                        hidden_channels: BTreeSet::new(),
                    }],
                    selected_connection: 0,
                    theme: legacy.theme,
                    orientation: legacy.orientation,
                    large_mode: legacy.large_mode,
                }
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Saved::default(),
        Err(error) => {
            eprintln!("Could not read {}: {error}", path.display());
            Saved::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saved_settings_start_without_connections() {
        assert!(Saved::default().connections.is_empty());
    }

    #[test]
    fn new_connections_enable_automatic_options() {
        let connection = Connection::default();
        assert!(connection.auto_start_connection);
        assert!(connection.auto_reconnect);
    }
}
