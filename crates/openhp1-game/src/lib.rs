pub mod app;

pub use app::GameApp;

#[cfg(target_os = "android")]
use anyhow::Result;
#[cfg(target_os = "android")]
use openhp1_package::resolve_game_installation;
#[cfg(target_os = "android")]
use openhp1_scene::LoadedScene;
#[cfg(target_os = "android")]
use winit::event_loop::EventLoop;
#[cfg(target_os = "android")]
use winit::platform::android::EventLoopBuilderExtAndroid;
#[cfg(target_os = "android")]
use winit::platform::android::activity::AndroidApp;

#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
fn android_main(app: AndroidApp) {
    let _ = run_android(app);
}

#[cfg(target_os = "android")]
fn run_android(app: AndroidApp) -> Result<()> {
    let _ = app::logging::init_logging();
    tracing::info!("starting OpenHP1 Android port");

    let event_loop = EventLoop::builder().with_android_app(app).build()?;

    match resolve_game_installation() {
        Ok(installation) => {
            let startup_level = installation.startup_map().to_path_buf();
            tracing::info!(level = %startup_level.display(), "loading Android startup map");

            match LoadedScene::load(startup_level) {
                Ok(scene) => {
                    tracing::info!("scene successfully loaded, launching GameApp");
                    event_loop.run_app(&mut GameApp::new(scene, None))?;
                    Ok(())
                }
                Err(err) => {
                    tracing::error!("failed to load startup map scene: {err:#}");
                    let mut launcher = app::android_launcher::AndroidLauncherApp::new(Some(format!("Ошибка загрузки карты: {err:#}")));
                    event_loop.run_app(&mut launcher)?;
                    Ok(())
                }
            }
        }
        Err(err) => {
            tracing::warn!("game installation not found: {err:#}");
            let mut launcher = app::android_launcher::AndroidLauncherApp::new(Some(err.to_string()));
            event_loop.run_app(&mut launcher)?;
            Ok(())
        }
    }
}
