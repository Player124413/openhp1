pub mod app;

pub use app::GameApp;

#[cfg(target_os = "android")]
use std::{
    fs::{self, File},
    path::PathBuf,
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};

#[cfg(target_os = "android")]
use anyhow::{Context, Result};
#[cfg(target_os = "android")]
use openhp1_package::{resolve_game_installation, settings_dir};
#[cfg(target_os = "android")]
use openhp1_scene::LoadedScene;
#[cfg(target_os = "android")]
use tracing::info;
#[cfg(target_os = "android")]
use tracing_subscriber::{EnvFilter, Layer, layer::SubscriberExt, util::SubscriberInitExt};
#[cfg(target_os = "android")]
use winit::event_loop::EventLoop;
#[cfg(target_os = "android")]
use winit::platform::android::EventLoopBuilderExtAndroid;
#[cfg(target_os = "android")]
use winit::platform::android::activity::AndroidApp;

#[cfg(target_os = "android")]
#[no_mangle]
fn android_main(app: AndroidApp) {
    let _ = run_android(app);
}

#[cfg(target_os = "android")]
fn run_android(app: AndroidApp) -> Result<()> {
    let log_path = init_android_logging()?;
    info!(path = %log_path.display(), "starting OpenHP1 Android port");

    let installation = resolve_game_installation()
        .context("could not locate Harry Potter game files in standard Android paths (/sdcard/OpenHP1, /sdcard/Android/data/org.openhp1.game/files)")?;

    let startup_level = installation.startup_map().to_path_buf();
    info!(level = %startup_level.display(), "loading Android startup map");

    let scene = LoadedScene::load(startup_level)?;
    let event_loop = EventLoop::builder().with_android_app(app).build()?;
    event_loop.run_app(&mut GameApp::new(scene, None))?;
    Ok(())
}

#[cfg(target_os = "android")]
fn init_android_logging() -> Result<PathBuf> {
    let directory = settings_dir().join("Logs");
    let _ = fs::create_dir_all(&directory);
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let path = directory.join(format!("openhp1-android-{timestamp}.log"));
    if let Ok(file) = File::create(&path) {
        let _ = tracing_subscriber::registry()
            .with(tracing_subscriber::fmt::layer().with_filter(EnvFilter::from_default_env()))
            .with(
                tracing_subscriber::fmt::layer()
                    .with_ansi(false)
                    .with_writer(Mutex::new(file))
                    .with_filter(EnvFilter::new("info,symphonia_bundle_mp3=off")),
            )
            .try_init();
    }
    Ok(path)
}
