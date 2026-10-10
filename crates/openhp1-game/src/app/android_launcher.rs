use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use egui::{Align2, Color32, CornerRadius, RichText, ScrollArea, Stroke, Vec2};
use openhp1_package::{
    configure_game_installation, read_openhp1_ini_value, save_openhp1_ini_values,
};
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, Size};
use winit::event::WindowEvent;
use winit::event_loop::ActiveEventLoop;
use winit::window::{Window, WindowAttributes, WindowId};

use super::logging::copy_logs;

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
        if let Ok(output) = std::process::Command::new("getprop")
            .arg("persist.sys.locale")
            .output()
        {
            let s = String::from_utf8_lossy(&output.stdout).to_ascii_lowercase();
            if s.contains("ru") {
                return AppLanguage::Russian;
            }
        }
    }
    AppLanguage::English
}

fn default_target_dir() -> PathBuf {
    PathBuf::from("/sdcard/Android/data/org.openhp1.game/files")
}

fn check_game_folder(path: &Path) -> bool {
    let has_maps = path.join("Maps").is_dir() || path.join("maps").is_dir();
    let has_system = path.join("System").is_dir() || path.join("system").is_dir();
    has_maps && has_system
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let ft = entry.file_type()?;
        let dest_child = dst.join(entry.file_name());
        if ft.is_dir() {
            copy_dir_recursive(&entry.path(), &dest_child)?;
        } else {
            let _ = std::fs::copy(entry.path(), dest_child);
        }
    }
    Ok(())
}

fn transfer_game_files(from_dir: &Path, target_dir: &Path) -> Result<PathBuf, String> {
    let source_root = openhp1_package::find_game_root(from_dir)
        .or_else(|| {
            if check_game_folder(from_dir) {
                Some(from_dir.to_path_buf())
            } else {
                None
            }
        })
        .ok_or_else(|| {
            "В выбранной папке не найдены файлы игры (Maps, System)".to_string()
        })?;

    std::fs::create_dir_all(target_dir)
        .map_err(|e| format!("Не удалось создать папку назначения: {e}"))?;

    let subfolders = [
        "Maps",
        "System",
        "Textures",
        "Sounds",
        "Music",
        "StaticMeshes",
        "Classes",
        "Save",
    ];
    for name in &subfolders {
        if let Ok(entries) = std::fs::read_dir(&source_root) {
            for entry in entries.flatten() {
                let filename = entry.file_name().to_string_lossy().to_string();
                if filename.eq_ignore_ascii_case(name) && entry.path().is_dir() {
                    let dest = target_dir.join(&filename);
                    let _ = copy_dir_recursive(&entry.path(), &dest);
                }
            }
        }
    }

    if let Ok(entries) = std::fs::read_dir(&source_root) {
        for entry in entries.flatten() {
            if entry.path().is_file() {
                let dest = target_dir.join(entry.file_name());
                let _ = std::fs::copy(entry.path(), dest);
            }
        }
    }

    Ok(target_dir.to_path_buf())
}

fn unpack_and_transfer_zip(zip_file: &Path, target_dir: &Path) -> Result<PathBuf, String> {
    let temp_unpack = target_dir.join(".unpack_temp");
    let _ = std::fs::remove_dir_all(&temp_unpack);
    let _ = std::fs::create_dir_all(&temp_unpack);

    let unpacked_root = openhp1_package::unpack_zip_archive(zip_file, &temp_unpack)
        .map_err(|e| format!("Ошибка распаковки архива: {e}"))?;

    let res = transfer_game_files(&unpacked_root, target_dir);
    let _ = std::fs::remove_dir_all(&temp_unpack);
    res
}

pub struct AndroidLauncherApp {
    initial_error: Option<String>,
    lang: AppLanguage,
    show_about: bool,
    show_settings: bool,
    show_file_manager: bool,
    manager_mode_zip: bool,
    current_browse_path: PathBuf,
    selected_path: Option<PathBuf>,
    custom_input_path: String,
    toast: Option<(String, Instant)>,
    state: Option<LauncherState>,

