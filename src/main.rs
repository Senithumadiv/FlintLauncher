use eframe::egui;
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use rayon::prelude::*;
use reqwest;
use serde::Deserialize;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, Instant};
use std::env;

struct Theme {
    background: String,
    text_color: String,
    selection_bg: String,
    selection_text: String,
    border_color: String,
    font_size: f32,
    border_radius: f32,
    font_family: String,
    highlight_color: String,
    enable_icons: bool,
    icon_theme: String,
    icon_size: f32,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            background: "#ffffff".to_string(),
            text_color: "#333333".to_string(),
            selection_bg: "#007aff".to_string(),
            selection_text: "#ffffff".to_string(),
            border_color: "#e0e0e0".to_string(),
            font_size: 18.0,
            border_radius: 0.0,
            font_family: "System".to_string(),
            highlight_color: "#007aff".to_string(),
            enable_icons: true,
            icon_theme: "Papirus".to_string(),
            icon_size: 24.0,
        }
    }
}

impl Theme {
    fn load_from_config() -> Self {
        let config_dir = get_config_dir();
        let theme_path = config_dir.join("theme.conf");
        
        if !theme_path.exists() {
            create_default_theme(&theme_path);
            return Self::default();
        }
        
        let mut theme = Self::default();
        
        if let Ok(content) = fs::read_to_string(&theme_path) {
            for line in content.lines() {
                let parts: Vec<&str> = line.splitn(2, '=').collect();
                if parts.len() == 2 {
                    let key = parts[0].trim();
                    let value = parts[1].trim();
                    
                    match key {
                        "background" => theme.background = value.to_string(),
                        "text_color" => theme.text_color = value.to_string(),
                        "selection_bg" => theme.selection_bg = value.to_string(),
                        "selection_text" => theme.selection_text = value.to_string(),
                        "border_color" => theme.border_color = value.to_string(),
                        "highlight_color" => theme.highlight_color = value.to_string(),
                        "font_size" => {
                            if let Ok(size) = value.parse() {
                                theme.font_size = size;
                            }
                        }
                        "border_radius" => {
                            if let Ok(radius) = value.parse() {
                                theme.border_radius = radius;
                            }
                        }
                        "font_family" => theme.font_family = value.to_string(),
                        "enable_icons" => theme.enable_icons = value == "true" || value == "1",
                        "icon_theme" => theme.icon_theme = value.to_string(),
                        "icon_size" => {
                            if let Ok(size) = value.parse() {
                                theme.icon_size = size;
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
        
        theme
    }
    
    fn hex_to_rgb(&self, hex: &str) -> [f32; 3] {
        let hex = hex.trim_start_matches('#');
        if hex.len() == 6 {
            if let (Ok(r), Ok(g), Ok(b)) = (
                u8::from_str_radix(&hex[0..2], 16),
                u8::from_str_radix(&hex[2..4], 16),
                u8::from_str_radix(&hex[4..6], 16),
            ) {
                return [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0];
            }
        }
        [0.0, 0.0, 0.0]
    }
}

#[derive(Clone)]
enum ResultType {
    App(AppEntry),
    Calculator(String),
    Command(String),
    WebSearch(String),
    Url(String),
    File(PathBuf),
    Emoji(String, String),
    Currency(String, String, f64),
    Flatpak(FlatpakAppEntry),
}

#[derive(Clone)]
struct AppEntry {
    name: String,
    desktop_id: String,
    exec_command: String,
    match_indices: Vec<usize>,
    icon_path: Option<PathBuf>,
}

#[derive(Clone)]
struct FlatpakAppEntry {
    name: String,
    flatpak_id: String,
    description: String,
    match_indices: Vec<usize>,
    icon_path: Option<PathBuf>,
}

struct AnimationState {
    progress: f32,
    start_time: Instant,
    duration: Duration,
    animation_type: AnimationType,
}

impl AnimationState {
    fn new(duration: Duration, animation_type: AnimationType) -> Self {
        Self {
            progress: 0.0,
            start_time: Instant::now(),
            duration,
            animation_type,
        }
    }
    
    fn update(&mut self) -> bool {
        let elapsed = self.start_time.elapsed();
        self.progress = (elapsed.as_millis() as f32 / self.duration.as_millis() as f32).min(1.0);
        self.progress < 1.0
    }
    
    fn ease_out(&self) -> f32 {
        1.0 - (1.0 - self.progress).powf(2.0)
    }
    
    fn ease_in_out(&self) -> f32 {
        if self.progress < 0.5 {
            2.0 * self.progress * self.progress
        } else {
            let x = -2.0 * self.progress + 2.0;
            1.0 - (x * x) / 2.0
        }
    }
    
    fn ease_out_back(&self) -> f32 {
        let c1 = 1.70158;
        let c3 = c1 + 1.0;
        1.0 + c3 * (self.progress - 1.0).powf(3.0) + c1 * (self.progress - 1.0).powf(2.0)
    }
    
    fn ease_out_bounce(&self) -> f32 {
        let n1 = 7.5625;
        let d1 = 2.75;

        if self.progress < 1.0 / d1 {
            n1 * self.progress * self.progress
        } else if self.progress < 2.0 / d1 {
            let x = self.progress - 1.5 / d1;
            n1 * x * x + 0.75
        } else if self.progress < 2.5 / d1 {
            let x = self.progress - 2.25 / d1;
            n1 * x * x + 0.9375
        } else {
            let x = self.progress - 2.625 / d1;
            n1 * x * x + 0.984375
        }
    }
}

#[derive(Clone, Copy)]
enum AnimationType {
    FadeIn,
    ScaleIn,
    SlideDown,
    BounceDown,
}

struct FlintApp {
    query: String,
    results: Vec<ResultType>,
    items: Vec<AppEntry>,
    flatpak_items: Vec<FlatpakAppEntry>,
    selected: usize,
    should_close: bool,
    has_focused: bool,
    theme: Theme,
    _lock_file: File,
    window_animation: AnimationState,
    result_animations: Vec<AnimationState>,
    runtime: tokio::runtime::Runtime,
}

impl FlintApp {
    fn new() -> Result<Self, String> {
        let lock_file = acquire_lock()?;
        let items = scan_desktop_apps();
        let flatpak_items = scan_flatpak_apps();
        let runtime = tokio::runtime::Runtime::new()
            .map_err(|e| format!("Failed to create async runtime: {}", e))?;
        
        Ok(Self {
            query: String::new(),
            results: Vec::new(),
            items,
            flatpak_items,
            selected: 0,
            should_close: false,
            has_focused: false,
            theme: Theme::load_from_config(),
            _lock_file: lock_file,
            window_animation: AnimationState::new(Duration::from_millis(300), AnimationType::FadeIn),
            result_animations: Vec::new(),
            runtime,
        })
    }
    
    fn update_result_animations(&mut self) {
        if self.result_animations.len() != self.results.len() {
            self.result_animations = self.results.iter()
                .enumerate()
                .map(|(i, _)| {
                    let delay = Duration::from_millis((i * 40) as u64).min(Duration::from_millis(200));
                    let mut anim = AnimationState::new(Duration::from_millis(250), AnimationType::SlideDown);
                    anim.start_time += delay;
                    anim
                })
                .collect();
        }
        
        for anim in &mut self.result_animations {
            anim.update();
        }
    }
    
    fn get_result_offset(&self, index: usize) -> f32 {
        self.result_animations.get(index)
            .map(|anim| {
                match anim.animation_type {
                    AnimationType::SlideDown => (1.0 - anim.ease_out()) * -30.0,
                    AnimationType::BounceDown => (1.0 - anim.ease_out_bounce()) * -40.0,
                    _ => 0.0,
                }
            })
            .unwrap_or(0.0)
    }
    
    fn get_result_alpha(&self, index: usize) -> f32 {
        self.result_animations.get(index)
            .map(|anim| {
                match anim.animation_type {
                    AnimationType::SlideDown | AnimationType::BounceDown => anim.ease_out(),
                    _ => anim.ease_out(),
                }
            })
            .unwrap_or(1.0)
    }
}

impl eframe::App for FlintApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let _window_animating = self.window_animation.update();
        self.update_result_animations();
        
        let still_animating = self.window_animation.progress < 1.0 || self.result_animations.iter().any(|a| a.progress < 1.0);
        
        if still_animating {
            ctx.request_repaint();
        }
        
        if self.should_close {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }

        let window_alpha = self.window_animation.ease_out();
        
        let window_width = 600.0;
        let search_box_height = 50.0;
        let result_item_height = 44.0;
        let max_visible_results = 8;
        let visible_results = self.results.len().min(max_visible_results);
        let results_height = if visible_results > 0 {
            (visible_results as f32 * result_item_height) + 10.0
        } else {
            0.0
        };
        let total_height = search_box_height + results_height;
        
        ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(
            window_width,
            total_height
        )));

        let bg_rgb = self.theme.hex_to_rgb(&self.theme.background);
        let border_rgb = self.theme.hex_to_rgb(&self.theme.border_color);
        
        let bg_color = egui::Color32::from_rgba_premultiplied(
            (bg_rgb[0] * 255.0 * window_alpha) as u8,
            (bg_rgb[1] * 255.0 * window_alpha) as u8,
            (bg_rgb[2] * 255.0 * window_alpha) as u8,
            (window_alpha * 255.0) as u8,
        );
        
        let border_color = egui::Color32::from_rgba_premultiplied(
            (border_rgb[0] * 255.0 * window_alpha) as u8,
            (border_rgb[1] * 255.0 * window_alpha) as u8,
            (border_rgb[2] * 255.0 * window_alpha) as u8,
            (window_alpha * 255.0) as u8,
        );
        
        egui::CentralPanel::default()
            .frame(egui::Frame::none()
                .fill(bg_color)
                .stroke(egui::Stroke::new(1.0, border_color))
                .rounding(self.theme.border_radius)
                .shadow(egui::epaint::Shadow {
                    offset: egui::vec2(0.0, 2.0),
                    blur: 8.0,
                    spread: 0.0,
                    color: egui::Color32::from_rgba_premultiplied(0, 0, 0, (50.0 * window_alpha) as u8),
                }))
            .show(ctx, |ui| {
                
            ui.set_min_width(window_width);
            ui.set_max_width(window_width);
                
            ui.vertical(|ui| {
                let text_rgb = self.theme.hex_to_rgb(&self.theme.text_color);
                
                ui.add_space(5.0);
                ui.add_space(5.0);
                
                // Full opacity for text to ensure visibility
                let search_text_color = egui::Color32::from_rgba_premultiplied(
                    (text_rgb[0] * 255.0) as u8,
                    (text_rgb[1] * 255.0) as u8,
                    (text_rgb[2] * 255.0) as u8,
                    255,
                );
                
                ui.horizontal(|ui| {
                    ui.add_space(15.0);
                    
                    let text_edit = egui::TextEdit::singleline(&mut self.query)
                        .hint_text("Search...")
                        .frame(false)
                        .text_color(search_text_color)
                        .font(egui::FontId::proportional(20.0))
                        .id(egui::Id::new("search_field"))
                        .desired_width(window_width - 30.0)
                        .min_size(egui::vec2(window_width - 30.0, 30.0));

                    let response = ui.add(text_edit);

                    if !self.has_focused {
                        ui.ctx().memory_mut(|mem| mem.request_focus(response.id));
                        self.has_focused = true;
                    }
                    
                    ui.add_space(15.0);
                });
                
                if !self.results.is_empty() {
                    ui.add_space(5.0);
                    let separator_alpha = (window_alpha * 255.0) as u8;
                    let border_rgb = self.theme.hex_to_rgb(&self.theme.border_color);
                    let separator_color = egui::Color32::from_rgba_premultiplied(
                        (border_rgb[0] * 255.0) as u8,
                        (border_rgb[1] * 255.0) as u8,
                        (border_rgb[2] * 255.0) as u8,
                        separator_alpha
                    );
                    
                    let separator_height = 1.0;
                    let available_width = ui.available_width();
                    let separator_rect = egui::Rect::from_min_size(
                        ui.cursor().min,
                        egui::vec2(available_width, separator_height)
                    );
                    ui.painter().rect_filled(separator_rect, 0.0, separator_color);
                    
                    ui.add_space(separator_height + 5.0);
                }

                if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                    self.should_close = true;
                }

                if ui.input(|i| i.key_pressed(egui::Key::ArrowDown)) && !self.results.is_empty() {
                    self.selected = (self.selected + 1) % self.results.len();
                }

                if ui.input(|i| i.key_pressed(egui::Key::ArrowUp)) && !self.results.is_empty() {
                    if self.selected == 0 {
                        self.selected = self.results.len() - 1;
                    } else {
                        self.selected -= 1;
                    }
                }

                if ui.input(|i| i.key_pressed(egui::Key::Enter)) && !self.results.is_empty() {
                    if let Some(result) = self.results.get(self.selected) {
                        match result {
                            ResultType::App(app) => {
                                launch_app(&app.desktop_id);
                                self.should_close = true;
                            }
                            ResultType::Flatpak(app) => {
                                launch_flatpak_app(&app.flatpak_id);
                                self.should_close = true;
                            }
                            ResultType::Calculator(result) => {
                                copy_to_clipboard(result);
                                self.should_close = true;
                            }
                            ResultType::Command(cmd) => {
                                execute_command(cmd);
                                self.should_close = true;
                            }
                            ResultType::WebSearch(query) => {
                                open_web_search(query);
                                self.should_close = true;
                            }
                            ResultType::Url(url) => {
                                open_url(&url);
                                self.should_close = true;
                            }
                            ResultType::File(path) => {
                                open_file(&path);
                                self.should_close = true;
                            }
                            ResultType::Emoji(_, emoji) => {
                                copy_to_clipboard(&emoji);
                                self.should_close = true;
                            }
                            ResultType::Currency(_, _, result) => {
                                copy_to_clipboard(&result.to_string());
                                self.should_close = true;
                            }
                        }
                    }
                }

                self.results.clear();

                if !self.query.is_empty() {
                    if self.query.to_lowercase().starts_with("file:") {
                        let file_query = &self.query[5..].trim().to_lowercase();
                        if !file_query.is_empty() {
                            let file_results = search_files(file_query);
                            for path in file_results {
                                self.results.push(ResultType::File(path));
                            }
                        } else {
                            self.results.push(ResultType::Command("Search files...".to_string()));
                        }
                    }
                    else if self.query.to_lowercase().starts_with("e:") {
                        let emoji_query = &self.query[2..].trim().to_lowercase();
                        if !emoji_query.is_empty() {
                            let emoji_results = search_emojis(emoji_query);
                            for (name, emoji) in emoji_results {
                                self.results.push(ResultType::Emoji(name, emoji));
                            }
                        } else {
                            self.results.push(ResultType::Command("Search emojis...".to_string()));
                        }
                    }
                    else if let Some((from, to, result)) = self.runtime.block_on(convert_currency_online(&self.query)) {
                        self.results.push(ResultType::Currency(from, to, result));
                    }
                    else if looks_like_url(&self.query) {
                        let url = if self.query.contains("://") {
                            self.query.clone()
                        } else {
                            format!("https://{}", self.query)
                        };
                        self.results.push(ResultType::Url(url));
                    }
                    else if is_calculation(&self.query) {
                        let expr = self.query.trim();
                        if !expr.is_empty() {
                            match meval::eval_str(expr) {
                                Ok(result) => {
                                    self.results.push(ResultType::Calculator(result.to_string()));
                                }
                                Err(_) => {}
                            }
                        }
                    }
                    else if self.query.starts_with('$') {
                        let cmd = &self.query[1..].trim();
                        if !cmd.is_empty() {
                            self.results.push(ResultType::Command(cmd.to_string()));
                        } else {
                            self.results.push(ResultType::Command("Enter shell command...".to_string()));
                        }
                    }
                    else if self.query.starts_with('@') {
                        let search = &self.query[1..].trim();
                        if !search.is_empty() {
                            self.results.push(ResultType::WebSearch(search.to_string()));
                        } else {
                            self.results.push(ResultType::Command("Search the web...".to_string()));
                        }
                    }
                    
                    if self.results.is_empty() {
                        let matcher = SkimMatcherV2::default();
                        let query = self.query.to_lowercase();
                        
                        let mut scored_results: Vec<(i64, AppEntry)> = self
                            .items
                            .par_iter()
                            .filter_map(|app| {
                                if let Some((score, indices)) = matcher.fuzzy_indices(&app.name.to_lowercase(), &query) {
                                    let mut app_with_match = app.clone();
                                    app_with_match.match_indices = indices;
                                    return Some((score + 100, app_with_match));
                                }
                                
                                if let Some((score, _)) = matcher.fuzzy_indices(&app.exec_command, &query) {
                                    let mut app_with_match = app.clone();
                                    app_with_match.match_indices = Vec::new();
                                    return Some((score, app_with_match));
                                }
                                
                                None
                            })
                            .collect();
                        
                        let mut flatpak_scored_results: Vec<(i64, FlatpakAppEntry)> = self
                            .flatpak_items
                            .par_iter()
                            .filter_map(|app| {
                                if let Some((score, indices)) = matcher.fuzzy_indices(&app.name.to_lowercase(), &query) {
                                    let mut app_with_match = app.clone();
                                    app_with_match.match_indices = indices;
                                    return Some((score + 50, app_with_match));
                                }
                                
                                if let Some((score, _)) = matcher.fuzzy_indices(&app.description.to_lowercase(), &query) {
                                    let mut app_with_match = app.clone();
                                    app_with_match.match_indices = Vec::new();
                                    return Some((score - 20, app_with_match));
                                }
                                
                                if let Some((score, _)) = matcher.fuzzy_indices(&app.flatpak_id.to_lowercase(), &query) {
                                    let mut app_with_match = app.clone();
                                    app_with_match.match_indices = Vec::new();
                                    return Some((score, app_with_match));
                                }
                                
                                None
                            })
                            .collect();
                        
                        scored_results.sort_by(|a, b| b.0.cmp(&a.0));
                        flatpak_scored_results.sort_by(|a, b| b.0.cmp(&a.0));
                        
                        let mut all_results: Vec<(i64, ResultType)> = Vec::new();
                        
                        for (score, app) in scored_results {
                            all_results.push((score, ResultType::App(app)));
                        }
                        
                        for (score, app) in flatpak_scored_results {
                            all_results.push((score, ResultType::Flatpak(app)));
                        }
                        
                        all_results.sort_by(|a, b| b.0.cmp(&a.0));
                        
                        for (_, result) in all_results.into_iter().take(max_visible_results) {
                            self.results.push(result);
                        }
                        
                        if self.results.is_empty() {
                            self.results.push(ResultType::WebSearch(query));
                        }
                    }
                    
                    if self.selected >= self.results.len() && !self.results.is_empty() {
                        self.selected = 0;
                    }
                }

                if !self.results.is_empty() {
                    egui::ScrollArea::vertical()
                        .max_height(result_item_height * max_visible_results as f32)
                        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                        .show(ui, |ui| {
                            
                            for (i, result) in self.results.iter().enumerate() {
                                let is_selected = i == self.selected;
                                let item_alpha = self.get_result_alpha(i);
                                let item_offset = self.get_result_offset(i);
                                
                                let sel_bg_rgb = self.theme.hex_to_rgb(&self.theme.selection_bg);
                                let text_rgb = self.theme.hex_to_rgb(&self.theme.text_color);
                                let sel_text_rgb = self.theme.hex_to_rgb(&self.theme.selection_text);
                                
                                let item_bg = if is_selected {
                                    egui::Color32::from_rgba_premultiplied(
                                        (sel_bg_rgb[0] * 255.0 * item_alpha) as u8,
                                        (sel_bg_rgb[1] * 255.0 * item_alpha) as u8,
                                        (sel_bg_rgb[2] * 255.0 * item_alpha) as u8,
                                        (item_alpha * 255.0) as u8,
                                    )
                                } else {
                                    egui::Color32::TRANSPARENT
                                };
                                
                                ui.add_space(item_offset);
                                
                                let item_frame = egui::Frame::none()
                                    .fill(item_bg)
                                    .inner_margin(egui::Margin::symmetric(15.0, 8.0));
                                
                                let response = item_frame.show(ui, |ui| {
                                    ui.set_min_height(result_item_height - 16.0);
                                    ui.set_width(window_width);
                                    
                                    ui.horizontal(|ui| {
                                        match result {
                                            ResultType::App(app) => {
                                                if self.theme.enable_icons {
                                                    if let Some(ref icon_path) = app.icon_path {
                                                        if let Ok(icon_data) = fs::read(icon_path) {
                                                            if let Some(texture) = load_icon_texture(ctx, icon_path, &icon_data) {
                                                                let icon_size = self.theme.icon_size;
                                                                let texture_id = texture.clone();
                                                                let image = egui::Image::new(&texture_id).fit_to_exact_size(egui::vec2(icon_size, icon_size));
                                                                ui.add(image);
                                                                ui.add_space(8.0);
                                                            }
                                                        }
                                                    } else {
                                                        ui.add_space(self.theme.icon_size + 8.0);
                                                    }
                                                }
                                                render_highlighted_text(
                                                    ui,
                                                    &app.name,
                                                    &app.match_indices,
                                                    is_selected,
                                                    &self.theme,
                                                    item_alpha,
                                                );
                                            }
                                            ResultType::Flatpak(app) => {
                                                if self.theme.enable_icons {
                                                    if let Some(ref icon_path) = app.icon_path {
                                                        if let Ok(icon_data) = fs::read(icon_path) {
                                                            if let Some(texture) = load_icon_texture(ctx, icon_path, &icon_data) {
                                                                let icon_size = self.theme.icon_size;
                                                                let texture_id = texture.clone();
                                                                let image = egui::Image::new(&texture_id).fit_to_exact_size(egui::vec2(icon_size, icon_size));
                                                                ui.add(image);
                                                                ui.add_space(8.0);
                                                            }
                                                        }
                                                    } else {
                                                        ui.add_space(self.theme.icon_size + 8.0);
                                                    }
                                                }
                                                ui.label(
                                                    egui::RichText::new("Flatpak:")
                                                        .color(egui::Color32::from_rgba_premultiplied(
                                                            (text_rgb[0] * 255.0 * item_alpha) as u8,
                                                            (text_rgb[1] * 255.0 * item_alpha) as u8,
                                                            (text_rgb[2] * 255.0 * item_alpha) as u8,
                                                            (item_alpha * 255.0) as u8,
                                                        ))
                                                        .size(self.theme.font_size)
                                                );
                                                render_highlighted_text(
                                                    ui,
                                                    &app.name,
                                                    &app.match_indices,
                                                    is_selected,
                                                    &self.theme,
                                                    item_alpha,
                                                );
                                            }
                                            ResultType::Calculator(res) => {
                                                let color = if is_selected { sel_text_rgb } else { text_rgb };
                                                ui.label(
                                                    egui::RichText::new(format!("🧮 {} = {}", self.query, res))
                                                        .color(egui::Color32::from_rgba_premultiplied(
                                                            (color[0] * 255.0 * item_alpha) as u8,
                                                            (color[1] * 255.0 * item_alpha) as u8,
                                                            (color[2] * 255.0 * item_alpha) as u8,
                                                            (item_alpha * 255.0) as u8,
                                                        ))
                                                        .size(self.theme.font_size)
                                                );
                                            }
                                            ResultType::Command(cmd) => {
                                                let color = if is_selected { sel_text_rgb } else { text_rgb };
                                                ui.label(
                                                    egui::RichText::new(format!("💻 {}", cmd))
                                                        .color(egui::Color32::from_rgba_premultiplied(
                                                            (color[0] * 255.0 * item_alpha) as u8,
                                                            (color[1] * 255.0 * item_alpha) as u8,
                                                            (color[2] * 255.0 * item_alpha) as u8,
                                                            (item_alpha * 255.0) as u8,
                                                        ))
                                                        .size(self.theme.font_size)
                                                );
                                            }
                                            ResultType::WebSearch(query) => {
                                                let color = if is_selected { sel_text_rgb } else { text_rgb };
                                                ui.label(
                                                    egui::RichText::new(format!("🔍 Search DuckDuckGo : {} ", query))
                                                        .color(egui::Color32::from_rgba_premultiplied(
                                                            (color[0] * 255.0 * item_alpha) as u8,
                                                            (color[1] * 255.0 * item_alpha) as u8,
                                                            (color[2] * 255.0 * item_alpha) as u8,
                                                            (item_alpha * 255.0) as u8,
                                                        ))
                                                        .size(self.theme.font_size)
                                                );
                                            }
                                            ResultType::Url(url) => {
                                                let color = if is_selected { sel_text_rgb } else { text_rgb };
                                                ui.label(
                                                    egui::RichText::new(format!("🌐 Open: {}", url))
                                                        .color(egui::Color32::from_rgba_premultiplied(
                                                            (color[0] * 255.0 * item_alpha) as u8,
                                                            (color[1] * 255.0 * item_alpha) as u8,
                                                            (color[2] * 255.0 * item_alpha) as u8,
                                                            (item_alpha * 255.0) as u8,
                                                        ))
                                                        .size(self.theme.font_size)
                                                );
                                            }
                                            ResultType::File(path) => {
                                                let color = if is_selected { sel_text_rgb } else { text_rgb };
                                                let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("Unknown");
                                                let parent_dir = path.parent()
                                                    .and_then(|p| p.file_name())
                                                    .and_then(|n| n.to_str())
                                                    .unwrap_or("");
                                                ui.label(
                                                    egui::RichText::new(format!("📄 {} ({})", file_name, parent_dir))
                                                        .color(egui::Color32::from_rgba_premultiplied(
                                                            (color[0] * 255.0 * item_alpha) as u8,
                                                            (color[1] * 255.0 * item_alpha) as u8,
                                                            (color[2] * 255.0 * item_alpha) as u8,
                                                            (item_alpha * 255.0) as u8,
                                                        ))
                                                        .size(self.theme.font_size)
                                                );
                                            }
                                            ResultType::Emoji(name, emoji) => {
                                                let color = if is_selected { sel_text_rgb } else { text_rgb };
                                                ui.label(
                                                    egui::RichText::new(format!("{} :{}", emoji, name))
                                                        .color(egui::Color32::from_rgba_premultiplied(
                                                            (color[0] * 255.0 * item_alpha) as u8,
                                                            (color[1] * 255.0 * item_alpha) as u8,
                                                            (color[2] * 255.0 * item_alpha) as u8,
                                                            (item_alpha * 255.0) as u8,
                                                        ))
                                                        .size(self.theme.font_size)
                                                );
                                            }
                                            ResultType::Currency(from, to, result) => {
                                                let color = if is_selected { sel_text_rgb } else { text_rgb };
                                                ui.label(
                                                    egui::RichText::new(format!("💱 {} {} = {:.2} {} (Live)", self.query, from, result, to))
                                                        .color(egui::Color32::from_rgba_premultiplied(
                                                            (color[0] * 255.0 * item_alpha) as u8,
                                                            (color[1] * 255.0 * item_alpha) as u8,
                                                            (color[2] * 255.0 * item_alpha) as u8,
                                                            (item_alpha * 255.0) as u8,
                                                        ))
                                                        .size(self.theme.font_size)
                                                );
                                            }
                                        }
                                    });
                                }).response;
                                
                                if is_selected {
                                    response.scroll_to_me(Some(egui::Align::Center));
                                }
                                
                                if response.clicked() {
                                    match result {
                                        ResultType::App(app) => {
                                            launch_app(&app.desktop_id);
                                            self.should_close = true;
                                        }
                                        ResultType::Flatpak(app) => {
                                            launch_flatpak_app(&app.flatpak_id);
                                            self.should_close = true;
                                        }
                                        ResultType::Calculator(res) => {
                                            copy_to_clipboard(res);
                                            self.should_close = true;
                                        }
                                        ResultType::Command(cmd) => {
                                            execute_command(cmd);
                                            self.should_close = true;
                                        }
                                        ResultType::WebSearch(query) => {
                                            open_web_search(query);
                                            self.should_close = true;
                                        }
                                        ResultType::Url(url) => {
                                            open_url(&url);
                                            self.should_close = true;
                                        }
                                        ResultType::File(path) => {
                                            open_file(&path);
                                            self.should_close = true;
                                        }
                                        ResultType::Emoji(_, emoji) => {
                                            copy_to_clipboard(&emoji);
                                            self.should_close = true;
                                        }
                                        ResultType::Currency(_, _, result) => {
                                            copy_to_clipboard(&result.to_string());
                                            self.should_close = true;
                                        }
                                    }
                                }
                                
                                ui.add_space(-item_offset);
                            }
                        });
                }
            });
        });

        ctx.request_repaint();
    }
}

fn render_highlighted_text(
    ui: &mut egui::Ui,
    text: &str,
    match_indices: &[usize],
    is_selected: bool,
    theme: &Theme,
    alpha: f32,
) {
    let normal_color = if is_selected {
        theme.hex_to_rgb(&theme.selection_text)
    } else {
        theme.hex_to_rgb(&theme.text_color)
    };
    
    let highlight_color = theme.hex_to_rgb(&theme.highlight_color);
    
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        
        for (i, ch) in text.chars().enumerate() {
            let base_color = if match_indices.contains(&i) {
                highlight_color
            } else {
                normal_color
            };
            
            let color = egui::Color32::from_rgba_premultiplied(
                (base_color[0] * 255.0 * alpha) as u8,
                (base_color[1] * 255.0 * alpha) as u8,
                (base_color[2] * 255.0 * alpha) as u8,
                (alpha * 255.0) as u8,
            );
            
            let weight = if match_indices.contains(&i) {
                egui::FontId::proportional(theme.font_size)
            } else {
                egui::FontId::proportional(theme.font_size)
            };
            
            ui.label(
                egui::RichText::new(ch.to_string())
                    .color(color)
                    .font(weight)
            );
        }
    });
}

