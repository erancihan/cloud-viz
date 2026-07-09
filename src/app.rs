//! The eframe application: provider/scope state machine, background fetch
//! workers, toolbar, details panel, and error guidance screens.

use crate::layout::{layout_topology, Layout};
use crate::model::*;
use crate::providers::{builtin_providers, CloudProvider};
use crate::theme::{self, Theme, ThemeKind};
use crate::ui::canvas::{self, Camera};
use crate::ui::{c32, c32a};
use eframe::egui::{
    self, Align, ComboBox, Context, FontId, Layout as EguiLayout, RichText, Ui,
};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::thread;

enum WorkerMsg {
    Status { generation: u64, status: ProviderStatus },
    Scopes { generation: u64, result: Result<Vec<ScopeOption>, ProviderError> },
    Topology { generation: u64, result: Result<Topology, ProviderError> },
}

enum Phase {
    Working(&'static str),
    Ready,
    Failed(ProviderError),
}

pub struct CloudVizApp {
    providers: Vec<Arc<dyn CloudProvider>>,
    provider_idx: usize,
    account_detail: Option<String>,
    scopes: Vec<ScopeOption>,
    scope_id: Option<String>,
    topology: Option<Topology>,
    layout: Option<Layout>,
    phase: Phase,
    selected: Option<usize>,
    theme: Theme,
    camera: Camera,
    tx: Sender<WorkerMsg>,
    rx: Receiver<WorkerMsg>,
    generation: u64,
}

impl CloudVizApp {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let (tx, rx) = channel();
        let providers = builtin_providers();
        let provider_idx = providers.iter().position(|p| !p.info().demo).unwrap_or(0);
        let mut app = Self {
            providers,
            provider_idx,
            account_detail: None,
            scopes: Vec::new(),
            scope_id: None,
            topology: None,
            layout: None,
            phase: Phase::Working("Starting…"),
            selected: None,
            theme: theme::DARK,
            camera: Camera::default(),
            tx,
            rx,
            generation: 0,
        };
        app.start_provider_pipeline(None);
        app
    }

    /// Full pipeline for the current provider: status -> scopes -> topology.
    /// Runs on a worker thread; a bumped generation discards stale results.
    fn start_provider_pipeline(&mut self, ctx: Option<Context>) {
        self.generation += 1;
        let generation = self.generation;
        self.phase = Phase::Working("Checking provider…");
        self.topology = None;
        self.layout = None;
        self.selected = None;
        self.scopes = Vec::new();
        self.scope_id = None;
        self.account_detail = None;

        let provider = Arc::clone(&self.providers[self.provider_idx]);
        let tx = self.tx.clone();
        thread::spawn(move || {
            let repaint = || {
                if let Some(ctx) = &ctx {
                    ctx.request_repaint();
                }
            };
            let status = provider.check_status();
            let ok = matches!(status, ProviderStatus::Ok { .. });
            let _ = tx.send(WorkerMsg::Status { generation, status });
            repaint();
            if !ok {
                return;
            }
            let scopes = provider.list_scopes();
            let default_scope = scopes.as_ref().ok().and_then(|s| {
                s.iter().find(|o| o.is_default).or(s.first()).map(|o| o.id.clone())
            });
            let _ = tx.send(WorkerMsg::Scopes { generation, result: scopes });
            repaint();
            let result = provider.fetch_topology(default_scope.as_deref());
            let _ = tx.send(WorkerMsg::Topology { generation, result });
            repaint();
        });
    }

    /// Re-fetch topology only (scope switch or manual refresh).
    fn start_topology_fetch(&mut self, ctx: Context) {
        self.generation += 1;
        let generation = self.generation;
        self.phase = Phase::Working("Fetching topology…");
        self.selected = None;

        let provider = Arc::clone(&self.providers[self.provider_idx]);
        let scope = self.scope_id.clone();
        let tx = self.tx.clone();
        thread::spawn(move || {
            let result = provider.fetch_topology(scope.as_deref());
            let _ = tx.send(WorkerMsg::Topology { generation, result });
            ctx.request_repaint();
        });
    }

