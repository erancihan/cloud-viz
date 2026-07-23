//! The eframe application: provider/scope state machine, background fetch
//! workers, toolbar, details panel, and error guidance screens.

use crate::cache;
use crate::config;
use crate::layout::{layout_topology, Layout};
use crate::model::*;
use crate::providers::{builtin_providers, CloudProvider};
use crate::theme::{self, Theme, ThemeKind};
use crate::ui::canvas::{self, Camera};
use crate::ui::{c32, c32a};
use eframe::egui::{self, Align, ComboBox, Context, FontId, Layout as EguiLayout, RichText, Ui};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

enum WorkerMsg {
    Status {
        generation: u64,
        status: ProviderStatus,
    },
    Scopes {
        generation: u64,
        result: Result<Vec<ScopeOption>, ProviderError>,
    },
    Topology {
        generation: u64,
        result: Result<Topology, ProviderError>,
        /// Some(age) when served from the on-disk cache instead of a fetch.
        cache_age: Option<Duration>,
    },
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
    fullscreen: bool,
    /// Some(age at load) when the current topology came from the disk cache.
    cache_age: Option<Duration>,
    /// Billing window the cost badges cover; changing it re-fetches (each
    /// period caches separately, so revisiting a month is instant).
    cost_period: CostPeriod,
    /// Last screen-zoom factor written to the config file, so Ctrl +/-
    /// changes persist across sessions without rewriting every frame.
    persisted_zoom: f32,
    tx: Sender<WorkerMsg>,
    rx: Receiver<WorkerMsg>,
    generation: u64,
}

