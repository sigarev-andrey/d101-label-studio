#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]
use niimbot_d101::{i18n::tr, local_format};
mod app_icon;
mod editor_geometry;
mod fluent_ui;
mod generator_ui;
mod preview;
mod preview_scene;
mod studio_style;
use anyhow::{Context, Result};
use niimbot_d101::{engine::Engine, ipp};
use std::path::PathBuf;

fn main() {
    if let Err(e) = run() {
        eprintln!("{e:#}");
        if !std::env::args().any(|s| s == "--headless") {
            rfd::MessageDialog::new()
                .set_title("D101 Label Studio")
                .set_description(format!("{e:#}"))
                .set_level(rfd::MessageLevel::Error)
                .show();
        }
        std::process::exit(1);
    }
}
fn run() -> Result<()> {
    let mut simulated = false;
    let mut headless = false;
    let mut directory = None;
    let mut port = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--simulate" => simulated = true,
            "--headless" => headless = true,
            "--data-dir" => {
                directory = Some(PathBuf::from(
                    args.next().context(tr("--data-dir требует путь"))?,
                ))
            }
            "--port" => {
                port = Some(
                    args.next()
                        .context(tr("--port требует номер"))?
                        .parse::<u16>()?,
                )
            }
            "--help" | "-h" => {
                println!("niimbot-d101 [--headless] [--simulate] [--port 8631] [--data-dir PATH]");
                return Ok(());
            }
            _ => anyhow::bail!(local_format!(
                "Неизвестный параметр: {arg}",
                "Unknown option: {arg}"
            )),
        }
    }
    let directory = directory.unwrap_or_else(|| {
        PathBuf::from(std::env::var_os("LOCALAPPDATA").unwrap_or_else(|| ".".into())).join(
            if simulated {
                "NiimbotD101-Simulation"
            } else {
                "NiimbotD101"
            },
        )
    });
    let engine = Engine::open(
        directory,
        simulated,
        port.unwrap_or(if simulated { 18631 } else { 8631 }),
    )?;
    let runtime = tokio::runtime::Runtime::new()?;
    // Bind before starting the worker: a second instance must never send duplicate jobs.
    let listener = runtime
        .block_on(tokio::net::TcpListener::bind((
            std::net::Ipv4Addr::LOCALHOST,
            engine.port,
        )))
        .context(tr(
            "IPP-порт занят. Закройте другой экземпляр D101 Label Studio",
        ))?;
    let service = engine.clone();
    let server = runtime.spawn(async move {
        if let Err(e) = ipp::serve_listener(service.clone(), listener).await {
            service.set_server_status(format!("IPP: {e:#}"));
        }
    });
    let mut worker = runtime.spawn(engine.clone().worker());
    let monitor = runtime.spawn(engine.clone().monitor_printer());
    let result = if headless {
        runtime
            .block_on(async { tokio::signal::ctrl_c().await })
            .context(tr("Ожидание завершения"))
    } else {
        let handle = runtime.handle().clone();
        fluent_ui::run(engine.clone(), handle).map_err(|e| {
            anyhow::anyhow!(local_format!(
                "Ошибка интерфейса: {e}",
                "User interface error: {e}"
            ))
        })
    };
    engine.begin_shutdown();
    server.abort();
    runtime.block_on(async {
        monitor.abort();
        let _ = monitor.await;
        // Let an active batch send its cancel command before releasing BLE.
        if tokio::time::timeout(std::time::Duration::from_secs(8), &mut worker)
            .await
            .is_err()
        {
            worker.abort();
            let _ = worker.await;
        }
        engine.disconnect_printer().await;
    });
    runtime.shutdown_timeout(std::time::Duration::from_secs(3));
    result
}