    fn drain_worker_messages(&mut self) {
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                WorkerMsg::Status { generation, status } if generation == self.generation => match status {
                    ProviderStatus::Ok { account_label, detail } => {
                        self.account_detail = Some(detail.unwrap_or(account_label));
                        self.phase = Phase::Working("Loading subscriptions…");
                    }
                    ProviderStatus::Failed(err) => self.phase = Phase::Failed(err),
                },
                WorkerMsg::Scopes { generation, result } if generation == self.generation => match result {
                    Ok(scopes) => {
                        self.scope_id = scopes
                            .iter()
                            .find(|s| s.is_default)
                            .or(scopes.first())
                            .map(|s| s.id.clone());
                        self.scopes = scopes;
                        self.phase = Phase::Working("Fetching topology…");
                    }
                    Err(err) => self.phase = Phase::Failed(err),
                },
                WorkerMsg::Topology { generation, result } if generation == self.generation => match result {
                    Ok(topology) => {
                        self.layout = Some(layout_topology(&topology));
                        self.topology = Some(topology);
                        self.camera = Camera::default(); // triggers fit-to-view
                        self.phase = Phase::Ready;
                    }
                    Err(err) => self.phase = Phase::Failed(err),
                },
                _ => {} // stale generation
            }
        }
    }

    fn switch_provider(&mut self, idx: usize, ctx: &Context) {
        if idx != self.provider_idx {
            self.provider_idx = idx;
            self.start_provider_pipeline(Some(ctx.clone()));
        }
    }

    fn toolbar(&mut self, ui: &mut Ui, ctx: &Context) {
        ui.horizontal(|ui| {
            ui.add_space(4.0);
            ui.label(RichText::new("CloudViz").font(FontId::proportional(16.0)).strong().color(c32(self.theme.accent)));
            ui.add_space(12.0);

            ui.label(RichText::new("PROVIDER").size(10.0).color(c32(self.theme.ink_3)));
            let mut selected_idx = self.provider_idx;
            ComboBox::from_id_salt("provider")
                .selected_text(self.providers[self.provider_idx].info().display_name)
                .show_ui(ui, |ui| {
                    for (i, p) in self.providers.iter().enumerate() {
                        ui.selectable_value(&mut selected_idx, i, p.info().display_name);
                    }
                });
            self.switch_provider(selected_idx, ctx);

            if !self.scopes.is_empty() {
                ui.add_space(8.0);
                ui.label(RichText::new("SUBSCRIPTION").size(10.0).color(c32(self.theme.ink_3)));
                let current_label = self
                    .scopes
                    .iter()
                    .find(|s| Some(&s.id) == self.scope_id.as_ref())
                    .map(|s| s.label.clone())
                    .unwrap_or_default();
                let mut changed: Option<String> = None;
                ComboBox::from_id_salt("scope")
                    .selected_text(current_label)
                    .width(240.0)
                    .show_ui(ui, |ui| {
                        for s in &self.scopes {
                            let is_current = Some(&s.id) == self.scope_id.as_ref();
                            if ui.selectable_label(is_current, &s.label).clicked() && !is_current {
                                changed = Some(s.id.clone());
                            }
                        }
                    });
                if let Some(id) = changed {
                    self.scope_id = Some(id);
                    self.start_topology_fetch(ctx.clone());
                }
            }

            ui.add_space(8.0);
            let working = matches!(self.phase, Phase::Working(_));
            if ui.add_enabled(!working, egui::Button::new(if working { "Loading…" } else { "⟳ Refresh" })).clicked() {
                self.start_topology_fetch(ctx.clone());
            }
            if ui.button("Fit view").clicked() {
                self.camera.needs_fit = true;
            }

            ui.with_layout(EguiLayout::right_to_left(Align::Center), |ui| {
                ui.add_space(4.0);
                let label = if self.theme.kind == ThemeKind::Dark { "Light mode" } else { "Dark mode" };
                if ui.button(label).clicked() {
                    self.theme = if self.theme.kind == ThemeKind::Dark { theme::LIGHT } else { theme::DARK };
                    apply_visuals(ctx, &self.theme);
                }
                if let Some(detail) = &self.account_detail {
                    ui.label(RichText::new(detail).color(c32(self.theme.ink_2)));
                }
            });
        });
    }

    fn statusbar(&self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            let muted = c32(self.theme.ink_3);
            match (&self.topology, &self.phase) {
                (Some(t), _) => {
                    let resources = t
                        .nodes
                        .iter()
                        .filter(|n| !matches!(n.category, ResourceCategory::Scope | ResourceCategory::Group))
                        .count();
                    ui.label(RichText::new(format!(
                        "{}  ·  {}  ·  {} resources  ·  {} connections",
                        self.providers[self.provider_idx].info().display_name,
                        t.scope_label,
                        resources,
                        t.edges.len()
                    )).color(muted));
                    if !t.warnings.is_empty() {
                        ui.label(
                            RichText::new(format!("⚠ {} warnings", t.warnings.len())).color(c32(self.theme.warn)),
                        )
                        .on_hover_text(t.warnings.join("\n"));
                    }
                }
                (None, Phase::Working(msg)) => {
                    ui.label(RichText::new(*msg).color(muted));
                }
                _ => {
                    ui.label(RichText::new("Not connected").color(muted));
                }
            }
        });
    }

    fn details_panel(&mut self, ctx: &Context) {
        let Some(selected) = self.selected else { return };
        let Some(topology) = &self.topology else { return };
        let Some(node) = topology.nodes.get(selected).cloned() else {
            self.selected = None;
            return;
        };

        let mut open = true;
        egui::SidePanel::right("details")
            .resizable(true)
            .default_width(320.0)
            .min_width(240.0)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        ui.heading(RichText::new(&node.name).color(c32(self.theme.ink)));
                        ui.with_layout(EguiLayout::right_to_left(Align::Center), |ui| {
                            if ui.button("✕").clicked() {
                                open = false;
                            }
                        });
                    });
                    let cat = self.theme.category_color(node.category);
                    ui.horizontal(|ui| {
                        let (rect, _) = ui.allocate_exact_size(egui::Vec2::splat(9.0), egui::Sense::hover());
                        ui.painter().circle_filled(rect.center(), 4.0, c32(cat));
                        ui.label(
                            RichText::new(format!("{} · {}", node.kind_label, node.category.label()))
                                .color(c32(self.theme.ink_2)),
                        );
                    });
                    ui.add_space(8.0);
                    ui.separator();

                    let mut row = |key: &str, value: &str, mono: bool| {
                        ui.add_space(6.0);
                        ui.label(RichText::new(key.to_uppercase()).size(10.0).color(c32(self.theme.ink_3)));
                        let text = if mono {
                            RichText::new(value).font(FontId::monospace(11.0)).color(c32(self.theme.ink_2))
                        } else {
                            RichText::new(value).color(c32(self.theme.ink))
                        };
                        ui.add(egui::Label::new(text).wrap());
                        ui.add_space(6.0);
                        ui.separator();
                    };

                    if let Some(region) = &node.region {
                        row("Region", region, false);
                    }
                    row("Type", &node.kind, true);
                    row("Resource id", &node.id, true);
                    for (key, value) in &node.metadata {
                        row(key, value, false);
                    }
                });
            });
        if !open {
            self.selected = None;
        }
    }

    fn central(&mut self, ctx: &Context) {
        let theme = self.theme;
        egui::CentralPanel::default()
            .frame(egui::Frame::default().fill(c32(theme.plane)))
            .show(ctx, |ui| {
                match (&self.topology, &self.layout, &self.phase) {
                    (Some(topology), Some(layout), _) => {
                        let output = canvas::show(ui, &mut self.camera, topology, layout, &theme, self.selected);
                        if let Some(idx) = output.clicked_node {
                            self.selected = Some(idx);
                        } else if output.clicked_background {
                            self.selected = None;
                        }
                    }
                    (_, _, Phase::Working(msg)) => {
                        ui.centered_and_justified(|ui| {
                            ui.vertical_centered(|ui| {
                                ui.add_space(ui.available_height() * 0.4);
                                ui.spinner();
                                ui.add_space(10.0);
                                ui.label(RichText::new(*msg).color(c32(theme.ink_2)));
                            });
                        });
                    }
                    (_, _, Phase::Failed(err)) => {
                        let mut retry = false;
                        let mut open_demo = false;
                        self.error_screen(ui, err, &mut retry, &mut open_demo);
                        if retry {
                            self.start_provider_pipeline(Some(ctx.clone()));
                        }
                        if open_demo {
                            if let Some(idx) = self.providers.iter().position(|p| p.info().demo) {
                                self.provider_idx = idx;
                                self.start_provider_pipeline(Some(ctx.clone()));
                            }
                        }
                    }
                    _ => {}
                }
            });
    }

    fn error_screen(&self, ui: &mut Ui, err: &ProviderError, retry: &mut bool, open_demo: &mut bool) {
        let theme = &self.theme;
        let provider_name = self.providers[self.provider_idx].info().display_name;
        let (title, body): (String, String) = match err.code {
            ProviderErrorCode::CliMissing => (
                format!("{provider_name} CLI not found"),
                "CloudViz reads your infrastructure through the provider's own CLI, so it never stores credentials itself.".into(),
            ),
            ProviderErrorCode::NotAuthenticated => (
                format!("Sign in to {provider_name}"),
                "The CLI is installed but not signed in. Run this in a terminal, then try again:".into(),
            ),
            _ => (format!("Could not load {provider_name}"), err.message.clone()),
        };

        ui.vertical_centered(|ui| {
            ui.add_space(ui.available_height() * 0.3);
            ui.label(RichText::new(title).font(FontId::proportional(18.0)).strong().color(c32(theme.ink)));
            ui.add_space(6.0);
            ui.label(RichText::new(body).color(c32(theme.ink_2)));
            if err.code == ProviderErrorCode::NotAuthenticated {
                ui.add_space(8.0);
                ui.label(
                    RichText::new("az login")
                        .font(FontId::monospace(14.0))
                        .background_color(c32a(theme.surface, 255))
                        .color(c32(theme.ink)),
                );
            } else if let Some(hint) = &err.hint {
                ui.add_space(6.0);
                ui.label(RichText::new(hint).color(c32(theme.ink_2)));
            }
            ui.add_space(14.0);
            ui.horizontal(|ui| {
                // center the button row
                let spacing = (ui.available_width() - 260.0).max(0.0) / 2.0;
                ui.add_space(spacing);
                if ui.button("Try again").clicked() {
                    *retry = true;
                }
                let has_other_demo = self
                    .providers
                    .iter()
                    .enumerate()
                    .any(|(i, p)| p.info().demo && i != self.provider_idx);
                if has_other_demo && ui.button("Explore demo data").clicked() {
                    *open_demo = true;
                }
            });
        });
    }
}

impl eframe::App for CloudVizApp {
    fn update(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        self.drain_worker_messages();
        apply_visuals(ctx, &self.theme);

        egui::TopBottomPanel::top("toolbar")
            .exact_height(44.0)
            .show(ctx, |ui| self.toolbar(ui, ctx));
        egui::TopBottomPanel::bottom("statusbar")
            .exact_height(26.0)
            .show(ctx, |ui| self.statusbar(ui));
        self.details_panel(ctx);
        self.central(ctx);

        // Keep polling while a worker is in flight (cheap; workers also
        // request repaints when they finish).
        if matches!(self.phase, Phase::Working(_)) {
            ctx.request_repaint_after(std::time::Duration::from_millis(120));
        }
    }
}

fn apply_visuals(ctx: &Context, theme: &Theme) {
    let mut visuals = if theme.kind == ThemeKind::Dark {
        egui::Visuals::dark()
    } else {
        egui::Visuals::light()
    };
    visuals.panel_fill = c32(theme.surface);
    visuals.window_fill = c32(theme.surface);
    visuals.override_text_color = Some(c32(theme.ink));
    visuals.selection.bg_fill = c32a(theme.accent, 70);
    ctx.set_visuals(visuals);
}
