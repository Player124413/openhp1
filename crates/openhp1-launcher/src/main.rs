use std::{
    path::PathBuf,
    process::Command,
    sync::mpsc::{Receiver, channel},
};

use anyhow::{Context, Result, anyhow, bail};
use eframe::egui::{self, Align2, Color32, CornerRadius, Id, RichText, Stroke, TextureHandle, Vec2};
use openhp1_package::{
    GameInstallation, configure_game_installation, read_openhp1_ini_value,
    resolve_game_installation, save_openhp1_ini_values,
};
use serde::Deserialize;

const SPLASH: &[u8] = include_bytes!("../../../splash.jpg");
const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AppLanguage {
    Russian,
    English,
}

fn detect_system_language() -> AppLanguage {
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
        if let Ok(output) = Command::new("getprop").arg("persist.sys.locale").output() {
            let s = String::from_utf8_lossy(&output.stdout).to_ascii_lowercase();
            if s.contains("ru") {
                return AppLanguage::Russian;
            }
        }
    }

    AppLanguage::English
}

#[derive(Deserialize, Clone, Debug)]
pub struct GitHubRelease {
    pub tag_name: String,
    pub name: Option<String>,
    pub html_url: String,
    pub body: Option<String>,
    pub published_at: Option<String>,
}

#[derive(Clone, Debug)]
enum UpdateStatus {
    Idle,
    Checking,
    Found(GitHubRelease),
    UpToDate { version: String },
    Error(String),
}

fn main() -> Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([724.0, 770.0])
            .with_resizable(false),
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };
    eframe::run_native(
        "OpenHP1",
        options,
        Box::new(|context| Ok(Box::new(Launcher::new(context)?))),
    )
    .map_err(|error| anyhow!(error.to_string()))
}

struct Launcher {
    splash: TextureHandle,
    installation: Option<GameInstallation>,
    status: String,
    status_color: Color32,
    touch_enabled: bool,
    lang: AppLanguage,
    show_about_popup: bool,
    show_updates_popup: bool,
    update_status: UpdateStatus,
    update_receiver: Option<Receiver<Result<GitHubRelease>>>,
}

impl Launcher {
    fn new(context: &eframe::CreationContext<'_>) -> Result<Self> {
        let lang = detect_system_language();
        let image = image::load_from_memory(SPLASH)
            .context("failed to decode launcher splash image")?
            .to_rgba8();
        let size = [image.width() as usize, image.height() as usize];
        let splash = context.egui_ctx.load_texture(
            "OpenHP1 launcher splash",
            egui::ColorImage::from_rgba_unmultiplied(size, image.as_raw()),
            egui::TextureOptions::LINEAR,
        );
        let (installation, status, status_color) = match resolve_game_installation() {
            Ok(installation) => {
                let msg = match lang {
                    AppLanguage::Russian => format!("Файлы игры: {}", installation.root().display()),
                    AppLanguage::English => format!("Game files: {}", installation.root().display()),
                };
                (Some(installation), msg, Color32::from_rgb(150, 210, 150))
            }
            Err(error) => {
                let msg = match lang {
                    AppLanguage::Russian => "Файлы игры не найдены. Выберите папку или распакуйте ZIP архив".to_owned(),
                    AppLanguage::English => error.to_string(),
                };
                (None, msg, Color32::from_rgb(235, 185, 110))
            }
        };
        let touch_enabled = read_openhp1_ini_value("OpenHP1.Touch", "Enabled")
            .map(|v| match v.trim().to_ascii_lowercase().as_str() {
                "true" | "1" | "on" => true,
                "false" | "0" | "off" => false,
                _ => cfg!(target_os = "android"),
            })
            .unwrap_or(cfg!(target_os = "android"));
        Ok(Self {
            splash,
            installation,
            status,
            status_color,
            touch_enabled,
            lang,
            show_about_popup: false,
            show_updates_popup: false,
            update_status: UpdateStatus::Idle,
            update_receiver: None,
        })
    }

    fn start_update_check(&mut self) {
        self.update_status = UpdateStatus::Checking;
        let (tx, rx) = channel();
        self.update_receiver = Some(rx);
        std::thread::spawn(move || {
            let res = check_github_releases();
            let _ = tx.send(res);
        });
    }