    // Settings
    setting_classic_renderer: bool,
    setting_bilinear_filtering: bool,
    setting_etc2: bool,
    setting_cutscene_skip: bool,
    setting_touch_enabled: bool,
    setting_touch_opacity: f32,
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
        let initial_browse = if Path::new("/sdcard/Download").is_dir() {
            PathBuf::from("/sdcard/Download")
        } else if Path::new("/sdcard").is_dir() {
            PathBuf::from("/sdcard")
        } else {
            PathBuf::from("/storage/emulated/0")
        };

        let mode_str = read_openhp1_ini_value("OpenHP1.Renderer", "Mode").unwrap_or_default();
        let filter_str =
            read_openhp1_ini_value("OpenHP1.Renderer", "FilterMode").unwrap_or_default();
        let etc2_str =
            read_openhp1_ini_value("OpenHP1.Renderer", "Etc2Compression").unwrap_or_default();
        let cutscene_str =
            read_openhp1_ini_value("OpenHP1.Gameplay", "JumpSkipsCutscenes").unwrap_or_default();
        let touch_str = read_openhp1_ini_value("OpenHP1.Touch", "Enabled").unwrap_or_default();
        let opacity_str = read_openhp1_ini_value("OpenHP1.Touch", "Opacity").unwrap_or_default();

