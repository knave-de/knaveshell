use std::process::ExitCode;

use knave_renderer::WgpuRenderer;
use knave_ui::UiScene;
use knave_wayland::ShellRole;

fn usage() -> &'static str {
    "usage: knave-shell [--check] [bar|overview]"
}

fn check(role: ShellRole) {
    if role == ShellRole::Overview {
        knave_shell::overview::Overview::new()
            .check([1920.0, 1080.0])
            .expect("valid overview composition");
        println!("knave-shell role=overview retained scene valid");
        return;
    }
    let scene = match role {
        ShellRole::Bar => UiScene::bar(1, 1920.0, 36.0),
        ShellRole::Overview => UiScene::overview(1, 1920.0, 1080.0),
    };
    let renderer = WgpuRenderer::new();
    let render_list = renderer.prepare(&scene);
    println!(
        "knave-shell role={} revision={} commands={}",
        match role {
            ShellRole::Bar => "bar",
            ShellRole::Overview => "overview",
        },
        render_list.revision,
        render_list.commands.len()
    );
}

fn run() -> Result<(), String> {
    let mut check_only = false;
    let mut role = ShellRole::Bar;

    for argument in std::env::args().skip(1) {
        match argument.as_str() {
            "--check" => check_only = true,
            "--help" | "-h" => {
                println!("{}", usage());
                return Ok(());
            }
            value => role = ShellRole::parse(value).ok_or_else(|| usage().to_string())?,
        }
    }

    if check_only {
        check(role);
        return Ok(());
    }

    if role == ShellRole::Overview {
        knave_wayland::run_application(
            knave_wayland::SurfaceOptions {
                keyboard: knave_wayland::KeyboardMode::Exclusive,
                layer: knave_wayland::SurfaceLayer::Overlay,
                namespace: "knave-shell-overview".into(),
                ..Default::default()
            },
            knave_shell::overview::Overview::new(),
        )
        .map_err(|error| error.to_string())
    } else {
        knave_wayland::run(role).map_err(|error| error.to_string())
    }
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("knave-shell: {error}\n\n{}", usage());
            ExitCode::FAILURE
        }
    }
}
