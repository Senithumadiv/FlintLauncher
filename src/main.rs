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
}

#[derive(Clone)]
struct AppEntry {
    name: String,
    desktop_id: String,
    match_indices: Vec<usize>,
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
        let runtime = tokio::runtime::Runtime::new()
            .map_err(|e| format!("Failed to create async runtime: {}", e))?;
        
        Ok(Self {
            query: String::new(),
            results: Vec::new(),
            items,
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
        // Ensure we have the right number of result animations
        if self.result_animations.len() != self.results.len() {
            self.result_animations = self.results.iter()
                .enumerate()
                .map(|(i, _)| {
                    // Stagger the animations based on index with slide down effect
                    let delay = Duration::from_millis((i * 40) as u64).min(Duration::from_millis(200));
                    let mut anim = AnimationState::new(Duration::from_millis(250), AnimationType::SlideDown);
                    anim.start_time += delay;
                    anim
                })
                .collect();
        }
        
        // Update all animations
        for anim in &mut self.result_animations {
            anim.update();
        }
    }
    
    fn get_result_offset(&self, index: usize) -> f32 {
        self.result_animations.get(index)
            .map(|anim| {
                match anim.animation_type {
                    AnimationType::SlideDown => (1.0 - anim.ease_out()) * -30.0, // Slide down from above
                    AnimationType::BounceDown => (1.0 - anim.ease_out_bounce()) * -40.0, // Bounce down effect
                    _ => 0.0,
                }
            })
            .unwrap_or(0.0)
    }
    
    fn get_result_alpha(&self, index: usize) -> f32 {
        self.result_animations.get(index)
            .map(|anim| {
                match anim.animation_type {
                    AnimationType::SlideDown | AnimationType::BounceDown => anim.ease_out(), // Fade in while sliding
                    _ => anim.ease_out(),
                }
            })
            .unwrap_or(1.0)
    }
}

impl eframe::App for FlintApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Update animations
        let _window_animating = self.window_animation.update();
        self.update_result_animations();
        
        let still_animating = self.window_animation.progress < 1.0 || self.result_animations.iter().any(|a| a.progress < 1.0);
        
        if still_animating {
            ctx.request_repaint();
        }
        
        // Close if clicked outside the window - ROFI STYLE (FIXED)
        if ctx.input(|i| i.pointer.any_click()) {
            if let Some(pos) = ctx.input(|i| i.pointer.interact_pos()) {
                let rect = ctx.screen_rect();
                if !rect.contains(pos) {
                    self.should_close = true;
                }
            }
        }

        // Close if window loses focus AFTER initial focus - FIXED
        if self.has_focused {
            if let Some(focused) = ctx.input(|i| i.viewport().focused) {
                if !focused {
                    self.should_close = true;
                }
            }
        }
        
        if self.should_close {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }

        // Animation values - Fade in for window
        let window_alpha = self.window_animation.ease_out();
        
        // Calculate dynamic window size
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
        