        Self {
            initial_error,
            lang: detect_language(),
            show_about: false,
            show_settings: false,
            show_file_manager: false,
            manager_mode_zip: false,
            current_browse_path: initial_browse,
            selected_path: None,
            custom_input_path: String::new(),
            toast: None,
            state: None,

            setting_classic_renderer: mode_str.eq_ignore_ascii_case("classic")
                || mode_str.is_empty(),
            setting_bilinear_filtering: !filter_str.eq_ignore_ascii_case("nearest"),
            setting_etc2: !etc2_str.eq_ignore_ascii_case("false"),
            setting_cutscene_skip: !cutscene_str.eq_ignore_ascii_case("false"),
            setting_touch_enabled: !touch_str.eq_ignore_ascii_case("false"),
            setting_touch_opacity: opacity_str.parse::<f32>().unwrap_or(0.75),
        }
    }

    fn save_settings(&self) -> Result<(), String> {
        let renderer_mode = if self.setting_classic_renderer {
            "classic"
        } else {
            "modern"
        };
        let filter_mode = if self.setting_bilinear_filtering {
            "linear"
        } else {
            "nearest"
        };
        let etc2 = if self.setting_etc2 { "true" } else { "false" };
        let cutscene = if self.setting_cutscene_skip {
            "true"
        } else {
            "false"
        };
        let touch = if self.setting_touch_enabled {
            "true"
        } else {
            "false"
        };
        let opacity = format!("{:.2}", self.setting_touch_opacity);

        save_openhp1_ini_values(&[
            ("OpenHP1.Renderer", "Mode", renderer_mode),
            ("OpenHP1.Renderer", "FilterMode", filter_mode),
            ("OpenHP1.Renderer", "Etc2Compression", etc2),
            ("OpenHP1.Gameplay", "JumpSkipsCutscenes", cutscene),
            ("OpenHP1.Touch", "Enabled", touch),
            ("OpenHP1.Touch", "Opacity", &opacity),
        ])
        .map_err(|e| format!("{e}"))
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

        let adapter = match pollster::block_on(instance.request_adapter(
            &wgpu::RequestAdapterOptions {
                compatible_surface: Some(&surface),
                ..Default::default()
            },
        )) {
            Ok(a) => a,
            Err(e) => {
                eprintln!("failed to find compatible graphics adapter: {e}");
                return;
            }
        };

        let (device, queue) = match pollster::block_on(adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: Some("OpenHP1 launcher device"),
                ..Default::default()
            },
        )) {
            Ok(pair) => pair,
            Err(e) => {
                eprintln!("failed to create graphics device: {e}");
                return;
            }
        };

        let size = window.inner_size();
        let mut config = match surface.get_default_config(
            &adapter,
            size.width.max(1),
            size.height.max(1),
        ) {
            Some(c) => c,
            None => {
                eprintln!("failed to get surface configuration");
                return;
            }
        };
        config.present_mode = wgpu::PresentMode::AutoVsync;
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

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
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
                let mut action_transfer_folder: Option<PathBuf> = None;
                let mut action_use_folder_direct: Option<PathBuf> = None;
                let mut action_unpack_and_transfer_zip: Option<PathBuf> = None;
                let mut action_save_settings = false;

                let egui_output = egui_ctx.run_ui(raw_input, |ui| {
                    egui::CentralPanel::default()
                        .frame(egui::Frame::NONE.fill(Color32::from_rgb(10, 13, 22)))
                        .show(ui, |ui| {
                            // Header bar
                            ui.horizontal(|ui| {
                                ui.add_space(8.0);
                                let lang_label = match self.lang {
                                    AppLanguage::Russian => "🌐 Язык: RU",
                                    AppLanguage::English => "🌐 Lang: EN",
                                };
                                if ui.button(RichText::new(lang_label).size(13.0)).clicked() {
                                    self.lang = match self.lang {
                                        AppLanguage::Russian => AppLanguage::English,
                                        AppLanguage::English => AppLanguage::Russian,
                                    };
                                }

                                ui.add_space(10.0);
                                let settings_label = match self.lang {
                                    AppLanguage::Russian => "⚙️ Настройки",
                                    AppLanguage::English => "⚙️ Settings",
                                };
                                if ui.button(RichText::new(settings_label).size(13.0).color(Color32::from_rgb(255, 215, 120))).clicked() {
                                    self.show_settings = true;
                                }

                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    ui.add_space(8.0);
                                    let copy_label = match self.lang {
                                        AppLanguage::Russian => "📋 Скопировать логи",
                                        AppLanguage::English => "📋 Copy Logs",
                                    };
                                    if ui.button(RichText::new(copy_label).size(13.0).color(Color32::from_rgb(170, 220, 255))).clicked() {
                                        action_copy_logs = true;
                                    }
                                });
                            });

                            ui.vertical_centered(|ui| {
                                ui.add_space(6.0);
                                ui.heading(
                                    RichText::new("✨ OpenHP1 Android ✨")
                                        .color(Color32::from_rgb(255, 215, 0))
                                        .size(24.0)
                                        .strong(),
                                );
                                ui.label(
                                    RichText::new("Harry Potter and the Philosopher's Stone")
                                        .color(Color32::from_rgb(200, 210, 230))
                                        .size(13.0),
                                );
                                ui.add_space(10.0);

                                // Main Error/Info Card
                                egui::Frame::group(ui.style())
                                    .fill(Color32::from_rgb(16, 20, 32))
                                    .stroke(Stroke::new(1.5, Color32::from_rgb(220, 160, 60)))
                                    .corner_radius(CornerRadius::same(10))
                                    .show(ui, |ui| {
                                        ui.set_max_width(680.0);
                                        ui.vertical(|ui| {
                                            ui.horizontal(|ui| {
                                                ui.label(
                                                    RichText::new(if self.lang == AppLanguage::Russian {
                                                        "⚠️ Файлы игры не найдены"
                                                    } else {
                                                        "⚠️ Game Files Not Found"
                                                    })
                                                    .color(Color32::from_rgb(255, 190, 80))
                                                    .size(16.0)
                                                    .strong(),
                                                );
                                            });

                                            ui.add_space(4.0);
                                            ui.label(RichText::new(if self.lang == AppLanguage::Russian {
                                                "Для запуска игры поместите папки (Maps, System, Textures, Sounds) в:\n• /sdcard/Android/data/org.openhp1.game/files/\n• /sdcard/OpenHP1/\nЛибо нажмите кнопку выбора папки или ZIP-архива ниже — игра автоматически перенесёт их в правильное место."
                                            } else {
                                                "To run the game, place game folders (Maps, System, Textures, Sounds) into:\n• /sdcard/Android/data/org.openhp1.game/files/\n• /sdcard/OpenHP1/\nOr choose a game folder or ZIP archive below — the app will transfer them automatically."
                                            }).size(13.0).color(Color32::from_rgb(225, 230, 245)));

                                            if let Some(err) = &self.initial_error {
                                                ui.add_space(4.0);
                                                ui.label(RichText::new(format!("Детали: {err}")).size(11.0).color(Color32::from_rgb(240, 120, 100)));
                                            }
                                        });
                                    });

                                ui.add_space(16.0);

                                // 3 Primary Action Buttons
                                let btn_size = Vec2::new(200.0, 48.0);
                                ui.horizontal(|ui| {
                                    ui.add_space((ui.available_width() - (btn_size.x * 3.0 + 32.0)).max(0.0) / 2.0);

                                    // Button 1: Choose game folder
                                    let folder_text = match self.lang {
                                        AppLanguage::Russian => "📁 Выбрать папку",
                                        AppLanguage::English => "📁 Choose folder",
                                    };
                                    if ui.add(egui::Button::new(RichText::new(folder_text).size(14.0).strong()).min_size(btn_size)).clicked() {
                                        self.manager_mode_zip = false;
                                        self.selected_path = None;
                                        self.show_file_manager = true;
                                    }

                                    ui.add_space(16.0);

                                    // Button 2: Choose ZIP archive
                                    let zip_text = match self.lang {
                                        AppLanguage::Russian => "📦 Выбрать ZIP-архив",
                                        AppLanguage::English => "📦 Choose ZIP",
                                    };
                                    if ui.add(egui::Button::new(RichText::new(zip_text).size(14.0).strong()).min_size(btn_size)).clicked() {
                                        self.manager_mode_zip = true;
                                        self.selected_path = None;
                                        self.show_file_manager = true;
                                    }

                                    ui.add_space(16.0);

                                    // Button 3: About port
                                    let about_text = match self.lang {
                                        AppLanguage::Russian => "ℹ️ О порте",
                                        AppLanguage::English => "ℹ️ About port",
                                    };
                                    if ui.add(egui::Button::new(RichText::new(about_text).size(14.0).strong()).min_size(btn_size)).clicked() {
                                        self.show_about = true;
                                    }
                                });

                                // Toast notification
                                if let Some((msg, time)) = &self.toast {
                                    if time.elapsed() < Duration::from_secs(7) {
                                        ui.add_space(14.0);
                                        ui.label(RichText::new(msg).color(Color32::from_rgb(100, 240, 150)).size(14.0).strong());
                                    }
                                }
                            });
                        });

                    // Modal 1: Interactive File & Folder Manager
                    if self.show_file_manager {
                        let title = if self.manager_mode_zip {
                            if self.lang == AppLanguage::Russian { "📦 Файловый менеджер (Выбор ZIP-архива)" } else { "📦 File Manager (Choose ZIP)" }
                        } else {
                            if self.lang == AppLanguage::Russian { "📁 Файловый менеджер (Выбор папки с игрой)" } else { "📁 File Manager (Choose Game Folder)" }
                        };

                        egui::Window::new(title)
                            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
                            .resizable(false)
                            .collapsible(false)
                            .frame(
                                egui::Frame::window(ui.style())
                                    .fill(Color32::from_rgb(14, 18, 28))
                                    .stroke(Stroke::new(2.0, Color32::from_rgb(100, 180, 255)))
                                    .corner_radius(CornerRadius::same(12)),
                            )
                            .show(ui.ctx(), |ui| {
                                ui.set_max_width(620.0);
                                ui.set_min_width(560.0);

                                ui.vertical(|ui| {
                                    // Quick jumps
                                    ui.horizontal_wrapped(|ui| {
                                        ui.label(RichText::new(if self.lang == AppLanguage::Russian { "Переход:" } else { "Jump:" }).size(12.0).weak());
                                        if ui.button("📥 Download").clicked() {
                                            self.current_browse_path = PathBuf::from("/sdcard/Download");
                                            self.selected_path = None;
                                        }
                                        if ui.button("📱 /sdcard").clicked() {
                                            self.current_browse_path = PathBuf::from("/sdcard");
                                            self.selected_path = None;
                                        }
                                        if ui.button("📂 OpenHP1").clicked() {
                                            self.current_browse_path = PathBuf::from("/sdcard/OpenHP1");
                                            self.selected_path = None;
                                        }
                                        if let Some(parent) = self.current_browse_path.parent() {
                                            if ui.button("⬆️ Вверх (..)").clicked() {
                                                self.current_browse_path = parent.to_path_buf();
                                                self.selected_path = None;
                                            }
                                        }
                                    });

                                    ui.add_space(4.0);
                                    ui.label(RichText::new(format!("Текущая папка: {}", self.current_browse_path.display())).monospace().size(12.0).color(Color32::from_rgb(255, 220, 100)));
                                    ui.separator();

                                    // Check if current directory itself is a game root
                                    let current_is_game = check_game_folder(&self.current_browse_path);

                                    // File list
                                    ScrollArea::vertical().max_height(240.0).show(ui, |ui| {
                                        if let Ok(entries) = std::fs::read_dir(&self.current_browse_path) {
                                            let mut sorted_entries = entries.flatten().collect::<Vec<_>>();
                                            sorted_entries.sort_by(|a, b| {
                                                let a_is_dir = a.file_type().map(|t| t.is_dir()).unwrap_or(false);
                                                let b_is_dir = b.file_type().map(|t| t.is_dir()).unwrap_or(false);
                                                b_is_dir.cmp(&a_is_dir).then_with(|| a.file_name().cmp(&b.file_name()))
                                            });

                                            for entry in sorted_entries {
                                                let path = entry.path();
                                                let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
                                                let filename = entry.file_name().to_string_lossy().to_string();

                                                if is_dir {
                                                    let is_hp_folder = check_game_folder(&path);
                                                    let is_selected = self.selected_path.as_ref() == Some(&path);

                                                    ui.horizontal(|ui| {
                                                        if ui.selectable_label(is_selected, RichText::new(format!("📁 {}", filename)).size(13.0)).clicked() {
                                                            self.selected_path = Some(path.clone());
                                                        }

                                                        if is_hp_folder {
                                                            ui.label(RichText::new("🌟 [Файлы HP1!]").color(Color32::GREEN).size(12.0).strong());
                                                        }

                                                        if ui.button(if self.lang == AppLanguage::Russian { "Открыть" } else { "Open" }).clicked() {
                                                            self.current_browse_path = path;
                                                            self.selected_path = None;
                                                        }
                                                    });
                                                } else if self.manager_mode_zip {
                                                    if let Some(ext) = path.extension() {
                                                        if ext.eq_ignore_ascii_case("zip") {
                                                            let is_selected = self.selected_path.as_ref() == Some(&path);
                                                            let size_mb = entry.metadata().map(|m| m.len() as f64 / (1024.0 * 1024.0)).unwrap_or(0.0);
                                                            if ui.selectable_label(is_selected, RichText::new(format!("📦 {} ({:.1} MB)", filename, size_mb)).size(13.0).color(Color32::from_rgb(180, 220, 255))).clicked() {
                                                                self.selected_path = Some(path.clone());
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        } else {
                                            ui.label(RichText::new(if self.lang == AppLanguage::Russian { "Не удалось прочитать папку (нет доступа или пуста)" } else { "Could not read directory" }).color(Color32::LIGHT_RED));
                                        }
                                    });

                                    ui.separator();

                                    // Action bar depending on selection
                                    if self.manager_mode_zip {
                                        if let Some(zip_path) = &self.selected_path {
                                            ui.label(RichText::new(format!("Выбран архив: {}", zip_path.file_name().and_then(|n| n.to_str()).unwrap_or(""))).color(Color32::from_rgb(100, 220, 255)).strong());
                                            ui.add_space(4.0);
                                            if ui.button(RichText::new(if self.lang == AppLanguage::Russian { "📦 Распаковать и перенести файлы игры в правильное место" } else { "📦 Unpack and transfer game files" }).color(Color32::GREEN).size(14.0).strong()).clicked() {
                                                action_unpack_and_transfer_zip = Some(zip_path.clone());
                                            }
                                        } else {
                                            ui.label(RichText::new(if self.lang == AppLanguage::Russian { "Нажмите на ZIP-архив в списке выше для выбора" } else { "Click a ZIP file above to select it" }).weak());
                                        }
                                    } else {
                                        let target_folder = self.selected_path.clone().unwrap_or_else(|| self.current_browse_path.clone());
                                        let has_game = check_game_folder(&target_folder);

                                        if has_game || current_is_game {
                                            ui.horizontal(|ui| {
                                                if ui.button(RichText::new(if self.lang == AppLanguage::Russian { "📥 Перенести файлы игры в правильное место" } else { "📥 Transfer game files to app directory" }).color(Color32::GREEN).size(13.5).strong()).clicked() {
                                                    action_transfer_folder = Some(target_folder.clone());
                                                }
                                                if ui.button(RichText::new(if self.lang == AppLanguage::Russian { "⚡ Использовать напрямую" } else { "⚡ Use folder directly" }).size(13.5)).clicked() {
                                                    action_use_folder_direct = Some(target_folder);
                                                }
                                            });
                                        } else {
                                            ui.horizontal(|ui| {
                                                ui.label(RichText::new(if self.lang == AppLanguage::Russian { "В текущей папке файлы не найдены." } else { "No game files in current folder." }).weak());
                                                if ui.button(if self.lang == AppLanguage::Russian { "Использовать всё равно" } else { "Use anyway" }).clicked() {
                                                    action_use_folder_direct = Some(target_folder);
                                                }
                                            });
                                        }
                                    }

                                    ui.add_space(8.0);
                                    ui.horizontal(|ui| {
                                        ui.label(if self.lang == AppLanguage::Russian { "Путь вручную:" } else { "Custom path:" });
                                        ui.text_edit_singleline(&mut self.custom_input_path);
                                        if ui.button(if self.lang == AppLanguage::Russian { "Перейти" } else { "Go" }).clicked() {
                                            let p = PathBuf::from(&self.custom_input_path);
                                            if p.is_dir() {
                                                self.current_browse_path = p;
                                                self.selected_path = None;
                                            }
                                        }
                                    });

                                    ui.add_space(6.0);
                                    if ui.button(if self.lang == AppLanguage::Russian { "Закрыть" } else { "Close" }).clicked() {
                                        self.show_file_manager = false;
                                    }
                                });
                            });
                    }

                    // Modal 2: Settings
                    if self.show_settings {
                        egui::Window::new(if self.lang == AppLanguage::Russian { "⚙️ Настройки OpenHP1" } else { "⚙️ OpenHP1 Settings" })
                            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
                            .resizable(false)
                            .collapsible(false)
                            .frame(
                                egui::Frame::window(ui.style())
                                    .fill(Color32::from_rgb(14, 18, 28))
                                    .stroke(Stroke::new(2.0, Color32::from_rgb(255, 215, 120)))
                                    .corner_radius(CornerRadius::same(12)),
                            )
                            .show(ui.ctx(), |ui| {
                                ui.set_max_width(480.0);
                                ui.vertical(|ui| {
                                    ui.heading(RichText::new(if self.lang == AppLanguage::Russian { "Графика и производительность" } else { "Graphics & Performance" }).size(15.0).strong());
                                    ui.add_space(6.0);

                                    // Renderer mode
                                    ui.horizontal(|ui| {
                                        ui.label(if self.lang == AppLanguage::Russian { "Режим рендеринга:" } else { "Renderer Mode:" });
                                        ui.radio_value(&mut self.setting_classic_renderer, true, if self.lang == AppLanguage::Russian { "Классический" } else { "Classic" });
                                        ui.radio_value(&mut self.setting_classic_renderer, false, if self.lang == AppLanguage::Russian { "Современный" } else { "Modern" });
                                    });

                                    // Texture filtering
                                    ui.horizontal(|ui| {
                                        ui.label(if self.lang == AppLanguage::Russian { "Фильтрация текстур:" } else { "Texture Filtering:" });
                                        ui.radio_value(&mut self.setting_bilinear_filtering, true, if self.lang == AppLanguage::Russian { "Билинейная" } else { "Bilinear" });
                                        ui.radio_value(&mut self.setting_bilinear_filtering, false, if self.lang == AppLanguage::Russian { "Без сглаживания" } else { "Nearest" });
                                    });

                                    // ETC2
                                    ui.checkbox(&mut self.setting_etc2, if self.lang == AppLanguage::Russian { "Аппаратное сжатие текстур ETC2 (рекомендуется)" } else { "Hardware ETC2 Texture Compression (recommended)" });

                                    ui.add_space(8.0);
                                    ui.separator();
                                    ui.add_space(4.0);

                                    ui.heading(RichText::new(if self.lang == AppLanguage::Russian { "Управление и геймплей" } else { "Controls & Gameplay" }).size(15.0).strong());
                                    ui.add_space(6.0);

                                    // Cutscene skip
                                    ui.checkbox(&mut self.setting_cutscene_skip, if self.lang == AppLanguage::Russian { "Пропуск катсцен по тапу или прыжку" } else { "Skip cutscenes via tap or jump" });

                                    // Touch controls toggle
                                    ui.checkbox(&mut self.setting_touch_enabled, if self.lang == AppLanguage::Russian { "Включить сенсорное управление" } else { "Enable Touch Controls" });

                                    if self.setting_touch_enabled {
                                        ui.horizontal(|ui| {
                                            ui.label(if self.lang == AppLanguage::Russian { "Прозрачность кнопок:" } else { "Button Opacity:" });
                                            ui.add(egui::Slider::new(&mut self.setting_touch_opacity, 0.2..=1.0).text(""));
                                        });
                                    }

                                    ui.add_space(12.0);
                                    ui.horizontal(|ui| {
                                        if ui.button(RichText::new(if self.lang == AppLanguage::Russian { "💾 Сохранить настройки" } else { "💾 Save Settings" }).color(Color32::GREEN).strong()).clicked() {
                                            action_save_settings = true;
                                        }
                                        if ui.button(if self.lang == AppLanguage::Russian { "Закрыть" } else { "Close" }).clicked() {
                                            self.show_settings = false;
                                        }
                                    });
                                });
                            });
                    }

                    // Modal 3: About Port
                    if self.show_about {
                        egui::Window::new(if self.lang == AppLanguage::Russian { "ℹ️ О порте OpenHP1" } else { "ℹ️ About OpenHP1 Port" })
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
                                ui.set_max_width(460.0);
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
                                });
                            });
                    }
                });

                if action_copy_logs {
                    let (text, copied) = copy_logs(Some(&state.egui_ctx));
                    let msg = if copied {
                        match self.lang {
                            AppLanguage::Russian => format!("Логи скопированы в буфер обмена! ({} символов)", text.len()),
                            AppLanguage::English => format!("Logs copied to clipboard! ({} characters)", text.len()),
                        }
                    } else {
                        match self.lang {
                            AppLanguage::Russian => format!("Логи собраны ({} символов)", text.len()),
                            AppLanguage::English => format!("Logs collected ({} characters)", text.len()),
                        }
                    };
                    self.toast = Some((msg, Instant::now()));
                }

                if action_save_settings {
                    match self.save_settings() {
                        Ok(()) => {
                            let msg = match self.lang {
                                AppLanguage::Russian => "✅ Настройки успешно сохранены!".to_string(),
                                AppLanguage::English => "✅ Settings successfully saved!".to_string(),
                            };
                            self.toast = Some((msg, Instant::now()));
                            self.show_settings = false;
                        }
                        Err(e) => {
                            let msg = match self.lang {
                                AppLanguage::Russian => format!("Ошибка сохранения: {e}"),
                                AppLanguage::English => format!("Save error: {e}"),
                            };
                            self.toast = Some((msg, Instant::now()));
                        }
                    }
                }

                // Transfer folder to default target
                if let Some(folder) = action_transfer_folder {
                    let target = default_target_dir();
                    match transfer_game_files(&folder, &target) {
                        Ok(dest) => {
                            let _ = configure_game_installation(&dest, None);
                            let msg = match self.lang {
                                AppLanguage::Russian => format!("✅ Файлы игры успешно скопированы в: {}\nПерезапустите приложение для старта!", dest.display()),
                                AppLanguage::English => format!("✅ Game files transferred to: {}\nRestart app to start!", dest.display()),
                            };
                            self.toast = Some((msg, Instant::now()));
                            self.show_file_manager = false;
                        }
                        Err(e) => {
                            let msg = match self.lang {
                                AppLanguage::Russian => format!("Ошибка переноса файлов: {e}"),
                                AppLanguage::English => format!("Transfer error: {e}"),
                            };
                            self.toast = Some((msg, Instant::now()));
                        }
                    }
                }

                // Use folder directly
                if let Some(folder) = action_use_folder_direct {
                    match configure_game_installation(&folder, None) {
                        Ok(inst) => {
                            let msg = match self.lang {
                                AppLanguage::Russian => format!("Папка установлена: {}\nПерезапустите приложение для старта!", inst.root().display()),
                                AppLanguage::English => format!("Folder configured: {}\nRestart app to start!", inst.root().display()),
                            };
                            self.toast = Some((msg, Instant::now()));
                            self.show_file_manager = false;
                        }
                        Err(e) => {
                            let msg = match self.lang {
                                AppLanguage::Russian => format!("Ошибка выбора папки: {e}"),
                                AppLanguage::English => format!("Folder error: {e}"),
                            };
                            self.toast = Some((msg, Instant::now()));
                        }
                    }
                }

                // Unpack zip and transfer to target
                if let Some(zip_file) = action_unpack_and_transfer_zip {
                    let target = default_target_dir();
                    match unpack_and_transfer_zip(&zip_file, &target) {
                        Ok(dest) => {
                            let _ = configure_game_installation(&dest, None);
                            let msg = match self.lang {
                                AppLanguage::Russian => format!("✅ Архив распакован и перенесён в: {}\nПерезапустите приложение для старта!", dest.display()),
                                AppLanguage::English => format!("✅ Archive unpacked & transferred to: {}\nRestart app to start!", dest.display()),
                            };
                            self.toast = Some((msg, Instant::now()));
                            self.show_file_manager = false;
                        }
                        Err(e) => {
                            let msg = match self.lang {
                                AppLanguage::Russian => format!("Ошибка распаковки: {e}"),
                                AppLanguage::English => format!("Unpack error: {e}"),
                            };
                            self.toast = Some((msg, Instant::now()));
                        }
                    }
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
                                    r: 0.04,
                                    g: 0.05,
                                    b: 0.09,
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

                for id in &egui_output.textures_delta.free {
                    state.egui_renderer.free_texture(id);
                }

                commands.push(encoder.finish());
                state.queue.submit(commands);
                frame.present();
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(state) = &self.state {
            state.window.request_redraw();
        }
    }
}