    fn choose_game_folder(&mut self) {
        let title = match self.lang {
            AppLanguage::Russian => "Выберите папку с игрой Гарри Поттер",
            AppLanguage::English => "Choose the Harry Potter game folder",
        };
        let Some(root) = rfd::FileDialog::new()
            .set_title(title)
            .pick_folder()
        else {
            return;
        };
        match configure_game_installation(&root, None) {
            Ok(installation) => self.set_installation(installation),
            Err(error) => self.set_error(error),
        }
    }

    fn unpack_zip(&mut self) {
        let title = match self.lang {
            AppLanguage::Russian => "Выберите ZIP архив с игрой Гарри Поттер",
            AppLanguage::English => "Select Harry Potter ZIP archive",
        };
        let Some(zip_file) = rfd::FileDialog::new()
            .set_title(title)
            .add_filter("ZIP Archive", &["zip"])
            .pick_file()
        else {
            return;
        };

        let file_name = zip_file.file_name().unwrap_or_default().to_string_lossy();
        self.status = match self.lang {
            AppLanguage::Russian => format!("Распаковка {file_name}..."),
            AppLanguage::English => format!("Unpacking {file_name}..."),
        };
        self.status_color = Color32::from_rgb(180, 210, 255);

        match openhp1_package::install_from_zip(&zip_file, None) {
            Ok(installation) => {
                self.status = match self.lang {
                    AppLanguage::Russian => format!(
                        "Файлы игры успешно распакованы: {}",
                        installation.root().display()
                    ),
                    AppLanguage::English => format!(
                        "Successfully unpacked game files: {}",
                        installation.root().display()
                    ),
                };
                self.status_color = Color32::from_rgb(150, 210, 150);
                self.installation = Some(installation);
            }
            Err(error) => self.set_error(error),
        }
    }

    fn select_language(&mut self, root: &PathBuf, language: &str) {
        match configure_game_installation(root, Some(language)) {
            Ok(installation) => self.set_installation(installation),
            Err(error) => self.set_error(error),
        }
    }

    fn set_installation(&mut self, installation: GameInstallation) {
        self.status = match self.lang {
            AppLanguage::Russian => format!("Файлы игры: {}", installation.root().display()),
            AppLanguage::English => format!("Game files: {}", installation.root().display()),
        };
        self.status_color = Color32::from_rgb(150, 210, 150);
        self.installation = Some(installation);
    }

    fn set_error(&mut self, error: impl std::fmt::Display) {
        self.status = error.to_string();
        self.status_color = Color32::from_rgb(240, 135, 120);
    }

    fn language_selector(&mut self, ui: &mut egui::Ui) {
        let Some(installation) = &self.installation else {
            return;
        };
        let root = installation.root().to_path_buf();
        let mut selected = installation.language().to_owned();
        let languages = installation.available_languages().to_vec();
        let mut changed = false;

        let label = match self.lang {
            AppLanguage::Russian => "Язык игры",
            AppLanguage::English => "Language",
        };

        ui.horizontal(|ui| {
            ui.add_space(((ui.available_width() - 270.0) / 2.0).max(0.0));
            ui.label(label);
            egui::ComboBox::from_id_salt("game-language")
                .width(190.0)
                .selected_text(language_label(&selected))
                .show_ui(ui, |ui| {
                    for language in languages {
                        changed |= ui
                            .selectable_value(
                                &mut selected,
                                language.clone(),
                                language_label(&language),
                            )
                            .changed();
                    }
                });
        });
        if changed {
            self.select_language(&root, &selected);
        }
    }

    fn play(&mut self, context: &egui::Context) {
        match launch_game() {
            Ok(()) => context.send_viewport_cmd(egui::ViewportCommand::Close),
            Err(error) => {
                self.status = error.to_string();
                self.status_color = Color32::from_rgb(240, 135, 120);
            }
        }
    }

    fn copy_logs(&mut self, context: &egui::Context) {
        let logs_dir = openhp1_package::settings_dir().join("Logs");
        let mut log_content = String::new();
        if let Ok(entries) = std::fs::read_dir(&logs_dir) {
            let mut files: Vec<_> = entries.filter_map(|e| e.ok()).collect();
            files.sort_by_key(|e| e.metadata().and_then(|m| m.modified()).ok());
            if let Some(latest) = files.last() {
                if let Ok(content) = std::fs::read_to_string(latest.path()) {
                    log_content = content;
                }
            }
        }
        if log_content.is_empty() {
            log_content = format!(
                "OpenHP1 Status: {}\nGame Root: {:?}\nSettings Dir: {:?}",
                self.status,
                self.installation.as_ref().map(|i| i.root()),
                openhp1_package::settings_dir()
            );
        }
        context.copy_text(log_content.clone());
        context.output_mut(|o| o.copied_text = log_content);
        self.status = match self.lang {
            AppLanguage::Russian => "Логи скопированы в буфер обмена!".to_owned(),
            AppLanguage::English => "Logs copied to clipboard!".to_owned(),
        };
        self.status_color = Color32::from_rgb(100, 240, 140);
    }
}