#[derive(Debug, Deserialize)]
struct ExchangeRatesResponse {
    rates: std::collections::HashMap<String, f64>,
}

fn is_calculation(query: &str) -> bool {
    let trimmed = query.trim();
    
    let has_operator = trimmed.contains('+') || 
                      trimmed.contains('-') || 
                      trimmed.contains('*') || 
                      trimmed.contains('/') ||
                      trimmed.contains('%') ||
                      trimmed.contains('^');
    
    let has_numbers = trimmed.chars().any(|c| c.is_ascii_digit());
    
    let has_letters = trimmed.chars().any(|c| c.is_ascii_alphabetic() && c != 'e' && c != 'E' && c != 'p' && c != 'P' && c != 'i' && c != 'I');
    
    let reasonable_length = trimmed.len() >= 2 && trimmed.len() <= 50;
    
    has_operator && has_numbers && !has_letters && reasonable_length
}

fn normalize_currency_code(code: &str) -> Option<String> {
    let code_lower = code.to_lowercase();
    let result = match code_lower.as_str() {
        "usd" | "dollar" | "dollars" => "USD",
        "eur" | "euro" | "euros" => "EUR", 
        "gbp" | "pound" | "pounds" | "sterling" => "GBP",
        "jpy" | "yen" => "JPY",
        "cad" | "canadian dollar" => "CAD",
        "aud" | "australian dollar" => "AUD",
        "chf" | "swiss franc" => "CHF",
        "cny" | "yuan" | "renminbi" => "CNY",
        "inr" | "rupee" | "rupees" => "INR",
        "lkr" | "sri lankan rupee" | "sri lankan rupees" => "LKR",
        "brl" | "real" | "reais" => "BRL",
        "rub" | "ruble" | "rubles" => "RUB",
        "krw" | "won" => "KRW",
        "mxn" | "mexican peso" => "MXN",
        "sgd" | "singapore dollar" => "SGD",
        "hkd" | "hong kong dollar" => "HKD",
        "nzd" | "new zealand dollar" => "NZD",
        "sek" | "swedish krona" => "SEK",
        "nok" | "norwegian krone" => "NOK",
        "dkk" | "danish krone" => "DKK",
        "zar" | "rand" => "ZAR",
        "try" | "turkish lira" => "TRY",
        "pln" | "zloty" => "PLN",
        "thb" | "baht" => "THB",
        "idr" | "indonesian rupiah" => "IDR",
        "huf" | "forint" => "HUF",
        "czk" | "czech koruna" => "CZK",
        "ils" | "shekel" => "ILS",
        "clp" | "chilean peso" => "CLP",
        "php" | "philippine peso" => "PHP",
        "aed" | "uae dirham" => "AED",
        "cop" | "colombian peso" => "COP",
        "sar" | "saudi riyal" => "SAR",
        "myr" | "malaysian ringgit" => "MYR",
        "ron" | "romanian leu" => "RON",
        "btc" | "bitcoin" => "BTC",
        "eth" | "ethereum" => "ETH",
        _ if code.len() == 3 => {
            return Some(code.to_uppercase());
        }
        _ => return None,
    };
    Some(result.to_string())
}