impl CloudVizApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // Restore the persisted screen zoom (Ctrl +/-) from the last session.
        let saved = config::load();
        let persisted_zoom = saved
            .zoom_factor
            .map(|z| z.clamp(0.5, 3.0))
            .unwrap_or_else(|| cc.egui_ctx.zoom_factor());
        if saved.zoom_factor.is_some() {
            cc.egui_ctx.set_zoom_factor(persisted_zoom);
        }

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
            fullscreen: false,
            cache_age: None,
            cost_period: CostPeriod::MonthToDate,
            persisted_zoom,
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
        let period = self.cost_period;
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
                s.iter()
                    .find(|o| o.is_default)
                    .or(s.first())
                    .map(|o| o.id.clone())
            });
            let _ = tx.send(WorkerMsg::Scopes {
                generation,
                result: scopes,
            });
            repaint();
            // Start-up prefers the on-disk cache over re-running the whole
            // CLI inventory; ⟳ Refresh fetches live.
            let info = provider.info();
            let scope_key = cache_key(default_scope.as_deref(), period);
            if !info.demo {
                if let Some(hit) = cache::load(info.id, &scope_key) {
                    let _ = tx.send(WorkerMsg::Topology {
                        generation,
                        result: Ok(hit.topology),
                        cache_age: Some(hit.age),
                    });
                    repaint();
                    return;
                }
            }
            let result = provider.fetch_topology(default_scope.as_deref(), period);
            if let (Ok(topology), false) = (&result, info.demo) {
                cache::save(info.id, &scope_key, topology);
            }
            let _ = tx.send(WorkerMsg::Topology {
                generation,
                result,
                cache_age: None,
            });
            repaint();
        });
    }

    /// Re-fetch topology only (scope switch or manual refresh). `force` skips
    /// the on-disk cache — the ⟳ Refresh button always fetches live.
    fn start_topology_fetch(&mut self, ctx: Context, force: bool) {
        self.generation += 1;
        let generation = self.generation;
        self.phase = Phase::Working("Fetching topology…");
        self.selected = None;

        let provider = Arc::clone(&self.providers[self.provider_idx]);
        let scope = self.scope_id.clone();
        let period = self.cost_period;
        let tx = self.tx.clone();
        thread::spawn(move || {
            let info = provider.info();
            let scope_key = cache_key(scope.as_deref(), period);
            if !force && !info.demo {
                if let Some(hit) = cache::load(info.id, &scope_key) {
                    let _ = tx.send(WorkerMsg::Topology {
                        generation,
                        result: Ok(hit.topology),
                        cache_age: Some(hit.age),
                    });
                    ctx.request_repaint();
                    return;
                }
            }
            let result = provider.fetch_topology(scope.as_deref(), period);
            if let (Ok(topology), false) = (&result, info.demo) {
                cache::save(info.id, &scope_key, topology);
            }
            let _ = tx.send(WorkerMsg::Topology {
                generation,
                result,
                cache_age: None,
            });
            ctx.request_repaint();
        });
    }

    fn drain_worker_messages(&mut self) {
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                WorkerMsg::Status { generation, status } if generation == self.generation => {
                    match status {
                        ProviderStatus::Ok {
                            account_label,
                            detail,
                        } => {
                            self.account_detail = Some(detail.unwrap_or(account_label));
                            self.phase = Phase::Working("Loading subscriptions…");
                        }
                        ProviderStatus::Failed(err) => self.phase = Phase::Failed(err),
                    }
                }
                WorkerMsg::Scopes { generation, result } if generation == self.generation => {
                    match result {
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
                    }
                }
                WorkerMsg::Topology {
                    generation,
                    result,
                    cache_age,
                } if generation == self.generation => match result {
                    Ok(topology) => {
                        self.layout = Some(layout_topology(&topology));
                        self.topology = Some(topology);
                        self.cache_age = cache_age;
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

    /// Request real (borderless) fullscreen. Preferred over the OS maximize
    /// button: under WSLg the compositor leaves the previous window's
    /// client-side decorations — border and drop shadow — painted at their old
    /// bounds when a window is merely maximized. True fullscreen has no
    /// decorations, so nothing is left behind.
    fn set_fullscreen(&mut self, ctx: &Context, on: bool) {
        self.fullscreen = on;
        ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(on));
    }

    fn toolbar(&mut self, ui: &mut Ui, ctx: &Context) {
        ui.horizontal(|ui| {
            ui.add_space(4.0);
            ui.label(
                RichText::new("CloudViz")
                    .font(FontId::proportional(16.0))
                    .strong()
                    .color(c32(self.theme.accent)),
            );
            ui.add_space(12.0);

            ui.label(
                RichText::new("PROVIDER")
                    .size(10.0)
                    .color(c32(self.theme.ink_3)),
            );
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
                ui.label(
                    RichText::new("SUBSCRIPTION")
                        .size(10.0)
                        .color(c32(self.theme.ink_3)),
                );
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
                    self.start_topology_fetch(ctx.clone(), false);
                }
            }

            ui.add_space(8.0);
            ui.label(
                RichText::new("COSTS")
                    .size(10.0)
                    .color(c32(self.theme.ink_3)),
            );
            let mut new_period: Option<CostPeriod> = None;
            ComboBox::from_id_salt("cost-period")
                .selected_text(self.cost_period.label())
                .show_ui(ui, |ui| {
                    let (year, month) = current_year_month();
                    let options =
                        std::iter::once(CostPeriod::MonthToDate).chain((1..=12).map(|back| {
                            let (year, month) = month_minus(year, month, back);
                            CostPeriod::Month { year, month }
                        }));
                    for option in options {
                        let is_current = option == self.cost_period;
                        if ui.selectable_label(is_current, option.label()).clicked() && !is_current
                        {
                            new_period = Some(option);
                        }
                    }
                });
            if let Some(period) = new_period {
                self.cost_period = period;
                self.start_topology_fetch(ctx.clone(), false);
            }

            ui.add_space(8.0);
            let working = matches!(self.phase, Phase::Working(_));
            if ui
                .add_enabled(
                    !working,
                    egui::Button::new(if working { "Loading…" } else { "⟳ Refresh" }),
                )
                .clicked()
            {
                self.start_topology_fetch(ctx.clone(), true);
            }
            if ui.button("Fit view").clicked() {
                self.camera.needs_fit = true;
            }
            let fs_label = if self.fullscreen {
                "Exit full screen"
            } else {
                "Full screen"
            };
            if ui.button(fs_label).on_hover_text("F11").clicked() {
                self.set_fullscreen(ctx, !self.fullscreen);
            }

            ui.with_layout(EguiLayout::right_to_left(Align::Center), |ui| {
                ui.add_space(4.0);
                let label = if self.theme.kind == ThemeKind::Dark {
                    "Light mode"
                } else {
                    "Dark mode"
                };
                if ui.button(label).clicked() {
                    self.theme = if self.theme.kind == ThemeKind::Dark {
                        theme::LIGHT
                    } else {
                        theme::DARK
                    };
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
                        .filter(|n| {
                            !matches!(
                                n.category,
                                ResourceCategory::Scope | ResourceCategory::Group
                            )
                        })
                        .count();
                    ui.label(
                        RichText::new(format!(
                            "{}  ·  {}  ·  {} resources  ·  {} connections",
                            self.providers[self.provider_idx].info().display_name,
                            t.scope_label,
                            resources,
                            t.edges.len()
                        ))
                        .color(muted),
                    );
                    if let Some(age) = self.cache_age {
                        ui.label(
                            RichText::new(format!("·  cached {} ago", fmt_age(age))).color(muted),
                        )
                        .on_hover_text(
                            "Showing locally cached inventory — press ⟳ Refresh to fetch live data",
                        );
                    }
                    if !t.warnings.is_empty() {
                        ui.label(
                            RichText::new(format!("⚠ {} warnings", t.warnings.len()))
                                .color(c32(self.theme.warn)),
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
        let Some(selected) = self.selected else {
            return;
        };
        let Some(topology) = &self.topology else {
            return;
        };
        let Some(node) = topology.nodes.get(selected).cloned() else {
            self.selected = None;
            return;
        };
        let currency = topology.currency.clone();
        // Containers show the rolled-up subtree cost (what the box badge
        // displays) instead of the per-attachment breakdown leaves get.
        let subtree_total = node
            .container
            .then(|| subtree_costs(topology)[selected])
            .flatten();
        // Teardown needs the whole graph (edge dependents, container
        // children), so build it before the panel borrows anything.
        let plan = delete_plan(topology, selected);

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
                        let (rect, _) =
                            ui.allocate_exact_size(egui::Vec2::splat(9.0), egui::Sense::hover());
                        ui.painter().circle_filled(rect.center(), 4.0, c32(cat));
                        ui.label(
                            RichText::new(format!(
                                "{} · {}",
                                node.kind_label,
                                node.category.label()
                            ))
                            .color(c32(self.theme.ink_2)),
                        );
                    });
                    ui.add_space(8.0);
                    ui.separator();

                    let theme = self.theme;

                    // Collapsible sections (IntelliJ-style accordion), cost
                    // breakdown at the bottom.
                    section(ui, &theme, "Overview", true, |ui| {
                        if let Some(group) = &node.group {
                            detail_row(ui, &theme, "Resource group", group, false);
                        }
                        if let Some(region) = &node.region {
                            detail_row(ui, &theme, "Region", region, false);
                        }
                        detail_row(ui, &theme, "Type", &node.kind, true);
                        detail_row(ui, &theme, "Resource id", &node.id, true);
                    });

                    // Folded-in subsidiaries (disks, NICs, extensions, slots,
                    // SSH keys), grouped by kind in first-seen order. Their
                    // costs live in the Cost section below.
                    if !node.attachments.is_empty() {
                        let title = format!("Attached resources ({})", node.attachments.len());
                        section(ui, &theme, &title, true, |ui| {
                            let mut kinds: Vec<&str> = Vec::new();
                            for att in &node.attachments {
                                if !kinds.contains(&att.kind.as_str()) {
                                    kinds.push(&att.kind);
                                }
                            }
                            for kind in kinds {
                                let names: Vec<String> = node
                                    .attachments
                                    .iter()
                                    .filter(|a| a.kind == kind)
                                    .map(|a| {
                                        if a.shared {
                                            format!("{} (shared)", a.name)
                                        } else {
                                            a.name.clone()
                                        }
                                    })
                                    .collect();
                                detail_row(
                                    ui,
                                    &theme,
                                    &format!("{kind}s ({})", names.len()),
                                    &names.join("\n"),
                                    false,
                                );
                            }
                        });
                    }

                    if !node.metadata.is_empty() {
                        section(ui, &theme, "Metadata", true, |ui| {
                            for (key, value) in &node.metadata {
                                detail_row(ui, &theme, key, value, false);
                            }
                        });
                    }

                    // The selected period's spend — the card badge's number,
                    // itemized: the resource itself, then each folded-in
                    // subsidiary that accrued cost, then the total. Containers
                    // instead show the badge's subtree rollup.
                    if let Some(total) = subtree_total {
                        let currency = currency.as_deref();
                        let title = format!("Cost · {}", self.cost_period.label());
                        section(ui, &theme, &title, true, |ui| {
                            ui.add_space(4.0);
                            if let Some(own) = node.cost {
                                cost_row(
                                    ui,
                                    &theme,
                                    "This resource",
                                    &format_cost(own, currency),
                                    false,
                                );
                            }
                            cost_row(
                                ui,
                                &theme,
                                "Everything inside",
                                &format_cost(total, currency),
                                true,
                            );
                            ui.add_space(4.0);
                        });
                    } else if let Some(total) = node.total_cost() {
                        let currency = currency.as_deref();
                        let title = format!("Cost · {}", self.cost_period.label());
                        section(ui, &theme, &title, true, |ui| {
                            ui.add_space(4.0);
                            let costed: Vec<&Attachment> = node
                                .attachments
                                .iter()
                                .filter(|a| a.cost.is_some())
                                .collect();
                            if !costed.is_empty() {
                                if let Some(own) = node.cost {
                                    cost_row(
                                        ui,
                                        &theme,
                                        "This resource",
                                        &format_cost(own, currency),
                                        false,
                                    );
                                }
                                for att in costed {
                                    cost_row(
                                        ui,
                                        &theme,
                                        &format!("{} · {}", att.name, att.kind),
                                        &format_cost(att.cost.unwrap_or(0.0), currency),
                                        false,
                                    );
                                }
                                ui.add_space(2.0);
                                ui.separator();
                            }
                            cost_row(ui, &theme, "Total", &format_cost(total, currency), true);
                            ui.add_space(4.0);
                        });
                    }

                    // Teardown commands: the resource and everything folded
                    // into its card, dependency-ordered so nothing is left
                    // behind. Collapsed by default — it's the dangerous one.
                    if let Some(plan) = &plan {
                        section(ui, &theme, "Delete", false, |ui| {
                            ui.add_space(4.0);
                            ui.add(
                                egui::Label::new(
                                    RichText::new(
                                        "Irreversible. Run top to bottom — the order \
                                         frees dependents first (a VM releases its NIC, \
                                         the NIC its public IP) so nothing is left behind.",
                                    )
                                    .size(11.0)
                                    .color(c32(theme.warn)),
                                )
                                .wrap(),
                            );
                            ui.add_space(6.0);
                            let script: String = plan
                                .steps
                                .iter()
                                .enumerate()
                                .map(|(i, s)| format!("# {}. {}\n{}\n", i + 1, s.label, s.command))
                                .collect();
                            if ui.button("⧉  Copy commands").clicked() {
                                ui.ctx().copy_text(script.clone());
                            }
                            ui.add_space(6.0);
                            ui.add(
                                egui::Label::new(
                                    RichText::new(script.trim_end())
                                        .font(FontId::monospace(10.0))
                                        .color(c32(theme.ink_2)),
                                )
                                .wrap(),
                            );
                            for note in &plan.notes {
                                ui.add_space(4.0);
                                ui.add(
                                    egui::Label::new(
                                        RichText::new(format!("• {note}"))
                                            .size(10.5)
                                            .color(c32(theme.ink_3)),
                                    )
                                    .wrap(),
                                );
                            }
                            ui.add_space(4.0);
                        });
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
                        let output = canvas::show(
                            ui,
                            &mut self.camera,
                            topology,
                            layout,
                            &theme,
                            self.selected,
                        );
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

    fn error_screen(
        &self,
        ui: &mut Ui,
        err: &ProviderError,
        retry: &mut bool,
        open_demo: &mut bool,
    ) {
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
            ui.label(
                RichText::new(title)
                    .font(FontId::proportional(18.0))
                    .strong()
                    .color(c32(theme.ink)),
            );
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

        // Keep our flag in sync with the real window state (fullscreen can also
        // be left via the WM), then toggle on F11.
        if let Some(fs) = ctx.input(|i| i.viewport().fullscreen) {
            self.fullscreen = fs;
        }
        if ctx.input(|i| i.key_pressed(egui::Key::F11)) {
            self.set_fullscreen(ctx, !self.fullscreen);
        }

        // Persist the screen zoom (Ctrl +/-, Ctrl 0) so the next session
        // starts at the same zoom. Written only when it actually changes.
        let zoom = ctx.zoom_factor();
        if (zoom - self.persisted_zoom).abs() > 0.001 {
            self.persisted_zoom = zoom;
            config::save(&config::Config {
                zoom_factor: Some(zoom),
            });
        }

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

/// One collapsible details-panel section — an uppercase header with a
/// disclosure triangle (accordion). Open state persists for the session.
fn section(ui: &mut Ui, theme: &Theme, title: &str, default_open: bool, add: impl FnOnce(&mut Ui)) {
    egui::CollapsingHeader::new(
        RichText::new(title.to_uppercase())
            .size(10.5)
            .strong()
            .color(c32(theme.ink_3)),
    )
    .default_open(default_open)
    .show(ui, |ui| {
        ui.add_space(2.0);
        add(ui);
        ui.add_space(4.0);
    });
    ui.separator();
}

/// A key-over-value line inside a details-panel section.
fn detail_row(ui: &mut Ui, theme: &Theme, key: &str, value: &str, mono: bool) {
    ui.add_space(6.0);
    ui.label(
        RichText::new(key.to_uppercase())
            .size(10.0)
            .color(c32(theme.ink_3)),
    );
    let text = if mono {
        RichText::new(value)
            .font(FontId::monospace(11.0))
            .color(c32(theme.ink_2))
    } else {
        RichText::new(value).color(c32(theme.ink))
    };
    ui.add(egui::Label::new(text).wrap());
    ui.add_space(6.0);
}

/// One line of the cost breakdown: label left, amount right-aligned. The
/// label gets only the width the amount leaves over and truncates with an
/// ellipsis, so long resource names never run under the price.
fn cost_row(ui: &mut Ui, theme: &Theme, label: &str, amount: &str, strong: bool) {
    ui.horizontal(|ui| {
        let mut name =
            RichText::new(label).color(c32(if strong { theme.ink } else { theme.ink_2 }));
        let mut value = RichText::new(amount).color(c32(theme.ink));
        if strong {
            name = name.strong();
            value = value.strong();
        }
        let font = egui::TextStyle::Body.resolve(ui.style());
        let amount_w = ui.fonts(|f| {
            f.layout_no_wrap(amount.to_string(), font, egui::Color32::PLACEHOLDER)
                .rect
                .width()
        });
        let label_w = (ui.available_width() - amount_w - 12.0).max(40.0);
        ui.scope(|ui| {
            ui.set_min_width(label_w);
            ui.set_max_width(label_w);
            ui.add(egui::Label::new(name).truncate())
                .on_hover_text(label);
        });
        ui.with_layout(EguiLayout::right_to_left(Align::Center), |ui| {
            ui.label(value);
        });
    });
}

/// On-disk cache key for a scope + cost-period pair. Month-to-date keeps the
/// plain scope key, so caches written before period selection stay valid;
/// each past month caches under its own key (its costs never change, so
/// revisiting it is instant).
fn cache_key(scope: Option<&str>, period: CostPeriod) -> String {
    let scope = scope.unwrap_or("default");
    match period.cache_suffix() {
        Some(suffix) => format!("{scope}--{suffix}"),
        None => scope.to_string(),
    }
}

fn current_year_month() -> (i32, u32) {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let (year, month, _) = civil_from_unix(secs);
    (year, month)
}

fn fmt_age(age: Duration) -> String {
    let secs = age.as_secs();
    match secs {
        0..=59 => "moments".into(),
        60..=3599 => format!("{}m", secs / 60),
        3600..=86399 => format!("{}h", secs / 3600),
        _ => format!("{}d", secs / 86400),
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
    // Flat by design: kill egui's default drop shadows so dropdown menus and
    // tooltips don't cast a soft "box shadow" over the canvas.
    visuals.window_shadow = egui::Shadow::NONE;
    visuals.popup_shadow = egui::Shadow::NONE;
    ctx.set_visuals(visuals);
}