impl eframe::App for Launcher {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let context = ui.ctx().clone();

        // Check for async update check results
        if let Some(rx) = &self.update_receiver {
            if let Ok(res) = rx.try_recv() {
                self.update_receiver = None;
                match res {
                    Ok(release) => {
                        let clean_tag = release.tag_name.trim_start_matches(|c| c == 'v' || c == 'V');
                        if clean_tag == CURRENT_VERSION {
                            self.update_status = UpdateStatus::UpToDate {
                                version: release.tag_name.clone(),
                            };
                        } else {
                            self.update_status = UpdateStatus::Found(release);
                        }
                    }
                    Err(err) => {
                        self.update_status = UpdateStatus::Error(err.to_string());
                    }
                }
            }
        }

        ui.painter()
            .rect_filled(ui.max_rect(), 0.0, Color32::from_rgb(3, 3, 12));

        egui::Frame::NONE.show(ui, |ui| {
            let width = ui.available_width();
            ui.add(
                egui::Image::new(&self.splash)
                    .fit_to_exact_size(Vec2::new(width, width * 543.0 / 724.0)),
            );
            ui.add_space(10.0);
            ui.vertical_centered(|ui| {
                ui.label(RichText::new(&self.status).color(self.status_color));
                ui.add_space(8.0);
                self.language_selector(ui);
                ui.add_space(8.0);

                // Settings row with touch controls and interface language toggle
                ui.horizontal(|ui| {
                    ui.add_space(((ui.available_width() - 320.0) / 2.0).max(0.0));
                    let touch_text = match self.lang {
                        AppLanguage::Russian => "Сенсорное управление",
                        AppLanguage::English => "Touch Controls",
                    };
                    if ui
                        .checkbox(&mut self.touch_enabled, touch_text)
                        .changed()
                    {
                        let _ = save_openhp1_ini_values(&[(
                            "OpenHP1.Touch",
                            "Enabled",
                            &self.touch_enabled.to_string(),
                        )]);
                    }

                    ui.separator();

                    // Language switch button
                    let lang_text = match self.lang {
                        AppLanguage::Russian => "Язык: Русский",
                        AppLanguage::English => "Language: English",
                    };
                    if ui.button(RichText::new(lang_text).color(Color32::from_rgb(180, 210, 255))).clicked() {
                        self.lang = match self.lang {
                            AppLanguage::Russian => AppLanguage::English,
                            AppLanguage::English => AppLanguage::Russian,
                        };
                    }

                    ui.separator();

                    let copy_text = match self.lang {
                        AppLanguage::Russian => "📋 Логи",
                        AppLanguage::English => "📋 Logs",
                    };
                    if ui.button(RichText::new(copy_text).color(Color32::from_rgb(180, 240, 190))).clicked() {
                        self.copy_logs(&context);
                    }
                });

                ui.add_space(12.0);

                // Row 1 of action buttons: Play, Choose Game Folder, Unpack ZIP Archive
                ui.horizontal(|ui| {
                    ui.add_space(((ui.available_width() - 530.0) / 2.0).max(0.0));
                    let button_size = Vec2::new(170.0, 40.0);

                    let play_text = match self.lang {
                        AppLanguage::Russian => "Играть",
                        AppLanguage::English => "Play",
                    };
                    if ui
                        .add_enabled(
                            self.installation.is_some(),
                            egui::Button::new(RichText::new(play_text).strong().size(16.0)).min_size(button_size),
                        )
                        .clicked()
                    {
                        self.play(&context);
                    }

                    let folder_text = match self.lang {
                        AppLanguage::Russian => "Выбрать папку с игрой",
                        AppLanguage::English => "Choose Game Folder",
                    };
                    if ui
                        .add(egui::Button::new(folder_text).min_size(button_size))
                        .clicked()
                    {
                        self.choose_game_folder();
                    }

                    let zip_text = match self.lang {
                        AppLanguage::Russian => "Распаковать ZIP архив",
                        AppLanguage::English => "Unpack ZIP Archive",
                    };
                    if ui
                        .add(egui::Button::new(zip_text).min_size(button_size))
                        .clicked()
                    {
                        self.unpack_zip();
                    }
                });

                ui.add_space(8.0);

                // Row 2 of action buttons: Check for Updates, About Port, Exit
                ui.horizontal(|ui| {
                    ui.add_space(((ui.available_width() - 530.0) / 2.0).max(0.0));
                    let button_size = Vec2::new(170.0, 36.0);

                    let updates_text = match self.lang {
                        AppLanguage::Russian => "Проверить обновления",
                        AppLanguage::English => "Check for Updates",
                    };
                    if ui
                        .add(egui::Button::new(updates_text).min_size(button_size))
                        .clicked()
                    {
                        self.show_updates_popup = true;
                        if matches!(self.update_status, UpdateStatus::Idle) {
                            self.start_update_check();
                        }
                    }

                    let about_text = match self.lang {
                        AppLanguage::Russian => "О порте",
                        AppLanguage::English => "About Port",
                    };
                    if ui
                        .add(egui::Button::new(about_text).min_size(button_size))
                        .clicked()
                    {
                        self.show_about_popup = true;
                    }

                    let exit_text = match self.lang {
                        AppLanguage::Russian => "Выход",
                        AppLanguage::English => "Exit",
                    };
                    if ui
                        .add(egui::Button::new(exit_text).min_size(button_size))
                        .clicked()
                    {
                        context.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });
            });
        });

        // About / Credits Modal Popup
        if self.show_about_popup {
            let title = match self.lang {
                AppLanguage::Russian => "О порте OpenHP1",
                AppLanguage::English => "About OpenHP1 Port",
            };
            egui::Window::new(title)
                .id(Id::new("about_popup_window"))
                .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
                .resizable(false)
                .collapsible(false)
                .frame(
                    egui::Frame::window(ui.style())
                        .fill(Color32::from_rgb(18, 22, 34))
                        .stroke(Stroke::new(2.0, Color32::from_rgb(220, 185, 90)))
                        .corner_radius(CornerRadius::same(12)),
                )
                .show(&context, |ui| {
                    ui.set_max_width(460.0);
                    ui.vertical_centered(|ui| {
                        ui.add_space(8.0);
                        ui.heading(
                            RichText::new("OpenHP1 Android & Desktop Port")
                                .color(Color32::from_rgb(255, 215, 0))
                                .strong(),
                        );
                        ui.add_space(10.0);

                        let port_by = match self.lang {
                            AppLanguage::Russian => "Порт создан Player1444:",
                            AppLanguage::English => "Port made by Player1444:",
                        };
                        ui.label(RichText::new(port_by).size(15.0).strong());
                        ui.hyperlink_to(
                            RichText::new("https://t.me/player1444ports")
                                .size(15.0)
                                .color(Color32::from_rgb(100, 180, 255)),
                            "https://t.me/player1444ports",
                        );

                        ui.add_space(14.0);
                        ui.separator();
                        ui.add_space(10.0);

                        let thanks = match self.lang {
                            AppLanguage::Russian => "А также огромное спасибо этому репозиторию:",
                            AppLanguage::English => "Huge thanks to this repository:",
                        };
                        ui.label(RichText::new(thanks).size(14.0));
                        ui.hyperlink_to(
                            RichText::new("https://github.com/SplittyDev/openhp1")
                                .size(14.0)
                                .color(Color32::from_rgb(140, 200, 255)),
                            "https://github.com/SplittyDev/openhp1",
                        );

                        let without_it = match self.lang {
                            AppLanguage::Russian => "Без него этот порт бы не вышел!",
                            AppLanguage::English => {
                                "Without it, this port would not have been possible!"
                            }
                        };
                        ui.label(
                            RichText::new(without_it)
                                .italics()
                                .size(13.0)
                                .color(Color32::from_rgb(210, 210, 220)),
                        );

                        ui.add_space(16.0);
                        let close_text = match self.lang {
                            AppLanguage::Russian => "Закрыть",
                            AppLanguage::English => "Close",
                        };
                        if ui.button(RichText::new(close_text).size(14.0)).clicked() {
                            self.show_about_popup = false;
                        }
                        ui.add_space(4.0);
                    });
                });
        }

        // Updates Modal Popup (Checks GitHub Releases)
        if self.show_updates_popup {
            let title = match self.lang {
                AppLanguage::Russian => "Проверка обновлений (GitHub)",
                AppLanguage::English => "Check for Updates (GitHub)",
            };
            egui::Window::new(title)
                .id(Id::new("updates_popup_window"))
                .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
                .resizable(false)
                .collapsible(false)
                .frame(
                    egui::Frame::window(ui.style())
                        .fill(Color32::from_rgb(18, 22, 34))
                        .stroke(Stroke::new(2.0, Color32::from_rgb(100, 160, 240)))
                        .corner_radius(CornerRadius::same(12)),
                )
                .show(&context, |ui| {
                    ui.set_max_width(480.0);
                    ui.vertical_centered(|ui| {
                        ui.add_space(8.0);
                        ui.heading(
                            RichText::new(title)
                                .color(Color32::from_rgb(130, 200, 255))
                                .strong(),
                        );
                        ui.add_space(12.0);

                        match &self.update_status {
                            UpdateStatus::Idle | UpdateStatus::Checking => {
                                let check_msg = match self.lang {
                                    AppLanguage::Russian => "Проверка последних релизов на GitHub...",
                                    AppLanguage::English => "Checking for latest releases on GitHub...",
                                };
                                ui.spinner();
                                ui.add_space(8.0);
                                ui.label(RichText::new(check_msg).size(14.0));
                            }
                            UpdateStatus::Found(release) => {
                                let new_ver_msg = match self.lang {
                                    AppLanguage::Russian => "Доступно новое обновление на GitHub!",
                                    AppLanguage::English => "New update available on GitHub!",
                                };
                                ui.label(
                                    RichText::new(new_ver_msg)
                                        .color(Color32::from_rgb(120, 240, 150))
                                        .strong()
                                        .size(16.0),
                                );
                                ui.add_space(8.0);

                                let tag_title = release.name.as_deref().unwrap_or(&release.tag_name);
                                ui.label(
                                    RichText::new(format!("Релиз: {tag_title}"))
                                        .color(Color32::from_rgb(255, 220, 100))
                                        .size(15.0),
                                );
                                ui.add_space(6.0);

                                let open_gh = match self.lang {
                                    AppLanguage::Russian => "Открыть страницу релиза на GitHub",
                                    AppLanguage::English => "Open Release Page on GitHub",
                                };
                                ui.hyperlink_to(
                                    RichText::new(open_gh).size(14.0).color(Color32::from_rgb(100, 180, 255)).strong(),
                                    &release.html_url,
                                );
                            }
                            UpdateStatus::UpToDate { version } => {
                                let up_to_date_msg = match self.lang {
                                    AppLanguage::Russian => format!("У вас установлена актуальная версия ({version})!"),
                                    AppLanguage::English => format!("You are running the latest version ({version})!"),
                                };
                                ui.label(
                                    RichText::new(up_to_date_msg)
                                        .color(Color32::from_rgb(120, 220, 140))
                                        .size(15.0)
                                        .strong(),
                                );
                                ui.add_space(6.0);
                                let no_updates = match self.lang {
                                    AppLanguage::Russian => "Новых обновлений на GitHub нет.",
                                    AppLanguage::English => "No new updates found on GitHub.",
                                };
                                ui.label(RichText::new(no_updates).size(13.0));
                            }
                            UpdateStatus::Error(err) => {
                                let err_title = match self.lang {
                                    AppLanguage::Russian => "Не удалось связаться с GitHub:",
                                    AppLanguage::English => "Could not reach GitHub:",
                                };
                                ui.label(
                                    RichText::new(err_title)
                                        .color(Color32::from_rgb(240, 130, 110))
                                        .strong(),
                                );
                                ui.label(RichText::new(err).color(Color32::from_rgb(230, 180, 150)).size(12.0));
                            }
                        }

                        ui.add_space(14.0);
                        ui.separator();
                        ui.add_space(8.0);

                        let tg_label = match self.lang {
                            AppLanguage::Russian => "Следите за обновлениями и портами в Telegram:",
                            AppLanguage::English => "Follow updates and new ports on Telegram:",
                        };
                        ui.label(RichText::new(tg_label).size(13.0));
                        ui.hyperlink_to(
                            RichText::new("https://t.me/player1444ports").size(14.0).color(Color32::from_rgb(100, 180, 255)),
                            "https://t.me/player1444ports",
                        );

                        ui.add_space(14.0);
                        ui.horizontal(|ui| {
                            ui.add_space(((ui.available_width() - 220.0) / 2.0).max(0.0));
                            let refresh_text = match self.lang {
                                AppLanguage::Russian => "Проверить снова",
                                AppLanguage::English => "Check Again",
                            };
                            if ui.button(refresh_text).clicked() {
                                self.start_update_check();
                            }

                            let close_text = match self.lang {
                                AppLanguage::Russian => "Закрыть",
                                AppLanguage::English => "Close",
                            };
                            if ui.button(close_text).clicked() {
                                self.show_updates_popup = false;
                            }
                        });
                        ui.add_space(4.0);
                    });
                });
        }
    }
}