async fn convert_currency_online(query: &str) -> Option<(String, String, f64)> {
    let parts: Vec<&str> = query.split_whitespace().collect();
    
    if parts.len() >= 3 {
        let mut amount_str = parts[0];
        let mut from_currency_str = parts[1];
        let mut to_currency_str = parts.get(2).copied().unwrap_or("");
        
        if parts[0].to_lowercase() == "convert" && parts.len() >= 4 {
            amount_str = parts[1];
            from_currency_str = parts[2];
            to_currency_str = parts.get(3).copied().unwrap_or("");
        }
        
        if parts.len() >= 4 && parts[2].to_lowercase() == "to" {
            to_currency_str = parts[3];
        } else if parts.len() >= 4 && parts[0].to_lowercase() == "convert" && parts[3].to_lowercase() == "to" {
            to_currency_str = parts.get(4).copied().unwrap_or("");
        }
        
        if to_currency_str.is_empty() {
            return None;
        }
        
        if let (Ok(amount), Some(from_currency), Some(to_currency)) = (
            amount_str.parse::<f64>(),
            normalize_currency_code(from_currency_str),
            normalize_currency_code(to_currency_str),
        ) {
            if from_currency == to_currency {
                return Some((from_currency.to_string(), to_currency.to_string(), amount));
            }
            
            let client = reqwest::Client::new();
            let url = format!("https://api.exchangerate-api.com/v4/latest/{}", from_currency);
            
            match client.get(&url).send().await {
                Ok(response) => {
                    if response.status().is_success() {
                        if let Ok(exchange_data) = response.json::<ExchangeRatesResponse>().await {
                            if let Some(rate) = exchange_data.rates.get(&to_currency) {
                                let converted = amount * rate;
                                return Some((from_currency.to_string(), to_currency.to_string(), converted));
                            }
                        }
                    }
                }
                Err(_) => {
                    let fallback_url = format!("https://api.frankfurter.app/latest?from={}", from_currency);
                    if let Ok(fallback_response) = client.get(&fallback_url).send().await {
                        if fallback_response.status().is_success() {
                            if let Ok(exchange_data) = fallback_response.json::<ExchangeRatesResponse>().await {
                                if let Some(rate) = exchange_data.rates.get(&to_currency) {
                                    let converted = amount * rate;
                                    return Some((from_currency.to_string(), to_currency.to_string(), converted));
                                }
                            }
                        }
                    }
                    return None;
                }
            }
        }
    }
    None
}

