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

    let mut viewport = eframe::egui::ViewportBuilder::default()
        .with_title("CloudViz")
        .with_inner_size([1440.0, 900.0])
        .with_min_inner_size([960.0, 600.0]);

    // Under WSLg the app runs as a Wayland client, and winit draws its own
    // client-side decorations (border + drop shadow) on top of the native
    // Windows title bar the compositor already provides. WSLg doesn't update
    // that winit-drawn frame when the window is maximized, so the previous
    // window's outline is left painted over the canvas. Dropping the redundant
    // client-side decorations removes the artifact; Windows keeps drawing the
    // real title bar for move / minimize / maximize / close.
    if std::env::var_os("WSL_DISTRO_NAME").is_some() {
        viewport = viewport.with_decorations(false);
    }

    let options = eframe::NativeOptions {
        viewport,
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