fn check_github_releases() -> Result<GitHubRelease> {
    // 1. Try Player124413/openhp1 fork releases
    if let Ok(release) = fetch_release_from_url("https://api.github.com/repos/Player124413/openhp1/releases/latest") {
        return Ok(release);
    }
    // 2. Fallback to upstream SplittyDev/openhp1 releases
    fetch_release_from_url("https://api.github.com/repos/SplittyDev/openhp1/releases/latest")
}

fn fetch_release_from_url(url: &str) -> Result<GitHubRelease> {
    let output = Command::new("curl")
        .args([
            "-s",
            "-L",
            "--max-time",
            "10",
            "-H",
            "User-Agent: OpenHP1-Launcher",
            "-H",
            "Accept: application/vnd.github.v3+json",
            url,
        ])
        .output()
        .context("failed to execute curl")?;

    if !output.status.success() {
        bail!("curl failed with status {:?}", output.status.code());
    }

    let text = String::from_utf8(output.stdout)
        .context("invalid utf-8 in curl response")?;

    if text.contains("\"message\": \"Not Found\"") || text.contains("\"message\":\"Not Found\"") {
        bail!("release not found");
    }

    let release: GitHubRelease = serde_json::from_str(&text)
        .context("failed to parse GitHub release JSON")?;

    Ok(release)
}