fn copy_to_clipboard(text: &str) {
    // Try wl-copy first (Wayland)
    if env::var("WAYLAND_DISPLAY").is_ok() {
        let _ = Command::new("wl-copy")
            .stdin(std::process::Stdio::piped())
            .spawn()
            .and_then(|mut child| {
                if let Some(mut stdin) = child.stdin.take() {
                    stdin.write_all(text.as_bytes())?;
                }
                child.wait()
            });
        return;
    }
    
    // Fallback to xclip (X11)
    let _ = Command::new("xclip")
        .args(["-selection", "clipboard"])
        .stdin(std::process::Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            if let Some(mut stdin) = child.stdin.take() {
                stdin.write_all(text.as_bytes())?;
            }
            child.wait()
        });
}

fn execute_command(cmd: &str) {
    let _ = Command::new("sh")
        .arg("-c")
        .arg(cmd)
        .spawn();
}

fn open_web_search(query: &str) {
    let url = format!("https://duckduckgo.com/?q={}", urlencoding::encode(query));
    let _ = Command::new("xdg-open")
        .arg(url)
        .spawn();
}

fn open_url(url: &str) {
    let _ = Command::new("xdg-open")
        .arg(url)
        .spawn();
}

fn open_file(path: &PathBuf) {
    let _ = Command::new("xdg-open")
        .arg(path)
        .spawn();
}

fn search_files(query: &str) -> Vec<PathBuf> {
    let home = std::env::var("HOME").unwrap();
    let search_dirs = [
        format!("{}/Downloads", home),
        format!("{}/Documents", home),
        format!("{}/Desktop", home),
        format!("{}/Pictures", home),
        format!("{}/Music", home),
        format!("{}/Videos", home),
    ];
    
    let mut results = Vec::new();
    let query_lower = query.to_lowercase();
    
    for dir in &search_dirs {
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if let Some(file_name) = path.file_name().and_then(|n| n.to_str()) {
                    if file_name.to_lowercase().contains(&query_lower) || file_name.contains(&query_lower) {
                        results.push(path);
                        if results.len() >= 5 {
                            break;
                        }
                    }
                }
            }
        }
    }
    
    results.sort_by(|a, b| {
        let a_name = a.file_name().and_then(|n| n.to_str()).unwrap_or("");
        let b_name = b.file_name().and_then(|n| n.to_str()).unwrap_or("");
        a_name.len().cmp(&b_name.len())
    });
    
    results.into_iter().take(8).collect()
}