        // Resize window based on content
        ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(
            window_width,
            total_height
        )));

        let bg_rgb = self.theme.hex_to_rgb(&self.theme.background);
        let border_rgb = self.theme.hex_to_rgb(&self.theme.border_color);
        
        // Apply window alpha to background for fade-in
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
                
            // Center everything with animation
            ui.vertical(|ui| {
                // Search box with Spotlight style
                let text_rgb = self.theme.hex_to_rgb(&self.theme.text_color);
                
                ui.add_space(5.0);
                ui.add_space(5.0);
                
                // Search input - also fade in with window
                let search_text_color = egui::Color32::from_rgba_premultiplied(
                    (text_rgb[0] * 255.0 * window_alpha) as u8,
                    (text_rgb[1] * 255.0 * window_alpha) as u8,
                    (text_rgb[2] * 255.0 * window_alpha) as u8,
                    (window_alpha * 255.0) as u8,
                );
                
                ui.horizontal(|ui| {
                    ui.add_space(15.0);
                    
                    let response = ui.add_sized(
                        [window_width - 30.0, 30.0],
                        egui::TextEdit::singleline(&mut self.query)
                            .hint_text("Search...")
                            .frame(false)
                            .text_color(search_text_color)
                            .font(egui::FontId::proportional(20.0))
                            .id(egui::Id::new("search_field"))
                    );

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
                    
                    // Create a custom separator using a filled rectangle
                    let separator_height = 1.0;
                    let available_width = ui.available_width();
                    let separator_rect = egui::Rect::from_min_size(
                        ui.cursor().min,  // Use current cursor position
                        egui::vec2(available_width, separator_height)
                    );
                    ui.painter().rect_filled(separator_rect, 0.0, separator_color);
                    
                    // Move cursor down past the separator
                    ui.add_space(separator_height + 5.0);
                }

                // Handle keyboard input
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

// Update search results
self.results.clear();

if !self.query.is_empty() {
    // File search (triggered with file: prefix)
    if self.query.starts_with("file:") {
        let file_query = &self.query[5..].trim();
        if !file_query.is_empty() {
            let file_results = search_files(file_query);
            for path in file_results {
                self.results.push(ResultType::File(path));
            }
        } else {
            // Show hint when no query after file:
            self.results.push(ResultType::Command("Search files...".to_string()));
        }
    }
    // Emoji search (starts with e:)
    else if self.query.starts_with("e:") {
        let emoji_query = &self.query[2..].trim();
        if !emoji_query.is_empty() {
            let emoji_results = search_emojis(emoji_query);
            for (name, emoji) in emoji_results {
                self.results.push(ResultType::Emoji(name, emoji));
            }
        } else {
            // Show hint when no query after e:
            self.results.push(ResultType::Command("Search emojis...".to_string()));
        }
    }
    // Currency conversion - try online first
    else if let Some((from, to, result)) = self.runtime.block_on(convert_currency_online(&self.query)) {
        self.results.push(ResultType::Currency(from, to, result));
    }
    // URL detection
    else if looks_like_url(&self.query) {
        let url = if self.query.contains("://") {
            self.query.clone()
        } else {
            format!("https://{}", self.query)
        };
        self.results.push(ResultType::Url(url));
    }
    // Calculator mode - NO LONGER REQUIRES = PREFIX
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
    // Shell command mode
    else if self.query.starts_with('$') {
        let cmd = &self.query[1..].trim();
        if !cmd.is_empty() {
            self.results.push(ResultType::Command(cmd.to_string()));
        } else {
            // Show hint when no query after $
            self.results.push(ResultType::Command("Enter shell command...".to_string()));
        }
    }
    // Web search mode
    else if self.query.starts_with('@') {
        let search = &self.query[1..].trim();
        if !search.is_empty() {
            self.results.push(ResultType::WebSearch(search.to_string()));
        } else {
            // Show hint when no query after @
            self.results.push(ResultType::Command("Search the web...".to_string()));
        }
    }
    
    // Normal app search (only if no other results found)
    if self.results.is_empty() {
        let matcher = SkimMatcherV2::default();
        let query = self.query.clone();
        
        let mut scored_results: Vec<(i64, AppEntry)> = self
            .items
            .par_iter()
            .filter_map(|app| {
                matcher.fuzzy_indices(&app.name, &query)
                    .map(|(score, indices)| {
                        let mut app_with_match = app.clone();
                        app_with_match.match_indices = indices;
                        (score, app_with_match)
                    })
            })
            .collect();
        
        scored_results.sort_by(|a, b| b.0.cmp(&a.0));
        
        for (_, app) in scored_results.into_iter().take(max_visible_results) {
            self.results.push(ResultType::App(app));
        }
        
        // Offer web search if no apps found
        if self.results.is_empty() {
            self.results.push(ResultType::WebSearch(query));
        }
    }
    
    if self.selected >= self.results.len() && !self.results.is_empty() {
        self.selected = 0;
    }
}

                // Show results with drop down animation
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
                                
                                // Apply vertical offset for drop down animation
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
                                
                                // Scroll to selected item
                                if is_selected {
                                    response.scroll_to_me(Some(egui::Align::Center));
                                }
                                
                                if response.clicked() {
                                    match result {
                                        ResultType::App(app) => {
                                            launch_app(&app.desktop_id);
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
                                
                                // Add space after item for proper spacing during animation
                                ui.add_space(-item_offset); // Counteract the offset for next item
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

// Check if a string looks like a mathematical expression
fn is_calculation(query: &str) -> bool {
    let trimmed = query.trim();
    
    // Must contain at least one operator and numbers
    let has_operator = trimmed.contains('+') || 
                      trimmed.contains('-') || 
                      trimmed.contains('*') || 
                      trimmed.contains('/') ||
                      trimmed.contains('%') ||
                      trimmed.contains('^');
    
    let has_numbers = trimmed.chars().any(|c| c.is_ascii_digit());
    
    // Should not contain letters (except for math constants like pi, e, but we'll keep it simple)
    let has_letters = trimmed.chars().any(|c| c.is_ascii_alphabetic() && c != 'e' && c != 'E' && c != 'p' && c != 'P' && c != 'i' && c != 'I');
    
    // Should be reasonable length for a calculation
    let reasonable_length = trimmed.len() >= 2 && trimmed.len() <= 50;
    
    has_operator && has_numbers && !has_letters && reasonable_length
}

// Currency code mapping for case-insensitive support
fn normalize_currency_code(code: &str) -> Option<String> {
    let code_lower = code.to_lowercase();
    let result = match code_lower.as_str() {
        // Major currencies
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
        // Crypto currencies
        "btc" | "bitcoin" => "BTC",
        "eth" | "ethereum" => "ETH",
        // Fallback - if it's a 3-letter code, use it as-is in uppercase
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
        
        // Handle "convert" prefix
        if parts[0].to_lowercase() == "convert" && parts.len() >= 4 {
            amount_str = parts[1];
            from_currency_str = parts[2];
            to_currency_str = parts.get(3).copied().unwrap_or("");
        }
        
        // Handle "to" separator
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
            // Skip if currencies are the same
            if from_currency == to_currency {
                return Some((from_currency.to_string(), to_currency.to_string(), amount));
            }
            
            // Use ExchangeRate-API which supports LKR and many other currencies
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
                    // Fallback to Frankfurter API if ExchangeRate-API fails
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
                    if file_name.to_lowercase().contains(&query_lower) {
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
    use emojis::Emoji;
    
    let query_lower = query.to_lowercase();
    
    // Common aliases mapping for better search
    let common_aliases: Vec<(&str, &str)> = vec![
        // Smileys & Emotion
        ("smile", "😊"), ("happy", "😊"), ("grin", "😁"), ("laugh", "😂"), ("joy", "😂"),
        ("wink", "😉"), ("blush", "😊"), ("heart", "❤️"), ("love", "❤️"), ("kiss", "😘"),
        ("tongue", "😛"), ("crazy", "😜"), ("wink2", "😜"), ("cool", "😎"), ("sunglasses", "😎"),
        ("thinking", "🤔"), ("thinking_face", "🤔"), ("shush", "🤫"), ("silence", "🤫"),
        ("wow", "😮"), ("surprise", "😮"), ("scream", "😱"), ("fear", "😱"), ("cry", "😢"),
        ("sad", "😢"), ("tear", "😢"), ("angry", "😠"), ("mad", "😠"), ("rage", "😡"),
        
        // People & Body
        ("thumbsup", "👍"), ("like", "👍"), ("ok", "👌"), ("clap", "👏"), ("pray", "🙏"),
        ("thanks", "🙏"), ("wave", "👋"), ("hi", "👋"), ("muscle", "💪"), ("strong", "💪"),
        ("point", "👉"), ("finger", "👉"), ("eyes", "👀"), ("see", "👀"), ("ear", "👂"),
        ("listen", "👂"), ("nose", "👃"), ("mouth", "👄"), ("lips", "👄"), ("baby", "👶"),
        ("child", "👦"), ("boy", "👦"), ("girl", "👧"), ("man", "👨"), ("woman", "👩"),
        ("beard", "🧔"), ("blonde", "👱"), ("redhead", "👨‍🦰"), ("curly", "👨‍🦱"), ("bald", "👨‍🦲"),
        
        // Animals & Nature
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
        
        // Food & Drink
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
        
        // Activities & Sports
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
        
        // Travel & Places
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
        
        // Objects
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
        
        // Symbols
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
    
    // First, search in common aliases (most user-friendly)
    let alias_results: Vec<(String, String)> = common_aliases
        .iter()
        .filter(|(alias, _)| alias.contains(&query_lower))
        .map(|(alias, emoji)| (alias.to_string(), emoji.to_string()))
        .take(3)
        .collect();
    
    // Then search in emoji names from the crate
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
    
    // Combine results, removing duplicates
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
    
    // Common URL patterns
    if text.contains("://") {
        return text.starts_with("http://") || text.starts_with("https://") || 
               text.starts_with("ftp://") || text.starts_with("file://");
    }
    
    // Domain-like patterns with optional paths
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
                        
                        for line in content.lines() {
                            if line.starts_with("Name=") {
                                name = Some(line["Name=".len()..].to_string());
                                break;
                            }
                        }
                        
                        if let Some(app_name) = name {
                            if let Some(file_stem) = path.file_stem().and_then(|s| s.to_str()) {
                                apps.push(AppEntry {
                                    name: app_name,
                                    desktop_id: file_stem.to_string(),
                                    match_indices: Vec::new(),
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

fn apply_theme(ctx: &egui::Context, theme: &Theme) {
    let mut visuals = egui::Visuals::light();
    
    visuals.window_fill = egui::Color32::TRANSPARENT;
    visuals.panel_fill = egui::Color32::TRANSPARENT;
    
    ctx.set_visuals(visuals);
    
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
"#;
    
    if let Some(parent) = theme_path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let _ = fs::write(theme_path, default_theme);
}

fn main() -> eframe::Result<()> {
    let app = match FlintApp::new() {
        Ok(app) => app,
        Err(e) => {
            eprintln!("{}", e);
            std::process::exit(1);
        }
    };
    
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([600.0, 50.0])
            .with_decorations(false)
            .with_always_on_top()
            .with_resizable(false)
            .with_window_level(egui::WindowLevel::AlwaysOnTop)
            .with_taskbar(false)
            .with_window_type(egui::X11WindowType::Utility)
            .with_position(egui::pos2(
                (1920.0 - 600.0) / 2.0,
                200.0,
            )),
        centered: false,
        ..Default::default()
    };

    eframe::run_native(
        "Flint",
        options,
        Box::new(move |cc| {
            cc.egui_ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
            Box::new(app)
        }),
    )
}