use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Result;
use egui::{Align2, Color32, CornerRadius, RichText, ScrollArea, Stroke, Vec2};
use openhp1_package::install_from_zip;
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, Size};
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow};
use winit::window::{Window, WindowAttributes, WindowId};

use super::logging::{copy_logs, get_all_logs};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AppLanguage {
    Russian,
    English,
}

fn detect_language() -> AppLanguage {
    for key in ["LC_ALL", "LC_MESSAGES", "LANG", "LANGUAGE"] {
        if let Ok(val) = std::env::var(key) {
            let lower = val.to_ascii_lowercase();
            if lower.contains("ru") || lower.contains("rus") {
                return AppLanguage::Russian;
            }
        }
    }
    #[cfg(target_os = "android")]
    {
        if let Ok(output) = std::process::Command::new("getprop").arg("persist.sys.locale").output() {
            let s = String::from_utf8_lossy(&output.stdout).to_ascii_lowercase();
            if s.contains("ru") {
                return AppLanguage::Russian;
            }
        }
    }
    AppLanguage::English
}

pub struct AndroidLauncherApp {
    initial_error: Option<String>,
    lang: AppLanguage,
    show_about: bool,
    show_logs: bool,
    toast: Option<(String, Instant)>,
    state: Option<LauncherState>,
}

struct LauncherState {
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    egui_ctx: egui::Context,
    egui_state: egui_winit::State,
    egui_renderer: egui_wgpu::Renderer,
}

impl AndroidLauncherApp {
    pub fn new(initial_error: Option<String>) -> Self {
        Self {
            initial_error,
            lang: detect_language(),
            show_about: false,
            show_logs: false,
            toast: None,
            state: None,
        }
    }

    fn try_auto_unpack() -> Result<PathBuf> {
        let search_dirs = [
            PathBuf::from("/sdcard/Download"),
            PathBuf::from("/sdcard"),
            PathBuf::from("/storage/emulated/0/Download"),
            PathBuf::from("/storage/emulated/0"),
            PathBuf::from("/sdcard/Android/data/org.openhp1.game/files"),
        ];

        let target_dest = PathBuf::from("/sdcard/Android/data/org.openhp1.game/files");
        let _ = std::fs::create_dir_all(&target_dest);

        for dir in &search_dirs {
            if let Ok(entries) = std::fs::read_dir(dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_file() {
                        if let Some(ext) = path.extension() {
                            if ext.eq_ignore_ascii_case("zip") {
                                let inst = install_from_zip(&path, Some(&target_dest))?;
                                return Ok(inst.root().to_path_buf());
                            }
                        }
                    }
                }
            }
        }

        anyhow::bail!("ZIP-архивы не найдены в папках /sdcard/Download или /sdcard")
    }
}