fn search_emojis(query: &str) -> Vec<(String, String)> {
    let query_lower = query.to_lowercase();
    
    let common_aliases: Vec<(&str, &str)> = vec![
        ("smile", "😊"), ("happy", "😊"), ("grin", "😁"), ("laugh", "😂"), ("joy", "😂"),
        ("wink", "😉"), ("blush", "😊"), ("heart", "❤️"), ("love", "❤️"), ("kiss", "😘"),
        ("tongue", "😛"), ("crazy", "😜"), ("wink2", "😜"), ("cool", "😎"), ("sunglasses", "😎"),
        ("thinking", "🤔"), ("thinking_face", "🤔"), ("shush", "🤫"), ("silence", "🤫"),
        ("wow", "😮"), ("surprise", "😮"), ("scream", "😱"), ("fear", "😱"), ("cry", "😢"),
        ("sad", "😢"), ("tear", "😢"), ("angry", "😠"), ("mad", "😠"), ("rage", "😡"),
        
        ("thumbsup", "👍"), ("like", "👍"), ("ok", "👌"), ("clap", "👏"), ("pray", "🙏"),
        ("thanks", "🙏"), ("wave", "👋"), ("hi", "👋"), ("muscle", "💪"), ("strong", "💪"),
        ("point", "👉"), ("finger", "👉"), ("eyes", "👀"), ("see", "👀"), ("ear", "👂"),
        ("listen", "👂"), ("nose", "👃"), ("mouth", "👄"), ("lips", "👄"), ("baby", "👶"),
        ("child", "👦"), ("boy", "👦"), ("girl", "👧"), ("man", "👨"), ("woman", "👩"),
        ("beard", "🧔"), ("blonde", "👱"), ("redhead", "👨‍🦰"), ("curly", "👨‍🦱"), ("bald", "👨‍🦲"),
        
        ("monkey", "🐵"), ("dog", "🐶"), ("puppy", "🐶"), ("cat", "🐱"), ("kitten", "🐱"),
        ("lion", "🦁"), ("tiger", "🐯"), ("horse", "🐴"), ("unicorn", "🦄"), ("cow", "🐮"),
        ("pig", "🐷"), ("frog", "🐸"), ("mouse", "🐭"), ("hamster", "🐹"), ("rabbit", "🐰"),
        ("bear", "🐻"), ("panda", "🐼"), ("koala", "🐨"), ("penguin", "🐧"), ("bird", "🐦"),
        ("chicken", "🐔"), ("eagle", "🦅"), ("duck", "🦆"), ("owl", "🦉"), ("bat", "🦇"),
        ("wolf", "🐺"), ("fox", "🦊"), ("raccoon", "🦝"), ("turtle", "🐢"), ("snake", "🐍"),
        ("dragon", "🐲"), ("sauropod", "🦕"), ("trex", "🦖"), ("whale", "🐳"), ("dolphin", "🐬"),
        ("fish", "🐟"), ("tropical", "🐠"), ("blowfish", "🐡"), ("shark", "🦈"), ("octopus", "🐙"),
        ("shell", "🐚"), ("snail", "🐌"), ("butterfly", "🦋"), ("bug", "🐛"), ("ant", "🐜"),
        ("bee", "🐝"), ("ladybug", "🐞"), ("cricket", "🦗"), ("spider", "🕷️"), ("scorpion", "🦂"),
        ("flower", "🌸"), ("rose", "🌹"), ("sunflower", "🌻"), ("tree", "🌳"), ("palm", "🌴"),
        ("cactus", "🌵"), ("sheaf", "🌾"), ("shamrock", "☘️"), ("maple", "🍁"), ("leaf", "🍃"),
        
        ("grape", "🍇"), ("melon", "🍈"), ("watermelon", "🍉"), ("orange", "🍊"), ("lemon", "🍋"),
        ("banana", "🍌"), ("pineapple", "🍍"), ("apple", "🍎"), ("greenapple", "🍏"), ("pear", "🍐"),
        ("peach", "🍑"), ("cherry", "🍒"), ("strawberry", "🍓"), ("kiwi", "🥝"), ("tomato", "🍅"),
        ("coconut", "🥥"), ("avocado", "🥑"), ("eggplant", "🍆"), ("potato", "🥔"), ("carrot", "🥕"),
        ("corn", "🌽"), ("pepper", "🌶️"), ("cucumber", "🥒"), ("broccoli", "🥦"), ("mushroom", "🍄"),
        ("peanuts", "🥜"), ("bread", "🍞"), ("croissant", "🥐"), ("french", "🥖"), ("pretzel", "🥨"),
        ("cheese", "🧀"), ("meat", "🍖"), ("poultry", "🍗"), ("bacon", "🥓"), ("hamburger", "🍔"),
        ("fries", "🍟"), ("pizza", "🍕"), ("hotdog", "🌭"), ("taco", "🌮"), ("burrito", "🌯"),
        ("egg", "🥚"), ("cooking", "🍳"), ("stew", "🍲"), ("bowl", "🍜"), ("popcorn", "🍿"),
        ("salt", "🧂"), ("bento", "🍱"), ("rice", "🍚"), ("riceball", "🍙"), ("ricecracker", "🍘"),
        ("sushi", "🍣"), ("dango", "🍡"), ("oden", "🍢"), ("shavedice", "🍧"), ("icecream", "🍨"),
        ("doughnut", "🍩"), ("cookie", "🍪"), ("cake", "🍰"), ("cupcake", "🧁"), ("pie", "🥧"),
        ("chocolate", "🍫"), ("candy", "🍬"), ("lollipop", "🍭"), ("custard", "🍮"), ("honey", "🍯"),
        ("babybottle", "🍼"), ("milk", "🥛"), ("coffee", "☕"), ("tea", "🍵"), ("sake", "🍶"),
        ("champagne", "🍾"), ("wine", "🍷"), ("cocktail", "🍸"), ("tropicaldrink", "🍹"), ("beer", "🍺"),
        ("beers", "🍻"), ("clinking", "🥂"), ("tumbler", "🥃"), ("cup", "🥤"), ("chopsticks", "🥢"),
        
        ("soccer", "⚽"), ("basketball", "🏀"), ("football", "🏈"), ("baseball", "⚾"), ("tennis", "🎾"),
        ("volleyball", "🏐"), ("rugby", "🏉"), ("pool", "🎱"), ("pingpong", "🏓"), ("badminton", "🏸"),
        ("hockey", "🏒"), ("fieldhockey", "🏑"), ("cricket", "🏏"), ("goal", "🥅"), ("dart", "🎯"),
        ("golf", "⛳"), ("kite", "🪁"), ("fishing", "🎣"), ("boxing", "🥊"), ("martialarts", "🥋"),
        ("running", "🏃"), ("surfing", "🏄"), ("swimming", "🏊"), ("weightlifting", "🏋️"), ("biking", "🚴"),
        ("mountainbiking", "🚵"), ("cartwheel", "🤸"), ("wrestling", "🤼"), ("waterpolo", "🤽"), ("handball", "🤾"),
        ("juggling", "🤹"), ("meditation", "🧘"), ("bath", "🛀"), ("sleep", "🛌"), ("arts", "🎨"),
        ("music", "🎵"), ("microphone", "🎤"), ("headphone", "🎧"), ("saxophone", "🎷"), ("guitar", "🎸"),
        ("piano", "🎹"), ("trumpet", "🎺"), ("violin", "🎻"), ("drum", "🥁"), ("game", "🎮"),
        ("joystick", "🕹️"), ("slot", "🎰"), ("dice", "🎲"), ("chess", "♟️"), ("puzzle", "🧩"),
        
        ("car", "🚗"), ("taxi", "🚕"), ("jeep", "🚙"), ("bus", "🚌"), ("trolley", "🚎"),
        ("racing", "🏎️"), ("policecar", "🚓"), ("ambulance", "🚑"), ("fireengine", "🚒"), ("minibus", "🚐"),
        ("truck", "🚚"), ("delivery", "🚚"), ("articulated", "🚛"), ("tractor", "🚜"), ("scooter", "🛴"),
        ("bike", "🚲"), ("motorcycle", "🏍️"), ("autorickshaw", "🛺"), ("train", "🚆"), ("metro", "🚇"),
        ("tram", "🚊"), ("monorail", "🚝"), ("mountainrailway", "🚞"), ("bullet", "🚅"), ("train2", "🚄"),
        ("lightrail", "🚈"), ("station", "🚉"), ("airplane", "✈️"), ("flight", "✈️"), ("rocket", "🚀"),
        ("helicopter", "🚁"), ("satellite", "🛰️"), ("ufo", "🛸"), ("ship", "🚢"), ("boat", "⛵"),
        ("sailboat", "⛵"), ("speedboat", "🚤"), ("ferry", "⛴️"), ("passengership", "🛳️"), ("anchor", "⚓"),
        ("fuel", "⛽"), ("construction", "🚧"), ("verticaltraffic", "🚦"), ("trafficlight", "🚥"), ("busstop", "🚏"),
        ("map", "🗺️"), ("world", "🌎"), ("japan", "🗾"), ("compass", "🧭"), ("mountain", "⛰️"),
        ("snowmountain", "🏔️"), ("volcano", "🌋"), ("mountfuji", "🗻"), ("camping", "🏕️"), ("beach", "🏖️"),
        ("island", "🏝️"), ("desert", "🏜️"), ("park", "🏞️"), ("stadium", "🏟️"), ("classical", "🏛️"),
        ("building", "🏢"), ("house", "🏠"), ("home", "🏠"), ("office", "🏢"), ("postoffice", "🏤"),
        ("hospital", "🏥"), ("bank", "🏦"), ("hotel", "🏨"), ("lovenotel", "🏩"), ("store", "🏪"),
        ("school", "🏫"), ("department", "🏬"), ("factory", "🏭"), ("japanesecastle", "🏯"), ("europeancastle", "🏰"),
        ("wedding", "💒"), ("tokyotower", "🗼"), ("statue", "🗽"), ("church", "⛪"), ("mosque", "🕌"),
        ("synagogue", "🕍"), ("shrine", "⛩️"), ("kaaba", "🕋"), ("fountain", "⛲"), ("tent", "⛺"),
        ("foggy", "🌁"), ("night", "🌃"), ("cityscape", "🏙️"), ("sunrise", "🌅"), ("sunset", "🌇"),
        ("bridge", "🌉"), ("carousel", "🎠"), ("ferris", "🎡"), ("rollercoaster", "🎢"), ("barber", "💈"),
        ("circus", "🎪"),
        
        ("watch", "⌚"), ("iphone", "📱"), ("phone", "📱"), ("calling", "📲"), ("computer", "💻"),
        ("keyboard", "⌨️"), ("desktop", "🖥️"), ("printer", "🖨️"), ("mouse", "🖱️"), ("trackball", "🖲️"),
        ("joystick", "🕹️"), ("gamepad", "🎮"), ("lightbulb", "💡"), ("battery", "🔋"), ("electric", "🧯"),
        ("money", "💰"), ("dollar", "💵"), ("yen", "💴"), ("euro", "💶"), ("pound", "💷"),
        ("creditcard", "💳"), ("receipt", "🧾"), ("chart", "💹"), ("email", "✉️"), ("envelope", "✉️"),
        ("incoming", "📨"), ("post", "📮"), ("package", "📦"), ("mailbox", "📫"), ("pencil", "✏️"),
        ("pen", "🖊️"), ("crayon", "🖍️"), ("paintbrush", "🖌️"), ("scissors", "✂️"), ("ruler", "📏"),
        ("wrench", "🔧"), ("hammer", "🔨"), ("tools", "🛠️"), ("knife", "🔪"), ("gun", "🔫"),
        ("microscope", "🔬"), ("telescope", "🔭"), ("satellite", "📡"), ("syringe", "💉"), ("pill", "💊"),
        ("door", "🚪"), ("bed", "🛏️"), ("couch", "🛋️"), ("toilet", "🚽"), ("shower", "🚿"),
        ("bathtub", "🛁"), ("razor", "🪒"), ("lotion", "🧴"), ("safetypin", "🧷"), ("broom", "🧹"),
        ("basket", "🧺"), ("roll", "🧻"), ("soap", "🧼"), ("sponge", "🧽"), ("fire", "🔥"),
        ("bomb", "💣"), ("smoking", "🚬"), ("coffin", "⚰️"), ("urn", "⚱️"), ("clown", "🤡"),
        
        ("check", "✅"), ("mark", "✅"), ("cross", "❌"), ("wrong", "❌"), ("question", "❓"),
        ("exclamation", "❗"), ("warning", "⚠️"), ("info", "ℹ️"), ("plus", "➕"), ("minus", "➖"),
        ("divide", "➗"), ("equals", "🟰"), ("infinity", "♾️"), ("recycle", "♻️"), ("fleur", "⚜️"),
        ("trident", "🔱"), ("namebadge", "📛"), ("beginner", "🔰"), ("o", "⭕"), ("whitecheck", "✅"),
        ("ballot", "☑️"), ("radio", "🔘"), ("link", "🔗"), ("curly", "➰"), ("loop", "➿"),
        ("part", "〽️"), ("eight", "✴️"), ("double", "‼️"), ("interrobang", "⁉️"), ("questionex", "⁉️"),
        ("bangbang", "‼️"), ("tm", "™️"), ("copyright", "©️"), ("registered", "®️"), ("zero", "0️⃣"),
        ("one", "1️⃣"), ("two", "2️⃣"), ("three", "3️⃣"), ("four", "4️⃣"), ("five", "5️⃣"),
        ("six", "6️⃣"), ("seven", "7️⃣"), ("eightnum", "8️⃣"), ("nine", "9️⃣"), ("ten", "🔟"),
        ("keycap", "#️⃣"), ("asterisk", "*️⃣"), ("play", "▶️"), ("pause", "⏸️"), ("stop", "⏹️"),
        ("record", "⏺️"), ("forward", "⏩"), ("rewind", "⏪"), ("up", "🔼"), ("down", "🔽"),
        ("next", "⏭️"), ("previous", "⏮️"), ("eject", "⏏️"), ("cinema", "🎦"), ("signal", "📶"),
        ("vibration", "📳"), ("mobile", "📴"), ("female", "♀️"), ("male", "♂️"), ("medical", "⚕️"),
        ("atom", "⚛️"), ("om", "🕉️"), ("starofdavid", "✡️"), ("wheeldharma", "☸️"), ("yinyang", "☯️"),
        ("latin", "✝️"), ("starandcrescent", "☪️"), ("peace", "☮️"), ("coffee", "☕"), ("skull", "💀"),
        ("poo", "💩"), ("robot", "🤖"), ("alien", "👽"), ("ghost", "👻"), ("angel", "👼"),
        ("space", "🚀"), ("ufo", "🛸"), ("gun", "🔫"), ("knife", "🔪"), ("bomb", "💣"),
    ];
    
    let alias_results: Vec<(String, String)> = common_aliases
        .iter()
        .filter(|(alias, _)| alias.contains(&query_lower))
        .map(|(alias, emoji)| (alias.to_string(), emoji.to_string()))
        .take(3)
        .collect();
    
        let crate_results: Vec<(String, String)> = emojis::iter()
        .filter_map(|emoji| {
            if emoji.name().to_lowercase().contains(&query_lower) {
                Some((emoji.name().to_string(), emoji.as_str().to_string()))
            } else {
                None
            }
        })
        .take(2)
        .collect();
    
    let mut combined = alias_results;
    for result in crate_results {
        if !combined.iter().any(|(_, emoji)| emoji == &result.1) {
            combined.push(result);
        }
    }
    
    combined.truncate(5);
    combined
}