fn launch_game() -> Result<()> {
    let executable = game_executable()?;
    Command::new(&executable)
        .spawn()
        .with_context(|| format!("failed to launch {}", executable.display()))?;
    Ok(())
}

fn game_executable() -> Result<PathBuf> {
    let launcher = std::env::current_exe().context("failed to locate the OpenHP1 launcher")?;
    let directory = launcher
        .parent()
        .context("the OpenHP1 launcher has no parent directory")?;
    let executable = directory.join(if cfg!(target_os = "windows") {
        "openhp1-game.exe"
    } else {
        "openhp1-game"
    });
    if !executable.is_file() {
        bail!("could not find {}", executable.display());
    }
    Ok(executable)
}

fn language_label(language: &str) -> String {
    let name = match language.to_ascii_lowercase().as_str() {
        "int" | "eng" => Some("English"),
        "fre" => Some("French"),
        "ger" => Some("German"),
        "spa" => Some("Spanish"),
        "ita" => Some("Italian"),
        "dut" => Some("Dutch"),
        "por" => Some("Portuguese"),
        "pol" => Some("Polish"),
        "rus" => Some("Russian"),
        "hun" => Some("Hungarian"),
        "cze" => Some("Czech"),
        _ => None,
    };
    name.map_or_else(
        || language.to_ascii_uppercase(),
        |name| format!("{name} ({language})"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_known_and_unknown_game_languages() {
        assert_eq!(language_label("fre"), "French (fre)");
        assert_eq!(language_label("xyz"), "XYZ");
    }

    #[test]
    fn system_language_detection_fallback() {
        assert!(matches!(
            detect_system_language(),
            AppLanguage::English | AppLanguage::Russian
        ));
    }

    #[test]
    fn parses_github_release_json() {
        let json = r#"{
            "tag_name": "v0.2.0",
            "name": "Release v0.2.0",
            "html_url": "https://github.com/SplittyDev/openhp1/releases/tag/v0.2.0",
            "body": "Fixed bugs and added features"
        }"#;
        let release: GitHubRelease = serde_json::from_str(json).expect("valid JSON");
        assert_eq!(release.tag_name, "v0.2.0");
        assert_eq!(release.name.as_deref(), Some("Release v0.2.0"));
        assert_eq!(
            release.html_url,
            "https://github.com/SplittyDev/openhp1/releases/tag/v0.2.0"
        );
    }
}
