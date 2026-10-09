use std::collections::HashMap;

use egui::{
    Align2, Color32, CornerRadius, FontId, Id, LayerId, Order, Pos2, Rect, Stroke, StrokeKind, Vec2,
};
use openhp1_runtime::{ConsoleCommands, PlayerInput};
use winit::event::{Touch, TouchPhase};

const CONFIG: &str = "OpenHP1";
const SECTION: &str = "OpenHP1.Touch";

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TouchButtonConfig {
    pub x: f32,
    pub y: f32,
    pub size: f32,
    pub visible: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TouchStickConfig {
    pub x: f32,
    pub y: f32,
    pub radius: f32,
    pub visible: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum TouchElementKind {
    Stick,
    Cast,
    Jump,
    Interact,
    Sneak,
    BroomBoost,
    BroomBrake,
    Menu,
    Console,
}

impl TouchElementKind {
    pub const ALL: [Self; 9] = [
        Self::Stick,
        Self::Cast,
        Self::Jump,
        Self::Interact,
        Self::Sneak,
        Self::BroomBoost,
        Self::BroomBrake,
        Self::Menu,
        Self::Console,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Stick => "Move Stick",
            Self::Cast => "Cast Spell",
            Self::Jump => "Jump / Climb",
            Self::Interact => "Use / Interact",
            Self::Sneak => "Walk / Sneak",
            Self::BroomBoost => "Broom Boost",
            Self::BroomBrake => "Broom Brake",
            Self::Menu => "Pause Menu",
            Self::Console => "Console",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct TouchSettings {
    pub enabled: bool,
    pub opacity: f32,
    pub look_sensitivity: f32,
    pub stick: TouchStickConfig,
    pub cast: TouchButtonConfig,
    pub jump: TouchButtonConfig,
    pub interact: TouchButtonConfig,
    pub sneak: TouchButtonConfig,
    pub broom_boost: TouchButtonConfig,
    pub broom_brake: TouchButtonConfig,
    pub menu: TouchButtonConfig,
    pub console: TouchButtonConfig,
    pub look_visible: bool,
}

impl Default for TouchSettings {
    fn default() -> Self {
        Self {
            enabled: cfg!(target_os = "android"),
            opacity: 0.75,
            look_sensitivity: 1.0,
            stick: TouchStickConfig {
                x: 0.15,
                y: 0.72,
                radius: 64.0,
                visible: true,
            },
            cast: TouchButtonConfig {
                x: 0.88,
                y: 0.72,
                size: 46.0,
                visible: true,
            },
            jump: TouchButtonConfig {
                x: 0.77,
                y: 0.82,
                size: 40.0,
                visible: true,
            },
            interact: TouchButtonConfig {
                x: 0.88,
                y: 0.52,
                size: 36.0,
                visible: true,
            },
            sneak: TouchButtonConfig {
                x: 0.15,
                y: 0.42,
                size: 32.0,
                visible: true,
            },
            broom_boost: TouchButtonConfig {
                x: 0.76,
                y: 0.63,
                size: 34.0,
                visible: true,
            },
            broom_brake: TouchButtonConfig {
                x: 0.76,
                y: 0.47,
                size: 34.0,
                visible: true,
            },
            menu: TouchButtonConfig {
                x: 0.94,
                y: 0.08,
                size: 26.0,
                visible: true,
            },
            console: TouchButtonConfig {
                x: 0.06,
                y: 0.08,
                size: 26.0,
                visible: true,
            },
            look_visible: true,
        }
    }
}

impl TouchSettings {
    pub fn load(console: &ConsoleCommands) -> Self {
        let defaults = Self::default();

        let parse_f32 = |key: &str, def: f32| -> f32 {
            console
                .config_value(CONFIG, SECTION, key)
                .and_then(|v| v.parse().ok())
                .unwrap_or(def)
        };

        let parse_bool = |key: &str, def: bool| -> bool {
            console
                .config_value(CONFIG, SECTION, key)
                .map(|v| match v.trim().to_ascii_lowercase().as_str() {
                    "true" | "1" | "on" => true,
                    "false" | "0" | "off" => false,
                    _ => def,
                })
                .unwrap_or(def)
        };

        let parse_button = |prefix: &str, def: TouchButtonConfig| -> TouchButtonConfig {
            TouchButtonConfig {
                x: parse_f32(&format!("{prefix}X"), def.x),
                y: parse_f32(&format!("{prefix}Y"), def.y),
                size: parse_f32(&format!("{prefix}Size"), def.size),
                visible: parse_bool(&format!("{prefix}Visible"), def.visible),
            }
        };

        Self {
            enabled: parse_bool("Enabled", defaults.enabled),
            opacity: parse_f32("Opacity", defaults.opacity).clamp(0.1, 1.0),
            look_sensitivity: parse_f32("LookSensitivity", defaults.look_sensitivity).clamp(0.2, 5.0),
            stick: TouchStickConfig {
                x: parse_f32("StickX", defaults.stick.x),
                y: parse_f32("StickY", defaults.stick.y),
                radius: parse_f32("StickRadius", defaults.stick.radius),
                visible: parse_bool("StickVisible", defaults.stick.visible),
            },
            cast: parse_button("Cast", defaults.cast),
            jump: parse_button("Jump", defaults.jump),
            interact: parse_button("Interact", defaults.interact),
            sneak: parse_button("Sneak", defaults.sneak),
            broom_boost: parse_button("BroomBoost", defaults.broom_boost),
            broom_brake: parse_button("BroomBrake", defaults.broom_brake),
            menu: parse_button("Menu", defaults.menu),
            console: parse_button("Console", defaults.console),
            look_visible: parse_bool("LookVisible", defaults.look_visible),
        }
    }

    pub fn save(&self, console: &ConsoleCommands) -> std::io::Result<()> {
        let mut entries = Vec::new();
        entries.push(("Enabled", self.enabled.to_string()));
        entries.push(("Opacity", format!("{:.2}", self.opacity)));
        entries.push(("LookSensitivity", format!("{:.2}", self.look_sensitivity)));

        entries.push(("StickX", format!("{:.3}", self.stick.x)));
        entries.push(("StickY", format!("{:.3}", self.stick.y)));
        entries.push(("StickRadius", format!("{:.1}", self.stick.radius)));
        entries.push(("StickVisible", self.stick.visible.to_string()));

        let mut push_button = |prefix: &str, btn: &TouchButtonConfig| {
            entries.push((format!("{prefix}X").leak() as &str, format!("{:.3}", btn.x)));
            entries.push((format!("{prefix}Y").leak() as &str, format!("{:.3}", btn.y)));
            entries.push((format!("{prefix}Size").leak() as &str, format!("{:.1}", btn.size)));
            entries.push((format!("{prefix}Visible").leak() as &str, btn.visible.to_string()));
        };

        push_button("Cast", &self.cast);
        push_button("Jump", &self.jump);
        push_button("Interact", &self.interact);
        push_button("Sneak", &self.sneak);
        push_button("BroomBoost", &self.broom_boost);
        push_button("BroomBrake", &self.broom_brake);
        push_button("Menu", &self.menu);
        push_button("Console", &self.console);

        entries.push(("LookVisible", self.look_visible.to_string()));

        console.save_config_values(CONFIG, SECTION, &entries)
    }

    pub fn reset_defaults(&mut self) {
        let enabled = self.enabled;
        *self = Self::default();
        self.enabled = enabled;
    }

    pub fn element_rect(&self, kind: TouchElementKind, screen_size: [f32; 2]) -> Rect {
        let w = screen_size[0];
        let h = screen_size[1];
        match kind {
            TouchElementKind::Stick => {
                let center = Pos2::new(self.stick.x * w, self.stick.y * h);
                Rect::from_center_size(center, Vec2::splat(self.stick.radius * 2.0))
            }
            TouchElementKind::Cast => {
                let center = Pos2::new(self.cast.x * w, self.cast.y * h);
                Rect::from_center_size(center, Vec2::splat(self.cast.size * 2.0))
            }
            TouchElementKind::Jump => {
                let center = Pos2::new(self.jump.x * w, self.jump.y * h);
                Rect::from_center_size(center, Vec2::splat(self.jump.size * 2.0))
            }
            TouchElementKind::Interact => {
                let center = Pos2::new(self.interact.x * w, self.interact.y * h);
                Rect::from_center_size(center, Vec2::splat(self.interact.size * 2.0))
            }
            TouchElementKind::Sneak => {
                let center = Pos2::new(self.sneak.x * w, self.sneak.y * h);
                Rect::from_center_size(center, Vec2::splat(self.sneak.size * 2.0))
            }
            TouchElementKind::BroomBoost => {
                let center = Pos2::new(self.broom_boost.x * w, self.broom_boost.y * h);
                Rect::from_center_size(center, Vec2::splat(self.broom_boost.size * 2.0))
            }
            TouchElementKind::BroomBrake => {
                let center = Pos2::new(self.broom_brake.x * w, self.broom_brake.y * h);
                Rect::from_center_size(center, Vec2::splat(self.broom_brake.size * 2.0))
            }
            TouchElementKind::Menu => {
                let center = Pos2::new(self.menu.x * w, self.menu.y * h);
                Rect::from_center_size(center, Vec2::splat(self.menu.size * 2.0))
            }
            TouchElementKind::Console => {
                let center = Pos2::new(self.console.x * w, self.console.y * h);
                Rect::from_center_size(center, Vec2::splat(self.console.size * 2.0))
            }
        }
    }

    pub fn set_element_pos(&mut self, kind: TouchElementKind, norm_pos: Pos2) {
        let x = norm_pos.x.clamp(0.02, 0.98);
        let y = norm_pos.y.clamp(0.02, 0.98);
        match kind {
            TouchElementKind::Stick => {
                self.stick.x = x;
                self.stick.y = y;
            }
            TouchElementKind::Cast => {
                self.cast.x = x;
                self.cast.y = y;
            }
            TouchElementKind::Jump => {
                self.jump.x = x;
                self.jump.y = y;
            }
            TouchElementKind::Interact => {
                self.interact.x = x;
                self.interact.y = y;
            }
            TouchElementKind::Sneak => {
                self.sneak.x = x;
                self.sneak.y = y;
            }
            TouchElementKind::BroomBoost => {
                self.broom_boost.x = x;
                self.broom_boost.y = y;
            }
            TouchElementKind::BroomBrake => {
                self.broom_brake.x = x;
                self.broom_brake.y = y;
            }
            TouchElementKind::Menu => {
                self.menu.x = x;
                self.menu.y = y;
            }
            TouchElementKind::Console => {
                self.console.x = x;
                self.console.y = y;
            }
        }
    }

    pub fn element_size(&self, kind: TouchElementKind) -> f32 {
        match kind {
            TouchElementKind::Stick => self.stick.radius,
            TouchElementKind::Cast => self.cast.size,
            TouchElementKind::Jump => self.jump.size,
            TouchElementKind::Interact => self.interact.size,
            TouchElementKind::Sneak => self.sneak.size,
            TouchElementKind::BroomBoost => self.broom_boost.size,
            TouchElementKind::BroomBrake => self.broom_brake.size,
            TouchElementKind::Menu => self.menu.size,
            TouchElementKind::Console => self.console.size,
        }
    }

    pub fn set_element_size(&mut self, kind: TouchElementKind, size: f32) {
        let size = size.clamp(16.0, 120.0);
        match kind {
            TouchElementKind::Stick => self.stick.radius = size,
            TouchElementKind::Cast => self.cast.size = size,
            TouchElementKind::Jump => self.jump.size = size,
            TouchElementKind::Interact => self.interact.size = size,
            TouchElementKind::Sneak => self.sneak.size = size,
            TouchElementKind::BroomBoost => self.broom_boost.size = size,
            TouchElementKind::BroomBrake => self.broom_brake.size = size,
            TouchElementKind::Menu => self.menu.size = size,
            TouchElementKind::Console => self.console.size = size,
        }
    }

    pub fn is_element_visible(&self, kind: TouchElementKind) -> bool {
        match kind {
            TouchElementKind::Stick => self.stick.visible,
            TouchElementKind::Cast => self.cast.visible,
            TouchElementKind::Jump => self.jump.visible,
            TouchElementKind::Interact => self.interact.visible,
            TouchElementKind::Sneak => self.sneak.visible,
            TouchElementKind::BroomBoost => self.broom_boost.visible,
            TouchElementKind::BroomBrake => self.broom_brake.visible,
            TouchElementKind::Menu => self.menu.visible,
            TouchElementKind::Console => self.console.visible,
        }
    }

    pub fn set_element_visible(&mut self, kind: TouchElementKind, visible: bool) {
        match kind {
            TouchElementKind::Stick => self.stick.visible = visible,
            TouchElementKind::Cast => self.cast.visible = visible,
            TouchElementKind::Jump => self.jump.visible = visible,
            TouchElementKind::Interact => self.interact.visible = visible,
            TouchElementKind::Sneak => self.sneak.visible = visible,
            TouchElementKind::BroomBoost => self.broom_boost.visible = visible,
            TouchElementKind::BroomBrake => self.broom_brake.visible = visible,
            TouchElementKind::Menu => self.menu.visible = visible,
            TouchElementKind::Console => self.console.visible = visible,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum TouchRole {
    Stick,
    Look { last_pos: Pos2 },
    Button(TouchElementKind),
}

#[derive(Default)]
pub struct TouchEditorState {
    pub is_active: bool,
    pub selected_element: Option<TouchElementKind>,
    pub dragging: bool,
    pub drag_offset: Vec2,
    pub needs_save: bool,
}

#[derive(Default)]
pub struct TouchController {
    active_touches: HashMap<u64, TouchRole>,
    stick_displacement: Vec2, // normalized -1.0 .. 1.0
    cast_active: bool,
    cast_just_pressed: bool,
    cast_just_released: bool,
    jump_active: bool,
    jump_just_pressed: bool,
    jump_just_released: bool,
    interact_active: bool,
    sneak_active: bool,
    broom_boost_active: bool,
    broom_brake_active: bool,
    menu_triggered: bool,
    console_triggered: bool,
    accumulated_look_delta: Vec2,
}

impl TouchController {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn reset_frame_triggers(&mut self) {
        self.cast_just_pressed = false;
        self.cast_just_released = false;
        self.jump_just_pressed = false;
        self.jump_just_released = false;
        self.menu_triggered = false;
        self.console_triggered = false;
        self.accumulated_look_delta = Vec2::ZERO;
    }

    pub fn handle_touch(
        &mut self,
        touch: &Touch,
        screen_size: [f32; 2],
        settings: &TouchSettings,
        editor: &mut TouchEditorState,
    ) {
        if !settings.enabled {
            return;
        }

        let pos = Pos2::new(touch.location.x as f32, touch.location.y as f32);

        if editor.is_active {
            self.handle_editor_touch(touch, pos, screen_size, settings, editor);
            return;
        }

        match touch.phase {
            TouchPhase::Started => {
                let role = self.hit_test(pos, screen_size, settings);
                self.activate_role(touch.id, role, pos);
            }
            TouchPhase::Moved => {
                if let Some(role) = self.active_touches.get_mut(&touch.id) {
                    match role {
                        TouchRole::Stick => {
                            let center = Pos2::new(
                                settings.stick.x * screen_size[0],
                                settings.stick.y * screen_size[1],
                            );
                            let delta = pos - center;
                            let radius = settings.stick.radius.max(10.0);
                            let dist = delta.length();
                            if dist > 0.0 {
                                let clamped_dist = dist.min(radius);
                                self.stick_displacement = (delta / dist) * (clamped_dist / radius);
                            } else {
                                self.stick_displacement = Vec2::ZERO;
                            }
                        }
                        TouchRole::Look { last_pos } => {
                            let delta = pos - *last_pos;
                            *last_pos = pos;
                            self.accumulated_look_delta += delta;
                        }
                        TouchRole::Button(_) => {}
                    }
                }
            }
            TouchPhase::Ended | TouchPhase::Cancelled => {
                if let Some(role) = self.active_touches.remove(&touch.id) {
                    self.deactivate_role(role);
                }
            }
        }
    }

    fn hit_test(
        &self,
        pos: Pos2,
        screen_size: [f32; 2],
        settings: &TouchSettings,
    ) -> TouchRole {
        let buttons = [
            (TouchElementKind::Menu, settings.menu.visible, settings.element_rect(TouchElementKind::Menu, screen_size)),
            (TouchElementKind::Console, settings.console.visible, settings.element_rect(TouchElementKind::Console, screen_size)),
            (TouchElementKind::Cast, settings.cast.visible, settings.element_rect(TouchElementKind::Cast, screen_size)),
            (TouchElementKind::Jump, settings.jump.visible, settings.element_rect(TouchElementKind::Jump, screen_size)),
            (TouchElementKind::Interact, settings.interact.visible, settings.element_rect(TouchElementKind::Interact, screen_size)),
            (TouchElementKind::Sneak, settings.sneak.visible, settings.element_rect(TouchElementKind::Sneak, screen_size)),
            (TouchElementKind::BroomBoost, settings.broom_boost.visible, settings.element_rect(TouchElementKind::BroomBoost, screen_size)),
            (TouchElementKind::BroomBrake, settings.broom_brake.visible, settings.element_rect(TouchElementKind::BroomBrake, screen_size)),
        ];

        for (kind, visible, rect) in buttons {
            if visible && rect.contains(pos) {
                return TouchRole::Button(kind);
            }
        }

        if settings.stick.visible {
            let stick_rect = settings.element_rect(TouchElementKind::Stick, screen_size);
            if stick_rect.contains(pos) || pos.x < screen_size[0] * 0.4 && pos.y > screen_size[1] * 0.4 {
                return TouchRole::Stick;
            }
        }

        TouchRole::Look { last_pos: pos }
    }

    fn activate_role(&mut self, id: u64, role: TouchRole, pos: Pos2) {
        match role {
            TouchRole::Stick => {
                self.active_touches.insert(id, TouchRole::Stick);
            }
            TouchRole::Look { .. } => {
                self.active_touches.insert(id, TouchRole::Look { last_pos: pos });
            }
            TouchRole::Button(kind) => {
                self.active_touches.insert(id, TouchRole::Button(kind));
                match kind {
                    TouchElementKind::Cast => {
                        self.cast_active = true;
                        self.cast_just_pressed = true;
                    }
                    TouchElementKind::Jump => {
                        self.jump_active = true;
                        self.jump_just_pressed = true;
                    }
                    TouchElementKind::Interact => {
                        self.interact_active = true;
                    }
                    TouchElementKind::Sneak => {
                        self.sneak_active = true;
                    }
                    TouchElementKind::BroomBoost => {
                        self.broom_boost_active = true;
                    }
                    TouchElementKind::BroomBrake => {
                        self.broom_brake_active = true;
                    }
                    TouchElementKind::Menu => {
                        self.menu_triggered = true;
                    }
                    TouchElementKind::Console => {
                        self.console_triggered = true;
                    }
                    TouchElementKind::Stick => {}
                }
            }
        }
    }

    fn deactivate_role(&mut self, role: TouchRole) {
        match role {
            TouchRole::Stick => {
                self.stick_displacement = Vec2::ZERO;
            }
            TouchRole::Look { .. } => {}
            TouchRole::Button(kind) => match kind {
                TouchElementKind::Cast => {
                    self.cast_active = false;
                    self.cast_just_released = true;
                }
                TouchElementKind::Jump => {
                    self.jump_active = false;
                    self.jump_just_released = true;
                }
                TouchElementKind::Interact => self.interact_active = false,
                TouchElementKind::Sneak => self.sneak_active = false,
                TouchElementKind::BroomBoost => self.broom_boost_active = false,
                TouchElementKind::BroomBrake => self.broom_brake_active = false,
                _ => {}
            },
        }
    }

    fn handle_editor_touch(
        &mut self,
        touch: &Touch,
        pos: Pos2,
        screen_size: [f32; 2],
        settings: &TouchSettings,
        editor: &mut TouchEditorState,
    ) {
        match touch.phase {
            TouchPhase::Started => {
                for &kind in &TouchElementKind::ALL {
                    let rect = settings.element_rect(kind, screen_size);
                    if rect.contains(pos) {
                        editor.selected_element = Some(kind);
                        editor.dragging = true;
                        editor.drag_offset = rect.center() - pos;
                        editor.needs_save = true;
                        return;
                    }
                }
            }
            TouchPhase::Moved => {
                if editor.dragging {
                    if let Some(kind) = editor.selected_element {
                        let target_pos = pos + editor.drag_offset;
                        let norm = Pos2::new(
                            (target_pos.x / screen_size[0]).clamp(0.02, 0.98),
                            (target_pos.y / screen_size[1]).clamp(0.02, 0.98),
                        );
                        // We will update settings in editor UI loop
                        editor.needs_save = true;
                        let _ = (kind, norm);
                    }
                }
            }
            TouchPhase::Ended | TouchPhase::Cancelled => {
                editor.dragging = false;
            }
        }
    }

    pub fn apply_to_player_input(
        &self,
        input: &mut PlayerInput,
        _delta_time: f32,
        settings: &TouchSettings,
    ) {
        if !settings.enabled {
            return;
        }

        if self.stick_displacement.length_sq() > 0.01 {
            let forward = -self.stick_displacement.y;
            let strafe = self.stick_displacement.x;

            let walk_multiplier = if self.sneak_active { 0.4 } else { 1.0 };

            if forward > 0.0 {
                input.base_y += forward * 6_000.0 * walk_multiplier;
            } else {
                input.base_y += forward * 3_000.0 * walk_multiplier;
            }

            input.strafe += strafe * 6_000.0 * walk_multiplier;
            input.base_x += strafe * 3_000.0 * walk_multiplier;

            if forward > 0.3 {
                input.broom_pitch_up = true;
            } else if forward < -0.3 {
                input.broom_pitch_down = true;
            }
        }

        if self.cast_active {
            input.alt_fire = true;
        }
        if self.cast_just_pressed {
            input.alt_fire_pressed = true;
        }
        if self.cast_just_released {
            input.alt_fire_released = true;
        }

        if self.jump_active {
            input.jump = true;
        }
        if self.jump_just_pressed {
            input.space_pressed = true;
            input.jump = true;
        }
        if self.jump_just_released {
            input.space_released = true;
        }

        if self.broom_boost_active {
            input.broom_boost = true;
        }
        if self.broom_brake_active {
            input.broom_brake = true;
        }

        let sensitivity = settings.look_sensitivity * 12.0;
        input.mouse_x += self.accumulated_look_delta.x * sensitivity;
        input.mouse_y -= self.accumulated_look_delta.y * sensitivity;
    }

    pub fn menu_requested(&self) -> bool {
        self.menu_triggered
    }

    pub fn console_requested(&self) -> bool {
        self.console_triggered
    }

    pub fn render(
        &mut self,
        context: &egui::Context,
        screen_size: [f32; 2],
        settings: &mut TouchSettings,
        editor: &mut TouchEditorState,
    ) {
        if !settings.enabled {
            return;
        }

        let screen_rect = Rect::from_min_size(Pos2::ZERO, Vec2::new(screen_size[0], screen_size[1]));
        let painter = context.layer_painter(LayerId::new(Order::Foreground, Id::new("touch overlay")));

        let alpha = (settings.opacity * 255.0).clamp(20.0, 255.0) as u8;

        if editor.is_active {
            self.render_editor_overlay(context, screen_size, settings, editor);
            return;
        }

        // 1. Render Move Joystick
        if settings.stick.visible {
            let center = Pos2::new(settings.stick.x * screen_size[0], settings.stick.y * screen_size[1]);
            let radius = settings.stick.radius;

            painter.circle_filled(center, radius, Color32::from_rgba_unmultiplied(20, 24, 38, alpha / 2));
            painter.circle_stroke(center, radius, Stroke::new(2.5, Color32::from_rgba_unmultiplied(220, 195, 120, alpha)));

            // Outer guide marks
            for angle in [0.0f32, std::f32::consts::FRAC_PI_2, std::f32::consts::PI, 3.0 * std::f32::consts::FRAC_PI_2] {
                let dir = Vec2::new(angle.cos(), angle.sin());
                painter.line_segment(
                    [center + dir * (radius - 8.0), center + dir * (radius + 2.0)],
                    Stroke::new(2.0, Color32::from_rgba_unmultiplied(240, 220, 150, alpha)),
                );
            }

            // Knob position
            let knob_pos = center + self.stick_displacement * radius;
            let knob_radius = radius * 0.42;
            let knob_color = if self.stick_displacement.length_sq() > 0.05 {
                Color32::from_rgba_unmultiplied(255, 215, 0, alpha)
            } else {
                Color32::from_rgba_unmultiplied(180, 190, 215, alpha)
            };

            painter.circle_filled(knob_pos, knob_radius, knob_color);
            painter.circle_stroke(knob_pos, knob_radius, Stroke::new(2.0, Color32::from_rgba_unmultiplied(255, 255, 255, alpha)));
        }

        // 2. Render Action Buttons
        let buttons = [
            (TouchElementKind::Cast, "CAST", settings.cast, self.cast_active, Color32::from_rgb(255, 60, 60)),
            (TouchElementKind::Jump, "JUMP", settings.jump, self.jump_active, Color32::from_rgb(60, 160, 255)),
            (TouchElementKind::Interact, "USE", settings.interact, self.interact_active, Color32::from_rgb(80, 230, 120)),
            (TouchElementKind::Sneak, "WALK", settings.sneak, self.sneak_active, Color32::from_rgb(210, 140, 240)),
            (TouchElementKind::BroomBoost, "BOOST", settings.broom_boost, self.broom_boost_active, Color32::from_rgb(255, 180, 30)),
            (TouchElementKind::BroomBrake, "BRAKE", settings.broom_brake, self.broom_brake_active, Color32::from_rgb(255, 100, 100)),
            (TouchElementKind::Menu, "MENU", settings.menu, false, Color32::from_rgb(180, 180, 200)),
            (TouchElementKind::Console, "~", settings.console, false, Color32::from_rgb(150, 180, 220)),
        ];

        for (_kind, label, btn, active, accent) in buttons {
            if !btn.visible {
                continue;
            }
            let center = Pos2::new(btn.x * screen_size[0], btn.y * screen_size[1]);
            let r = btn.size;

            let (bg_color, stroke_color, text_color) = if active {
                (
                    accent.gamma_multiply(0.85),
                    Color32::WHITE,
                    Color32::WHITE,
                )
            } else {
                (
                    Color32::from_rgba_unmultiplied(20, 25, 40, alpha / 2),
                    Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), alpha),
                    Color32::from_rgba_unmultiplied(240, 240, 250, alpha),
                )
            };

            painter.circle_filled(center, r, bg_color);
            painter.circle_stroke(center, r, Stroke::new(2.5, stroke_color));

            let font_size = (r * 0.46).clamp(10.0, 22.0);
            painter.text(
                center,
                Align2::CENTER_CENTER,
                label,
                FontId::proportional(font_size),
                text_color,
            );
        }

        // 3. Subtle look zone guide if visible
        if settings.look_visible {
            let look_rect = Rect::from_min_max(
                Pos2::new(screen_size[0] * 0.45, 0.0),
                Pos2::new(screen_size[0], screen_size[1]),
            );
            painter.rect_stroke(
                look_rect.shrink(8.0),
                CornerRadius::same(12),
                Stroke::new(1.0, Color32::from_rgba_unmultiplied(100, 140, 200, (alpha / 8).max(10))),
                StrokeKind::Inside,
            );
        }

        let _ = screen_rect;
    }

    fn render_editor_overlay(
        &mut self,
        context: &egui::Context,
        screen_size: [f32; 2],
        settings: &mut TouchSettings,
        editor: &mut TouchEditorState,
    ) {
        let painter = context.layer_painter(LayerId::new(Order::Foreground, Id::new("touch editor painter")));

        // Dim background slightly to focus on editor
        let full_screen = Rect::from_min_size(Pos2::ZERO, Vec2::new(screen_size[0], screen_size[1]));
        painter.rect_filled(full_screen, 0.0, Color32::from_rgba_unmultiplied(10, 15, 25, 120));

        // Draw each element with drag handle and outline
        for &kind in &TouchElementKind::ALL {
            let rect = settings.element_rect(kind, screen_size);
            let center = rect.center();
            let radius = rect.width() / 2.0;
            let is_selected = editor.selected_element == Some(kind);

            let stroke = if is_selected {
                Stroke::new(3.5, Color32::from_rgb(255, 220, 40))
            } else {
                Stroke::new(2.0, Color32::from_rgba_unmultiplied(140, 190, 255, 180))
            };

            let fill = if is_selected {
                Color32::from_rgba_unmultiplied(80, 140, 240, 100)
            } else {
                Color32::from_rgba_unmultiplied(30, 40, 60, 120)
            };

            painter.circle_filled(center, radius, fill);
            painter.circle_stroke(center, radius, stroke);

            let label = kind.label();
            painter.text(
                center,
                Align2::CENTER_CENTER,
                label,
                FontId::proportional(13.0),
                Color32::WHITE,
            );

            // Visibility badge
            let visible = settings.is_element_visible(kind);
            let badge_color = if visible { Color32::GREEN } else { Color32::RED };
            painter.circle_filled(center + Vec2::new(radius * 0.7, -radius * 0.7), 6.0, badge_color);
        }

        // Top Control Toolbar
        egui::Area::new(Id::new("touch editor top bar"))
            .fixed_pos(Pos2::new(screen_size[0] * 0.1, 16.0))
            .order(Order::Tooltip)
            .show(context, |ui| {
                ui.set_max_width(screen_size[0] * 0.8);
                egui::Frame::window(ui.style())
                    .fill(Color32::from_rgba_unmultiplied(20, 24, 38, 235))
                    .stroke(Stroke::new(1.5, Color32::from_rgb(180, 150, 80)))
                    .corner_radius(CornerRadius::same(10))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.heading(egui::RichText::new("Touch Controls Editor").color(Color32::from_rgb(255, 215, 0)));
                            ui.separator();

                            if let Some(selected) = editor.selected_element {
                                ui.label(egui::RichText::new(format!("Selected: {}", selected.label())).strong());

                                let mut size = settings.element_size(selected);
                                ui.label("Size:");
                                if ui.add(egui::Slider::new(&mut size, 20.0..=100.0).text("px")).changed() {
                                    settings.set_element_size(selected, size);
                                    editor.needs_save = true;
                                }

                                let mut visible = settings.is_element_visible(selected);
                                if ui.checkbox(&mut visible, "Visible").changed() {
                                    settings.set_element_visible(selected, visible);
                                    editor.needs_save = true;
                                }
                            } else {
                                ui.label(egui::RichText::new("Tap any button to select, or drag to move").italics());
                            }

                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if ui.button(egui::RichText::new("Done / Save").color(Color32::GREEN).strong()).clicked() {
                                    editor.is_active = false;
                                    editor.dragging = false;
                                    editor.selected_element = None;
                                }

                                if ui.button("Reset Defaults").clicked() {
                                    settings.reset_defaults();
                                    editor.needs_save = true;
                                }
                            });
                        });
                    });
            });

        // Handle mouse/touch drag for editor in desktop or touch simulation
        let pointer = context.input(|i| i.pointer.clone());
        if let Some(pos) = pointer.latest_pos() {
            if pointer.primary_pressed() {
                for &kind in &TouchElementKind::ALL {
                    let rect = settings.element_rect(kind, screen_size);
                    if rect.contains(pos) {
                        editor.selected_element = Some(kind);
                        editor.dragging = true;
                        editor.drag_offset = rect.center() - pos;
                        break;
                    }
                }
            } else if pointer.primary_down() && editor.dragging {
                if let Some(kind) = editor.selected_element {
                    let new_center = pos + editor.drag_offset;
                    let norm = Pos2::new(
                        (new_center.x / screen_size[0]).clamp(0.02, 0.98),
                        (new_center.y / screen_size[1]).clamp(0.02, 0.98),
                    );
                    settings.set_element_pos(kind, norm);
                    editor.needs_save = true;
                }
            } else if pointer.primary_released() {
                editor.dragging = false;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn touch_settings_defaults_and_resets() {
        let mut settings = TouchSettings::default();
        settings.cast.x = 0.5;
        settings.cast.size = 80.0;
        settings.reset_defaults();

        assert_eq!(settings.cast.x, 0.88);
        assert_eq!(settings.cast.size, 46.0);
    }

    #[test]
    fn element_position_clamping() {
        let mut settings = TouchSettings::default();
        settings.set_element_pos(TouchElementKind::Cast, Pos2::new(-0.5, 2.0));
        assert_eq!(settings.cast.x, 0.02);
        assert_eq!(settings.cast.y, 0.98);
    }

    #[test]
    fn touch_controller_input_translation() {
        let mut controller = TouchController::new();
        let settings = TouchSettings {
            enabled: true,
            ..Default::default()
        };

        controller.stick_displacement = Vec2::new(0.5, -0.8);
        controller.cast_active = true;
        controller.jump_active = true;

        let mut input = PlayerInput::default();
        controller.apply_to_player_input(&mut input, 0.016, &settings);

        assert!(input.base_y > 1000.0);
        assert!(input.strafe > 1000.0);
        assert!(input.alt_fire);
        assert!(input.jump);
    }

    #[test]
    fn touch_controller_respects_disabled_state() {
        let mut controller = TouchController::new();
        let settings = TouchSettings {
            enabled: false,
            ..Default::default()
        };

        controller.cast_active = true;
        let mut input = PlayerInput::default();
        controller.apply_to_player_input(&mut input, 0.016, &settings);

        assert!(!input.alt_fire);
    }
}