fn looks_like_url(text: &str) -> bool {
    let text = text.trim();
    
    if text.contains("://") {
        return text.starts_with("http://") || text.starts_with("https://") || 
               text.starts_with("ftp://") || text.starts_with("file://");
    }
    
    if text.contains('.') && !text.contains(' ') {
        let domain_part = if text.contains('/') {
            text.split('/').next().unwrap_or("")
        } else {
            text
        };
        
        let parts: Vec<&str> = domain_part.split('.').collect();
        if parts.len() >= 2 {
            let last_part = parts.last().unwrap();
            
            let common_tlds = [
                "com", "org", "net", "info", "biz", "name", "pro",
                "io", "co", "me", "tv", "ai", "dev", "app", "tech", "xyz", "store",
                "online", "site", "website", "space", "club", "fun", "live", "work",
                "cloud", "digital", "media", "news", "blog", "shop", "art", "design",
                "world", "global", "link", "click", "lol", "top", "win", "bid",
                "us", "uk", "ca", "au", "de", "fr", "jp", "cn", "in", "br", "ru",
                "it", "es", "nl", "se", "no", "dk", "fi", "pl", "ch", "at", "be",
                "ie", "nz", "sg", "hk", "kr", "tw", "mx", "za", "tr", "gr", "pt",
                "eu", "asia", "africa", "lat", "berlin", "london", "nyc", "tokyo",
                "guru", "expert", "services", "solutions", "systems", "technology",
                "network", "group", "company", "center", "support", "community",
                "agency", "studio", "exchange", "foundation", "institute", "management",
                "partners", "ventures", "capital", "enterprises", "holdings", "international",
            ];
            
            return common_tlds.iter().any(|&tld| *last_part == tld) || 
                   last_part.len() == 2 ||
                   last_part.starts_with("xn--");
        }
    }
    
    false
}

fn acquire_lock() -> Result<File, String> {
    let lock_path = get_lock_path();
    
    if lock_path.exists() {
        if let Ok(content) = fs::read_to_string(&lock_path) {
            if let Ok(pid) = content.trim().parse::<u32>() {
                let check = Command::new("kill")
                    .arg("-0")
                    .arg(pid.to_string())
                    .output();
                    
                if let Ok(output) = check {
                    if output.status.success() {
                        return Err("Flint is already running!".to_string());
                    }
                }
            }
        }
        let _ = fs::remove_file(&lock_path);
    }
    
    let mut lock_file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(&lock_path)
        .map_err(|e| format!("Failed to create lock file: {}", e))?;
    
    let pid = std::process::id();
    lock_file.write_all(pid.to_string().as_bytes())
        .map_err(|e| format!("Failed to write PID: {}", e))?;
    
    Ok(lock_file)
}

fn get_lock_path() -> PathBuf {
    let runtime_dir = std::env::var("XDG_RUNTIME_DIR")
        .unwrap_or_else(|_| format!("/tmp/flint-{}", std::env::var("USER").unwrap_or_else(|_| "user".to_string())));
    PathBuf::from(runtime_dir).join("flint.lock")
}

fn launch_app(desktop_id: &str) {
    let _ = Command::new("gtk-launch").arg(desktop_id).spawn();
}

fn launch_flatpak_app(flatpak_id: &str) {
    let _ = Command::new("flatpak")
        .arg("run")
        .arg(flatpak_id)
        .spawn();
}

fn load_icon_texture(ctx: &egui::Context, path: &PathBuf, data: &[u8]) -> Option<egui::TextureHandle> {
    let img = image::load_from_memory(data).ok()?;
    let rgba = img.to_rgba8();
    let size = [rgba.width() as usize, rgba.height() as usize];
    let pixels = rgba.into_raw();
    
    Some(ctx.load_texture(
        format!("icon_{}", path.display()),
        egui::ColorImage::from_rgba_unmultiplied(size, &pixels),
        egui::TextureOptions::LINEAR,
    ))
}