impl ApplicationHandler for AndroidLauncherApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.state.is_some() {
            return;
        }

        let attributes = WindowAttributes::default()
            .with_title("OpenHP1 Launcher")
            .with_inner_size(Size::Logical(LogicalSize::new(1280.0, 720.0)));

        let window = match event_loop.create_window(attributes) {
            Ok(w) => Arc::new(w),
            Err(e) => {
                eprintln!("failed to create launcher window: {e}");
                return;
            }
        };

        let instance = wgpu::Instance::default();
        let surface = match instance.create_surface(Arc::clone(&window)) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("failed to create wgpu surface: {e}");
                return;
            }
        };

        let adapter = match pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            compatible_surface: Some(&surface),
            ..Default::default()
        })) {
            Ok(a) => a,
            Err(e) => {
                eprintln!("failed to find compatible graphics adapter: {e}");
                return;
            }
        };

        let (device, queue) = match pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("OpenHP1 launcher device"),
            ..Default::default()
        })) {
            Ok(pair) => pair,
            Err(e) => {
                eprintln!("failed to create graphics device: {e}");
                return;
            }
        };

        let size = window.inner_size();
        let mut config = match surface.get_default_config(&adapter, size.width.max(1), size.height.max(1)) {
            Some(c) => c,
            None => {
                eprintln!("failed to get surface configuration");
                return;
            }
        };
        config.present_mode = wgpu::PresentMode::AutoNoVsync;
        surface.configure(&device, &config);

        let egui_ctx = egui::Context::default();
        let egui_state = egui_winit::State::new(
            egui_ctx.clone(),
            egui::ViewportId::ROOT,
            window.as_ref(),
            Some(window.scale_factor() as f32),
            window.theme(),
            Some(device.limits().max_texture_dimension_2d as usize),
        );
        let egui_renderer = egui_wgpu::Renderer::new(&device, config.format, Default::default());

        self.state = Some(LauncherState {
            window,
            surface,
            device,
            queue,
            config,
            egui_ctx,
            egui_state,
            egui_renderer,
        });
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, window_id: WindowId, event: WindowEvent) {
        let Some(state) = &mut self.state else {
            return;
        };

        if state.window.id() != window_id {
            return;
        }

        let _ = state.egui_state.on_window_event(&state.window, &event);

        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(new_size) => {
                if new_size.width > 0 && new_size.height > 0 {
                    state.config.width = new_size.width;
                    state.config.height = new_size.height;
                    state.surface.configure(&state.device, &state.config);
                }
            }
            WindowEvent::RedrawRequested => {
                let raw_input = state.egui_state.take_egui_input(&state.window);
                let egui_ctx = state.egui_ctx.clone();

                let mut action_copy_logs = false;
                let mut action_unpack_zip = false;
                let mut action_exit = false;

                let egui_output = egui_ctx.run_ui(raw_input, |ui| {
                    egui::Frame::NONE
                        .fill(Color32::from_rgb(8, 10, 18))
                        .show(ui, |ui| {
                            ui.vertical_centered(|ui| {
                                ui.add_space(16.0);
                                ui.heading(
                                    RichText::new("✨ OpenHP1 Android ✨")
                                        .color(Color32::from_rgb(255, 215, 0))
                                        .size(26.0)
                                        .strong(),
                                );
                                ui.add_space(4.0);
                                ui.label(
                                    RichText::new("Harry Potter and the Philosopher's Stone")
                                        .color(Color32::from_rgb(180, 200, 240))
                                        .size(16.0),
                                );
                                ui.add_space(14.0);

                                // Warning & instructions panel
                                let panel_frame = egui::Frame::window(ui.style())
                                    .fill(Color32::from_rgb(24, 28, 44))
                                    .stroke(Stroke::new(1.5, Color32::from_rgb(220, 160, 60)))
                                    .corner_radius(CornerRadius::same(10));

                                panel_frame.show(ui, |ui| {
                                    ui.set_max_width(620.0);
                                    ui.vertical(|ui| {
                                        let (title, desc) = match self.lang {
                                            AppLanguage::Russian => (
                                                "⚠️ Файлы игры не найдены",
                                                "Для запуска игры скопируйте оригинальные папки ПК-версии игры:\n• Maps, System, Textures, Sounds\nв любую из следующих папок на телефоне:\n\n1) /sdcard/Android/data/org.openhp1.game/files/\n2) /sdcard/OpenHP1/\n\nЛибо поместите ZIP-архив с игрой в /sdcard/Download и нажмите кнопку «Распаковать ZIP».",
                                            ),
                                            AppLanguage::English => (
                                                "⚠️ Game files not found",
                                                "To play, please copy original PC game folders:\n• Maps, System, Textures, Sounds\ninto either folder on your phone:\n\n1) /sdcard/Android/data/org.openhp1.game/files/\n2) /sdcard/OpenHP1/\n\nOr place a game ZIP archive in /sdcard/Download and tap «Unpack ZIP».",
                                            ),
                                        };
                                        ui.heading(RichText::new(title).color(Color32::from_rgb(255, 180, 60)).size(17.0));
                                        ui.add_space(6.0);
                                        ui.label(RichText::new(desc).color(Color32::from_rgb(220, 225, 240)).size(14.0));

                                        if let Some(err) = &self.initial_error {
                                            ui.add_space(8.0);
                                            ui.label(RichText::new(format!("Детали: {err}")).color(Color32::from_rgb(240, 120, 100)).size(12.0));
                                        }
                                    });
                                });

                                ui.add_space(14.0);

                                // Action Buttons Row 1
                                ui.horizontal(|ui| {
                                    ui.add_space(((ui.available_width() - 540.0) / 2.0).max(0.0));
                                    let btn_size = Vec2::new(165.0, 42.0);

                                    let copy_btn_text = match self.lang {
                                        AppLanguage::Russian => "📋 Скопировать логи",
                                        AppLanguage::English => "📋 Copy Logs",
                                    };
                                    if ui.add(egui::Button::new(RichText::new(copy_btn_text).size(14.0).strong()).min_size(btn_size)).clicked() {
                                        action_copy_logs = true;
                                    }

                                    let unpack_btn_text = match self.lang {
                                        AppLanguage::Russian => "📦 Распаковать ZIP",
                                        AppLanguage::English => "📦 Unpack ZIP",
                                    };
                                    if ui.add(egui::Button::new(RichText::new(unpack_btn_text).size(14.0)).min_size(btn_size)).clicked() {
                                        action_unpack_zip = true;
                                    }

                                    let logs_btn_text = match self.lang {
                                        AppLanguage::Russian => if self.show_logs { "📜 Скрыть логи" } else { "📜 Показать логи" },
                                        AppLanguage::English => if self.show_logs { "📜 Hide Logs" } else { "📜 Show Logs" },
                                    };
                                    if ui.add(egui::Button::new(RichText::new(logs_btn_text).size(14.0)).min_size(btn_size)).clicked() {
                                        self.show_logs = !self.show_logs;
                                    }
                                });

                                ui.add_space(10.0);

                                // Row 2: About, Language switch, Exit
                                ui.horizontal(|ui| {
                                    ui.add_space(((ui.available_width() - 540.0) / 2.0).max(0.0));
                                    let btn_size = Vec2::new(165.0, 36.0);

                                    let about_text = match self.lang {
                                        AppLanguage::Russian => "ℹ️ О порте",
                                        AppLanguage::English => "ℹ️ About Port",
                                    };
                                    if ui.add(egui::Button::new(RichText::new(about_text).size(13.0)).min_size(btn_size)).clicked() {
                                        self.show_about = true;
                                    }

                                    let lang_toggle = match self.lang {
                                        AppLanguage::Russian => "🌐 Язык: RU",
                                        AppLanguage::English => "🌐 Lang: EN",
                                    };
                                    if ui.add(egui::Button::new(RichText::new(lang_toggle).size(13.0)).min_size(btn_size)).clicked() {
                                        self.lang = match self.lang {
                                            AppLanguage::Russian => AppLanguage::English,
                                            AppLanguage::English => AppLanguage::Russian,
                                        };
                                    }

                                    let exit_text = match self.lang {
                                        AppLanguage::Russian => "🚪 Выход",
                                        AppLanguage::English => "🚪 Exit",
                                    };
                                    if ui.add(egui::Button::new(RichText::new(exit_text).size(13.0)).min_size(btn_size)).clicked() {
                                        action_exit = true;
                                    }
                                });

                                // Toast notification
                                if let Some((msg, time)) = &self.toast {
                                    if time.elapsed() < Duration::from_secs(5) {
                                        ui.add_space(10.0);
                                        ui.label(RichText::new(msg).color(Color32::from_rgb(100, 240, 140)).size(14.0).strong());
                                    }
                                }

                                // Logs scroll area
                                if self.show_logs {
                                    ui.add_space(12.0);
                                    let logs_frame = egui::Frame::window(ui.style())
                                        .fill(Color32::from_rgb(12, 14, 22))
                                        .stroke(Stroke::new(1.0, Color32::from_rgb(80, 100, 140)))
                                        .corner_radius(CornerRadius::same(6));

                                    logs_frame.show(ui, |ui| {
                                        ui.set_max_width(620.0);
                                        ScrollArea::vertical().max_height(200.0).show(ui, |ui| {
                                            ui.label(RichText::new(get_all_logs()).monospace().size(11.0).color(Color32::from_rgb(200, 210, 230)));
                                        });
                                    });
                                }
                            });
                        });

                    // About Modal
                    if self.show_about {
                        egui::Window::new(if self.lang == AppLanguage::Russian { "О порте OpenHP1" } else { "About OpenHP1 Port" })
                            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
                            .resizable(false)
                            .collapsible(false)
                            .frame(
                                egui::Frame::window(ui.style())
                                    .fill(Color32::from_rgb(18, 22, 34))
                                    .stroke(Stroke::new(2.0, Color32::from_rgb(220, 185, 90)))
                                    .corner_radius(CornerRadius::same(12)),
                            )
                            .show(ui.ctx(), |ui| {
                                ui.set_max_width(450.0);
                                ui.vertical_centered(|ui| {
                                    ui.add_space(8.0);
                                    ui.heading(RichText::new("OpenHP1 Android Port").color(Color32::from_rgb(255, 215, 0)).strong());
                                    ui.add_space(10.0);

                                    ui.label(RichText::new(if self.lang == AppLanguage::Russian { "Порт создан Player1444:" } else { "Port made by Player1444:" }).strong());
                                    ui.hyperlink_to(
                                        RichText::new("https://t.me/player1444ports").color(Color32::from_rgb(100, 180, 255)),
                                        "https://t.me/player1444ports",
                                    );

                                    ui.add_space(12.0);
                                    ui.separator();
                                    ui.add_space(8.0);

                                    ui.label(if self.lang == AppLanguage::Russian { "Огромное спасибо этому репозиторию:" } else { "Huge thanks to this repository:" });
                                    ui.hyperlink_to(
                                        RichText::new("https://github.com/SplittyDev/openhp1").color(Color32::from_rgb(140, 200, 255)),
                                        "https://github.com/SplittyDev/openhp1",
                                    );
                                    ui.label(RichText::new(if self.lang == AppLanguage::Russian { "Без него этот порт бы не вышел!" } else { "Without it, this port would not be possible!" }).italics().size(13.0));

                                    ui.add_space(16.0);
                                    if ui.button(if self.lang == AppLanguage::Russian { "Закрыть" } else { "Close" }).clicked() {
                                        self.show_about = false;
                                    }
                                    ui.add_space(4.0);
                                });
                            });
                    }
                });

                if action_copy_logs {
                    let (msg, _) = copy_logs(Some(&state.egui_ctx));
                    self.toast = Some((msg, Instant::now()));
                }

                if action_unpack_zip {
                    match Self::try_auto_unpack() {
                        Ok(root) => {
                            let msg = match self.lang {
                                AppLanguage::Russian => format!("Игра успешно распакована в:\n{}", root.display()),
                                AppLanguage::English => format!("Game unpacked successfully into:\n{}", root.display()),
                            };
                            self.toast = Some((msg, Instant::now()));
                        }
                        Err(e) => {
                            let msg = match self.lang {
                                AppLanguage::Russian => format!("Ошибка распаковки: {e}"),
                                AppLanguage::English => format!("Unpack failed: {e}"),
                            };
                            self.toast = Some((msg, Instant::now()));
                        }
                    }
                }

                if action_exit {
                    event_loop.exit();
                }

                state.egui_state.handle_platform_output(&state.window, egui_output.platform_output);

                for (id, delta) in &egui_output.textures_delta.set {
                    state.egui_renderer.update_texture(&state.device, &state.queue, *id, delta);
                }

                let paint_jobs = state.egui_ctx.tessellate(egui_output.shapes, egui_output.pixels_per_point);
                let screen = egui_wgpu::ScreenDescriptor {
                    size_in_pixels: [state.config.width, state.config.height],
                    pixels_per_point: egui_output.pixels_per_point,
                };

                let mut encoder = state.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("OpenHP1 launcher frame"),
                });

                let mut commands = state.egui_renderer.update_buffers(
                    &state.device,
                    &state.queue,
                    &mut encoder,
                    &paint_jobs,
                    &screen,
                );

                let frame = match state.surface.get_current_texture() {
                    wgpu::CurrentSurfaceTexture::Success(frame) => frame,
                    wgpu::CurrentSurfaceTexture::Suboptimal(frame) => {
                        state.surface.configure(&state.device, &state.config);
                        frame
                    }
                    _ => {
                        state.surface.configure(&state.device, &state.config);
                        state.window.request_redraw();
                        return;
                    }
                };

                let view = frame.texture.create_view(&Default::default());

                {
                    let pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("OpenHP1 launcher pass"),
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view: &view,
                            resolve_target: None,
                            depth_slice: None,
                            ops: wgpu::Operations {
                                load: wgpu::LoadOp::Clear(wgpu::Color {
                                    r: 0.03,
                                    g: 0.04,
                                    b: 0.07,
                                    a: 1.0,
                                }),
                                store: wgpu::StoreOp::Store,
                            },
                        })],
                        depth_stencil_attachment: None,
                        timestamp_writes: None,
                        occlusion_query_set: None,
                        multiview_mask: None,
                    });
                    state.egui_renderer.render(&mut pass.forget_lifetime(), &paint_jobs, &screen);
                }

                commands.push(encoder.finish());
                state.queue.submit(commands);
                frame.present();

                for id in &egui_output.textures_delta.free {
                    state.egui_renderer.free_texture(id);
                }

                state.window.request_redraw();
                event_loop.set_control_flow(ControlFlow::WaitUntil(Instant::now() + Duration::from_millis(33)));
            }
            _ => {}
        }
    }
}
