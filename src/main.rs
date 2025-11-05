use eframe::egui;
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use rayon::prelude::*;
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
    Url(String),  // Add URL variant
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
}

impl FlintApp {
    fn new() -> Result<Self, String> {
        let lock_file = acquire_lock()?;
        let items = scan_desktop_apps();
        
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
        let window_animating = self.window_animation.update();
        self.update_result_animations();
        
        let still_animating = window_animating || self.result_animations.iter().any(|a| a.progress < 1.0);
        
        if still_animating {
            ctx.request_repaint();
        }
        
        // Close if clicked outside the window
        ctx.input(|i| {
            if i.pointer.any_click() {
                if let Some(pos) = i.pointer.interact_pos() {
                    let rect = ctx.screen_rect();
                    if !rect.contains(pos) {
                        self.should_close = true;
                    }
                }
            }
            
            if let Some(focused) = i.viewport().focused {
                if !focused && i.pointer.any_click() {
                    self.should_close = true;
                }
            }
        });
        
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
                        }
                    }
                }

                // Update search results
                self.results.clear();
                
                if !self.query.is_empty() {
                    // URL detection - check this FIRST
                    if looks_like_url(&self.query) {
                        let url = if self.query.contains("://") {
                            self.query.clone()
                        } else {
                            format!("https://{}", self.query)
                        };
                        self.results.push(ResultType::Url(url));
                    }
                    // Calculator mode
                    else if self.query.starts_with('=') {
                        let expr = self.query[1..].trim();
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
                        let cmd = self.query[1..].trim();
                        if !cmd.is_empty() {
                            self.results.push(ResultType::Command(cmd.to_string()));
                        }
                    }
                    // Web search mode
                    else if self.query.starts_with('@') {
                        let search = self.query[1..].trim();
                        if !search.is_empty() {
                            self.results.push(ResultType::WebSearch(search.to_string()));
                        }
                    }
                    // Normal app search
                    else {
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
                                                    egui::RichText::new(format!("🧮 {}", res))
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

fn looks_like_url(text: &str) -> bool {
    let text = text.trim();
    
    // Common URL patterns
    if text.contains("://") {
        return text.starts_with("http://") || text.starts_with("https://") || 
               text.starts_with("ftp://") || text.starts_with("file://");
    }
    
    // Domain-like patterns
    if text.contains('.') && !text.contains(' ') {
        let parts: Vec<&str> = text.split('.').collect();
        if parts.len() >= 2 {
            let last_part = parts.last().unwrap();
            
            // Comprehensive list of common TLDs
            let common_tlds = [
                // Generic TLDs
                "com", "org", "net", "info", "biz", "name", "pro",
                // Country code TLDs
                "io", "co", "me", "tv", "ai", "dev", "app", "tech", "xyz", "store",
                "online", "site", "website", "space", "club", "fun", "live", "work",
                "cloud", "digital", "media", "news", "blog", "shop", "art", "design",
                "world", "global", "link", "click", "lol", "top", "win", "bid",
                // Traditional country codes
                "us", "uk", "ca", "au", "de", "fr", "jp", "cn", "in", "br", "ru",
                "it", "es", "nl", "se", "no", "dk", "fi", "pl", "ch", "at", "be",
                "ie", "nz", "sg", "hk", "kr", "tw", "mx", "za", "tr", "gr", "pt",
                // More country codes
                "eu", "asia", "africa", "lat", "berlin", "london", "nyc", "tokyo",
                // Newer TLDs
                "guru", "expert", "services", "solutions", "systems", "technology",
                "network", "group", "company", "center", "support", "community",
                "agency", "studio", "exchange", "foundation", "institute", "management",
                "partners", "ventures", "capital", "enterprises", "holdings", "international",
                "market", "tools", "equipment", "supplies", "gallery", "academy",
                "education", "school", "university", "institute", "training", "careers",
                "jobs", "recruitment", "health", "medical", "clinic", "hospital",
                "pharmacy", "dental", "fit", "fitness", "yoga", "travel", "tours",
                "vacations", "holiday", "hotel", "restaurant", "cafe", "bar", "pub",
                "food", "pizza", "sushi", "fashion", "shoes", "clothing", "jewelry",
                "beauty", "hair", "skin", "spa", "salon", "auto", "cars", "bike",
                "boats", "cycles", "motorcycles", "realestate", "properties", "rentals",
                "apartments", "villas", "condos", "construction", "contractors",
                "builders", "engineering", "architecture", "design", "photography",
                "photos", "pictures", "graphics", "art", "music", "film", "movies",
                "theater", "tickets", "events", "shows", "entertainment", "games",
                "gaming", "casino", "poker", "bet", "bingo", "lottery", "sports",
                "football", "soccer", "basketball", "baseball", "hockey", "tennis",
                "golf", "fishing", "hunting", "outdoors", "adventure", "camping",
                "hiking", "biking", "running", "swimming", "yoga", "fitness",
                "finance", "bank", "insurance", "investments", "loans", "credit",
                "money", "capital", "wealth", "trading", "forex", "crypto",
                "bitcoin", "ethereum", "blockchain", "nft", "metaverse",
                "energy", "green", "eco", "solar", "wind", "water", "renewable",
                "organic", "natural", "sustainable", "recycle", "environment",
                "charity", "foundation", "ngo", "nonprofit", "volunteer",
                "government", "gov", "mil", "edu", "ac", "govt", "parliament",
                "law", "legal", "attorney", "lawyer", "justice", "court",
                "security", "safety", "protection", "defense", "army", "navy",
                "airforce", "police", "fire", "rescue", "emergency",
            ];
            
            // Check if the last part matches any common TLD
            return common_tlds.iter().any(|&tld| *last_part == tld) || 
                   last_part.len() == 2 || // Any 2-letter country code
                   last_part.starts_with("xn--"); // Internationalized domain names
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