fn detect_icon_theme() -> Option<String> {
    if let Ok(output) = Command::new("gsettings").args(["get", "org.gnome.desktop.interface", "icon-theme"]).output() {
        if output.status.success() {
            let theme = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !theme.is_empty() && theme != "\n" {
                return Some(theme.replace('\'', "").replace('"', ""));
            }
        }
    }
    
    if let Ok(output) = Command::new("xfconf-query").args(["-c", "xsettings", "-p", "/Net/IconThemeName"]).output() {
        if output.status.success() {
            let theme = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !theme.is_empty() {
                return Some(theme);
            }
        }
    }
    
    if let Some(home) = std::env::var("HOME").ok() {
        let gtk3_path = format!("{}/.config/gtk-3.0/settings.ini", home);
        if let Ok(content) = fs::read_to_string(&gtk3_path) {
            for line in content.lines() {
                if line.contains("gtk-icon-theme-name") {
                    if let Some(theme) = line.split('=').nth(1) {
                        return Some(theme.trim().to_string());
                    }
                }
            }
        }
        
        let gtk4_path = format!("{}/.config/gtk-4.0/settings.ini", home);
        if let Ok(content) = fs::read_to_string(&gtk4_path) {
            for line in content.lines() {
                if line.contains("gtk-icon-theme-name") {
                    if let Some(theme) = line.split('=').nth(1) {
                        return Some(theme.trim().to_string());
                    }
                }
            }
        }
        
        let kdeglobals = format!("{}/.config/kdeglobals", home);
        if let Ok(content) = fs::read_to_string(&kdeglobals) {
            for line in content.lines() {
                if line.contains("Theme=") && line.contains("Icons") {
                    if let Some(theme) = line.split('=').nth(1) {
                        return Some(theme.trim().to_string());
                    }
                }
            }
        }
        
        let xresources = format!("{}/.Xresources", home);
        if let Ok(content) = fs::read_to_string(&xresources) {
            for line in content.lines() {
                if line.contains("*iconTheme") || line.contains("*.iconTheme") {
                    if let Some(theme) = line.split(':').nth(1) {
                        return Some(theme.trim().to_string());
                    }
                }
            }
        }
        
        let xsettings = format!("{}/.Xsettingsd", home);
        if let Ok(content) = fs::read_to_string(&xsettings) {
            for line in content.lines() {
                if line.contains("Net/IconThemeName") || line.contains("\"Net/IconThemeName\"") {
                    if let Some(theme) = line.split('"').nth(1) {
                        if !theme.contains("IconThemeName") {
                            return Some(theme.trim().to_string());
                        }
                    }
                }
            }
        }
    }
    
    None
}

fn find_icon_path(icon_name: &str) -> Option<PathBuf> {
    if icon_name.starts_with('/') {
        let path = PathBuf::from(icon_name);
        if path.exists() {
            return Some(path);
        }
    }
    
    let home = std::env::var("HOME").ok();
    let icon_sizes = vec!["128x128", "64x64", "48x48", "32x32", "24x24", "16x16", "scalable"];
    
    let icon_variants = vec![
        icon_name.to_string(),
        format!("{}.png", icon_name),
        format!("{}.svg", icon_name),
        format!("{}.xpm", icon_name),
    ];
    
    let search_dirs: Vec<String> = vec![
        "/usr/share/icons".to_string(),
        "/usr/local/share/icons".to_string(),
    ].into_iter().chain(home.clone().map(|h| format!("{}/.icons", h))).collect();
    
    let detected_theme = detect_icon_theme();
    
    let mut theme_dirs: Vec<String> = Vec::new();
    if let Some(ref theme) = detected_theme {
        theme_dirs.push(theme.clone());
    }
    theme_dirs.push("hicolor".to_string());
    
    for search_dir in &search_dirs {
        if !PathBuf::from(search_dir).exists() {
            continue;
        }
        
        for variant in &icon_variants {
            for size in &icon_sizes {
                for theme in &theme_dirs {
                    let icon_path = PathBuf::from(search_dir)
                        .join(theme)
                        .join(size)
                        .join("apps")
                        .join(variant);
                    if icon_path.exists() {
                        return Some(icon_path);
                    }
                }
            }
            
            let direct_path = PathBuf::from(search_dir).join(variant);
            if direct_path.exists() && direct_path.is_dir() {
                continue;
            }
            if direct_path.exists() {
                return Some(direct_path);
            }
        }
    }
    
    None
}

fn scan_desktop_apps() -> Vec<AppEntry> {
    let mut apps = Vec::new();
    let home = std::env::var("HOME").unwrap();
    let local_path = format!("{}/.local/share/applications", home);
    let paths = vec!["/usr/share/applications", "/usr/local/share/applications", &local_path];

    for path in paths {
        if let Ok(entries) = fs::read_dir(path) {
            for entry in entries.flatten() {
                let path: PathBuf = entry.path();
                if path.extension().and_then(|e| e.to_str()) == Some("desktop") {
                    if let Ok(content) = fs::read_to_string(&path) {
                        let mut name = None;
                        let mut exec = None;
                        let mut icon = None;
                        
                        for line in content.lines() {
                            if line.starts_with("Name=") {
                                name = Some(line["Name=".len()..].to_string());
                            } else if line.starts_with("Exec=") {
                                let exec_line = &line["Exec=".len()..];
                                let command = extract_command_from_exec(exec_line);
                                exec = Some(command);
                            } else if line.starts_with("Icon=") {
                                icon = Some(line["Icon=".len()..].to_string());
                            }
                            
                            if name.is_some() && exec.is_some() {
                                break;
                            }
                        }
                        
                        if let (Some(app_name), Some(exec_command)) = (name, exec) {
                            if let Some(file_stem) = path.file_stem().and_then(|s| s.to_str()) {
                                let icon_path = icon.and_then(|i| find_icon_path(&i));
                                apps.push(AppEntry {
                                    name: app_name,
                                    desktop_id: file_stem.to_string(),
                                    exec_command,
                                    match_indices: Vec::new(),
                                    icon_path,
                                });
                            }
                        }
                    }
                }
            }
        }
    }

    apps.sort_by(|a, b| a.name.cmp(&b.name));
    apps.dedup_by(|a, b| a.name == b.name);
    apps
}

fn scan_flatpak_apps() -> Vec<FlatpakAppEntry> {
    let mut flatpak_apps = Vec::new();
    
    let output = Command::new("flatpak")
        .args(["list", "--app", "--columns=name,application,description"])
        .output();
    
    match output {
        Ok(output) if output.status.success() => {
            let output_str = String::from_utf8_lossy(&output.stdout);
            
            for line in output_str.lines() {
                let parts: Vec<&str> = line.split('\t').collect();
                if parts.len() >= 3 {
                    let name = parts[0].to_string();
                    let flatpak_id = parts[1].to_string();
                    let description = parts[2].to_string();
                    let icon_path = find_icon_path(&flatpak_id);
                    
                    flatpak_apps.push(FlatpakAppEntry {
                        name,
                        flatpak_id,
                        description,
                        match_indices: Vec::new(),
                        icon_path,
                    });
                }
            }
        }
        _ => {}
    }
    
    flatpak_apps
}

fn extract_command_from_exec(exec_line: &str) -> String {
    let cleaned = exec_line
        .split_whitespace()
        .find(|part| {
            !part.starts_with('%') && 
            !part.starts_with('@') &&
            !part.starts_with('-') &&
            !part.is_empty()
        })
        .unwrap_or(exec_line)
        .to_string();
    
    cleaned.split('%').next().unwrap_or(&cleaned).to_string()
}

fn apply_theme(ctx: &egui::Context, theme: &Theme) {
    let mut visuals = egui::Visuals::light();
    
    visuals.window_fill = egui::Color32::TRANSPARENT;
    visuals.panel_fill = egui::Color32::TRANSPARENT;
    
    // Improve text rendering on Wayland
    visuals.widgets.noninteractive.fg_stroke = egui::Stroke::new(1.0, egui::Color32::WHITE);
    
    ctx.set_visuals(visuals);
    
    // Set default font size for better scaling
    let mut style = (*ctx.style()).clone();
    style.text_styles.insert(
        egui::TextStyle::Body,
        egui::FontId::proportional(theme.font_size),
    );
    style.text_styles.insert(
        egui::TextStyle::Button,
        egui::FontId::proportional(theme.font_size),
    );
    ctx.set_style(style);
    
    if theme.font_family != "System" {
        let home = std::env::var("HOME").unwrap();
        let possible_files = [
            format!("{}/.fonts/{}.ttf", home, theme.font_family),
            format!("{}/.fonts/{}.otf", home, theme.font_family),
            format!("{}/.local/share/fonts/{}.ttf", home, theme.font_family),
            format!("{}/.local/share/fonts/{}.otf", home, theme.font_family),
            format!("/usr/share/fonts/truetype/{}.ttf", theme.font_family),
            format!("/usr/share/fonts/opentype/{}.otf", theme.font_family),
            format!("/usr/local/share/fonts/{}.ttf", theme.font_family),
            format!("/usr/local/share/fonts/{}.otf", theme.font_family),
            format!("/usr/share/fonts/{}.ttf", theme.font_family),
            format!("/usr/share/fonts/{}.otf", theme.font_family),
        ];

        let mut found = false;
        for path in &possible_files {
            if std::path::Path::new(path).exists() {
                if let Ok(font_data) = std::fs::read(path) {
                    let mut fonts = egui::FontDefinitions::default();
                    fonts.font_data.insert(theme.font_family.clone(), egui::FontData::from_owned(font_data));
                    fonts.families.get_mut(&egui::FontFamily::Proportional).unwrap().insert(0, theme.font_family.clone());
                    fonts.families.get_mut(&egui::FontFamily::Monospace).unwrap().insert(0, theme.font_family.clone());
                    ctx.set_fonts(fonts);
                    found = true;
                    break;
                }
            }
        }

        if !found {
            eprintln!("Font '{}' not found in any common directory", theme.font_family);
        }
    }
}

fn get_config_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap();
    PathBuf::from(home).join(".config").join("flint")
}

