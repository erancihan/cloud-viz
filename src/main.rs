#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod export;
mod geom;
mod layout;
mod model;
mod providers;
mod theme;
mod ui;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();

    // Headless mode: `cloudviz --export-svg out.svg [--provider azure|demo]
    // [--scope <id>] [--light]` renders the topology to SVG without a window.
    if let Some(pos) = args.iter().position(|a| a == "--export-svg") {
        let path = args
            .get(pos + 1)
            .filter(|p| !p.starts_with("--"))
            .ok_or("--export-svg requires an output path")?;
        let provider_id = flag_value(&args, "--provider").unwrap_or("demo");
        let scope = flag_value(&args, "--scope");
        let theme = if args.iter().any(|a| a == "--light") {
            theme::LIGHT
        } else {
            theme::DARK
        };

        let providers = providers::builtin_providers();
        let provider = providers
            .iter()
            .find(|p| p.info().id == provider_id)
            .ok_or_else(|| format!("unknown provider: {provider_id}"))?;
        let topology = provider.fetch_topology(scope)?;
        std::fs::write(path, export::to_svg(&topology, &theme))?;
        eprintln!(
            "wrote {} ({} nodes, {} edges)",
            path,
            topology.nodes.len(),
            topology.edges.len()
        );
        return Ok(());
    }

    // Under WSLg, prefer the X11 (XWayland) backend. WSLg gives X11 windows a
    // native Windows title bar and maximizes them correctly. The Wayland-native
    // path instead makes winit draw its own client-side decorations, and WSLg
    // fails to update that border/shadow when the window is maximized, leaving a
    // stale outline painted over the canvas. winit picks Wayland whenever
    // WAYLAND_DISPLAY is set, so hide it to fall back to X11 — but only when an
    // X display is actually available, so we never strand the app with no
    // usable backend.
    if std::env::var_os("WSL_DISTRO_NAME").is_some()
        && std::env::var_os("DISPLAY").is_some_and(|v| !v.is_empty())
    {
        std::env::remove_var("WAYLAND_DISPLAY");
        std::env::remove_var("WAYLAND_SOCKET");
    }

    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("CloudViz")
            .with_inner_size([1440.0, 900.0])
            .with_min_inner_size([960.0, 600.0]),
        ..Default::default()
    };
    eframe::run_native(
        "CloudViz",
        options,
        Box::new(|cc| Ok(Box::new(app::CloudVizApp::new(cc)))),
    )
    .map_err(|e| format!("failed to start UI: {e}").into())
}

fn flag_value<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1))
        .map(String::as_str)
}