fn create_default_theme(theme_path: &PathBuf) {
    let default_theme = r#"# Flint Theme Configuration - Spotlight Style
# Made By SenithuMadiv ( https://github.com/Senithumadiv )
    # Use hex colors like #RRGGBB

# Main window colors
background=#ffffff
text_color=#333333
selection_bg=#007aff
selection_text=#ffffff
border_color=#cccccc
highlight_color=#007aff

# Font settings
font_size=18
font_family=System

# Border radius (0 = square corners, higher = more rounded)
border_radius=0

# Icon settings
enable_icons=true
icon_theme=Papirus
icon_size=24
"#;
    
    if let Some(parent) = theme_path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let _ = fs::write(theme_path, default_theme);
}

fn get_monitor_center() -> Option<(f32, f32)> {
    // Try Wayland first
    if env::var("WAYLAND_DISPLAY").is_ok() {
        if let Some(center) = get_monitor_center_wayland() {
            return Some(center);
        }
    }
    
    // Fallback to X11 methods
    let mouse_position = get_mouse_position()?;
    
    if let Some(center) = get_monitor_center_xrandr(mouse_position) {
        return Some(center);
    }
    
    if let Some(center) = get_monitor_center_xinerama(mouse_position) {
        return Some(center);
    }
    
    get_monitor_center_fallback(mouse_position)
}

fn get_monitor_center_wayland() -> Option<(f32, f32)> {
    // For Wayland, we need to use different methods
    // Try to get display info from environment
    
    // Option 1: Use wlr-randr (for wlroots-based compositors)
    if let Ok(output) = Command::new("wlr-randr").output() {
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            // Parse wlr-randr output for current monitor
            for line in stdout.lines() {
                if line.contains("current") && line.contains("x") {
                    if let Some(dim_part) = line.split(',').next() {
                        let dims: Vec<&str> = dim_part.split_whitespace().collect();
                        for dim in dims {
                            if dim.contains('x') {
                                let res: Vec<&str> = dim.split('x').collect();
                                if res.len() == 2 {
                                    if let (Ok(w), Ok(h)) = (res[0].parse::<f32>(), res[1].parse::<f32>()) {
                                        return Some((w / 2.0, h / 2.0));
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    
    // Option 2: Try to get display info from environment
    if let Ok(width) = env::var("WAYLAND_DISPLAY_WIDTH") {
        if let Ok(height) = env::var("WAYLAND_DISPLAY_HEIGHT") {
            if let (Ok(w), Ok(h)) = (width.parse::<f32>(), height.parse::<f32>()) {
                return Some((w / 2.0, h / 2.0));
            }
        }
    }
    
    // Default fallback
    Some((960.0, 540.0))
}

fn get_monitor_center_xrandr(mouse_position: (f32, f32)) -> Option<(f32, f32)> {
    let output = std::process::Command::new("xrandr")
        .arg("--query")
        .output()
        .ok()?;
    
    let output_str = String::from_utf8(output.stdout).ok()?;
    
    for line in output_str.lines() {
        if line.contains(" connected ") && line.contains("+") {
            let parts: Vec<&str> = line.split_whitespace().collect();
            
            for part in &parts {
                if part.contains('+') && part.contains('x') {
                    if let Some(plus_pos) = part.find('+') {
                        let resolution = &part[..plus_pos];
                        let position = &part[plus_pos + 1..];
                        
                        let res_parts: Vec<&str> = resolution.split('x').collect();
                        let pos_parts: Vec<&str> = position.split('+').collect();
                        
                        if res_parts.len() == 2 && pos_parts.len() == 2 {
                            if let (Ok(width), Ok(height), Ok(monitor_x), Ok(monitor_y)) = (
                                res_parts[0].parse::<f32>(),
                                res_parts[1].parse::<f32>(),
                                pos_parts[0].parse::<f32>(),
                                pos_parts[1].parse::<f32>()
                            ) {
                                if mouse_position.0 >= monitor_x && 
                                   mouse_position.0 < monitor_x + width &&
                                   mouse_position.1 >= monitor_y && 
                                   mouse_position.1 < monitor_y + height {
                                    return Some((monitor_x + width / 2.0, monitor_y + height / 2.0));
                                }
                            }
                        }
                    }
                    break;
                }
            }
        }
    }
    None
}

fn get_monitor_center_xinerama(mouse_position: (f32, f32)) -> Option<(f32, f32)> {
    let output = std::process::Command::new("xwininfo")
        .arg("-root")
        .arg("-tree")
        .output()
        .ok()?;
    
    let output_str = String::from_utf8(output.stdout).ok()?;
    
    let root_output = std::process::Command::new("xwininfo")
        .arg("-root")
        .output()
        .ok()?;
    
    let root_str = String::from_utf8(root_output.stdout).ok()?;
    
    let mut width = 0.0;
    let mut height = 0.0;
    
    for line in root_str.lines() {
        if line.contains("Width:") {
            if let Ok(w) = line.split(':').nth(1).unwrap_or("").trim().parse::<f32>() {
                width = w;
            }
        } else if line.contains("Height:") {
            if let Ok(h) = line.split(':').nth(1).unwrap_or("").trim().parse::<f32>() {
                height = h;
            }
        }
    }
    
    if width > 0.0 && height > 0.0 {
        Some((width / 2.0, height / 2.0))
    } else {
        None
    }
}

fn get_monitor_center_fallback(mouse_position: (f32, f32)) -> Option<(f32, f32)> {
    let output = std::process::Command::new("xdpyinfo")
        .output()
        .ok()?;
    
    let output_str = String::from_utf8(output.stdout).ok()?;
    
    let mut screen_width = 1920.0;
    let mut screen_height = 1080.0;
    
    for line in output_str.lines() {
        if line.contains("dimensions:") {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 3 {
                let dims: Vec<&str> = parts[1].split('x').collect();
                if dims.len() == 2 {
                    if let (Ok(w), Ok(h)) = (dims[0].parse::<f32>(), dims[1].parse::<f32>()) {
                        screen_width = w;
                        screen_height = h;
                    }
                }
            }
        }
    }
    
    if screen_width > 2000.0 {
        let monitor_width = screen_width / 2.0;
        if mouse_position.0 < monitor_width {
            Some((monitor_width / 2.0, screen_height / 2.0))
        } else {
            Some((monitor_width + monitor_width / 2.0, screen_height / 2.0))
        }
    } else {
        Some((screen_width / 2.0, screen_height / 2.0))
    }
}

fn get_mouse_position() -> Option<(f32, f32)> {
    // Try Wayland first
    if env::var("WAYLAND_DISPLAY").is_ok() {
        if let Some(pos) = get_mouse_position_wayland() {
            return Some(pos);
        }
    }
    
    // Fallback to xdotool (X11)
    let output = std::process::Command::new("xdotool")
        .arg("getmouselocation")
        .output()
        .ok()?;
    
    let output_str = String::from_utf8(output.stdout).ok()?;
    
    let mut x = None;
    let mut y = None;
    
    for part in output_str.split_whitespace() {
        if part.starts_with("x:") {
            x = part[2..].parse::<f32>().ok();
        } else if part.starts_with("y:") {
            y = part[2..].parse::<f32>().ok();
        }
    }
    
    match (x, y) {
        (Some(x), Some(y)) => Some((x, y)),
        _ => None,
    }
}

fn get_mouse_position_wayland() -> Option<(f32, f32)> {
    // Try to get mouse position using slurp or other Wayland tools
    if let Ok(output) = Command::new("slurp").arg("-p").output() {
        if output.status.success() {
            let pos_str = String::from_utf8_lossy(&output.stdout);
            let coords: Vec<&str> = pos_str.trim().split(',').collect();
            if coords.len() >= 2 {
                if let (Ok(x), Ok(y)) = (coords[0].parse::<f32>(), coords[1].parse::<f32>()) {
                    return Some((x, y));
                }
            }
        }
    }
    
    None
}

fn main() -> eframe::Result<()> {
    let app = match FlintApp::new() {
        Ok(app) => app,
        Err(e) => {
            eprintln!("{}", e);
            std::process::exit(1);
        }
    };

    let window_width = 600.0;
    let initial_height = 50.0;
    
    // Check if running under Wayland
    let is_wayland = env::var("WAYLAND_DISPLAY").is_ok();
    
    // Get DPI/scale factor
    let scale_factor = if is_wayland {
        // Try to get Wayland scale
        if let Ok(scale) = env::var("GDK_SCALE") {
            scale.parse::<f32>().unwrap_or(1.0)
        } else if let Ok(scale) = env::var("QT_SCALE_FACTOR") {
            scale.parse::<f32>().unwrap_or(1.0)
        } else {
            // Default to 1.0
            1.0
        }
    } else {
        1.0
    };

    let position = if let Some((center_x, center_y)) = get_monitor_center() {
        println!("Positioning window at: ({}, {})", center_x - window_width / 2.0, center_y - initial_height / 2.0);
        egui::pos2(center_x - window_width / 2.0, center_y - initial_height / 2.0)
    } else {
        eprintln!("Failed to get monitor center, using default position");
        egui::pos2(100.0, 100.0)
    };

    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size([window_width, initial_height])
        .with_decorations(false)
        .with_always_on_top()
        .with_resizable(false)
        .with_window_level(egui::WindowLevel::AlwaysOnTop)
        .with_taskbar(false)
        .with_position(position)
        .with_transparent(true);

    // Wayland-specific settings - use compatible methods
    if is_wayland {
        // For older eframe versions, we handle scaling differently
        // Just set the app_id for Wayland compositors
        viewport = viewport.with_app_id("flint".to_string());
    }

    let options = eframe::NativeOptions {
        viewport,
        centered: false,
        ..Default::default()
    };

    eframe::run_native(
        "Flint",
        options,
        Box::new(move |cc| {
            // Set the scale factor for egui
            if is_wayland {
                cc.egui_ctx.set_pixels_per_point(scale_factor);
            }
            apply_theme(&cc.egui_ctx, &app.theme);
            cc.egui_ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
            Box::new(app)
        }),
    )
}