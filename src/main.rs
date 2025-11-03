use eframe::egui;
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use rayon::prelude::*;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

struct Theme {
    background: String,
    text_color: String,
    selection_bg: String,
    selection_text: String,
    border_color: String,
    font_size: f32,
    border_radius: f32,
    font_family: String,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            background: "#1e1e1e".to_string(),
            text_color: "#d4d4d4".to_string(),
            selection_bg: "#0078d4".to_string(),
            selection_text: "#ffffff".to_string(),
            border_color: "#3c3c3c".to_string(),
            font_size: 16.0,
            border_radius: 0.0,
            font_family: "System".to_string(),
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

struct FlintApp {
    query: String,
    results: Vec<String>,
    items: Vec<String>,
    selected: usize,
    should_close: bool,
    has_focused: bool,
    theme: Theme,
}

impl Default for FlintApp {
    fn default() -> Self {
        let items = scan_desktop_apps();
        
        Self {
            query: String::new(),
            results: items.clone(),
            items,
            selected: 0,
            should_close: false,
            has_focused: false,
            theme: Theme::load_from_config(),
        }
    }
}

impl eframe::App for FlintApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        apply_theme(ctx, &self.theme);
        
        if self.should_close {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            let total_width = 800.0;
            ui.set_width(total_width);
            
            // Clean search area - no box, just text
            ui.vertical_centered(|ui| {
                ui.add_space(15.0);
                
                let text_rgb = self.theme.hex_to_rgb(&self.theme.text_color);
                
                let response = ui.add(
                    egui::TextEdit::singleline(&mut self.query)
                        .hint_text("Type to search...")
                        .desired_width(total_width - 40.0)
                        .text_color(egui::Color32::from_rgb(
                            (text_rgb[0] * 255.0) as u8,
                            (text_rgb[1] * 255.0) as u8,
                            (text_rgb[2] * 255.0) as u8,
                        ))
                        .id(egui::Id::new("search_field")),
                );

                if !self.has_focused {
                    ui.ctx().memory_mut(|mem| mem.request_focus(response.id));
                    self.has_focused = true;
                }
                
                ui.add_space(10.0);
            });

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
                if let Some(app) = self.results.get(self.selected) {
                    launch_app(app);
                    self.should_close = true;
                }
            }

            // Update search results
            if !self.query.is_empty() {
                let matcher = SkimMatcherV2::default();
                let query = self.query.clone();
                
                let mut scored_results: Vec<(i64, String)> = self
                    .items
                    .par_iter()
                    .filter_map(|s| {
                        matcher.fuzzy_match(s, &query)
                            .map(|score| (score, s.clone()))
                    })
                    .collect();
                
                scored_results.sort_by(|a, b| b.0.cmp(&a.0));
                self.results = scored_results.into_iter().map(|(_, s)| s).collect();
                
                if self.selected >= self.results.len() && !self.results.is_empty() {
                    self.selected = 0;
                }
            } else {
                self.results = self.items.clone();
                if self.selected >= self.results.len() && !self.results.is_empty() {
                    self.selected = 0;
                }
            }

            // Show results
            if !self.results.is_empty() {
                egui::ScrollArea::vertical()
                    .max_height(400.0)
                    .show(ui, |ui| {
                        ui.set_width(total_width);
                        
                        for (i, app) in self.results.iter().enumerate() {
                            let is_selected = i == self.selected;
                            
                            let bg_rgb = self.theme.hex_to_rgb(&self.theme.selection_bg);
                            let text_rgb = self.theme.hex_to_rgb(&self.theme.selection_text);
                            let normal_text_rgb = self.theme.hex_to_rgb(&self.theme.text_color);
                            
                            let bg_color = if is_selected {
                                egui::Color32::from_rgb(
                                    (bg_rgb[0] * 255.0) as u8,
                                    (bg_rgb[1] * 255.0) as u8,
                                    (bg_rgb[2] * 255.0) as u8,
                                )
                            } else {
                                egui::Color32::TRANSPARENT
                            };
                            
                            let text_color = if is_selected {
                                egui::Color32::from_rgb(
                                    (text_rgb[0] * 255.0) as u8,
                                    (text_rgb[1] * 255.0) as u8,
                                    (text_rgb[2] * 255.0) as u8,
                                )
                            } else {
                                egui::Color32::from_rgb(
                                    (normal_text_rgb[0] * 255.0) as u8,
                                    (normal_text_rgb[1] * 255.0) as u8,
                                    (normal_text_rgb[2] * 255.0) as u8,
                                )
                            };
                            
                            let button = egui::Button::new(
                                egui::RichText::new(app)
                                    .color(text_color)
                                    .size(self.theme.font_size)
                            )
                            .min_size(egui::vec2(total_width, 36.0))
                            .fill(bg_color)
                            .rounding(self.theme.border_radius);
                            
                            let response = ui.add(button);
                            
                            if response.clicked() {
                                launch_app(app);
                                self.should_close = true;
                            }
                            
                            if response.hovered() {
                                self.selected = i;
                            }
                        }
                    });
            } else if !self.query.is_empty() {
                ui.vertical_centered(|ui| {
                    ui.add_space(20.0);
                    ui.label(
                        egui::RichText::new("No results found")
                            .color(egui::Color32::from_rgb(120, 120, 120))
                            .size(14.0)
                    );
                });
            }
        });

        ctx.request_repaint();
    }
}

fn load_system_font(ctx: &egui::Context, font_family: &str) -> bool {
    if font_family == "System" {
        return true;
    }
    
    let home = std::env::var("HOME").unwrap();
    
    // Create owned strings for font directories
    let font_dirs = vec![
        "/usr/share/fonts".to_string(),
        "/usr/local/share/fonts".to_string(),
        format!("{}/.local/share/fonts", home),
        format!("{}/.fonts", home),
    ];
    
    for font_dir in font_dirs {
        if let Ok(entries) = fs::read_dir(&font_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if let Some(ext) = path.extension() {
                    if ext == "ttf" || ext == "otf" {
                        if let Some(file_name) = path.file_stem() {
                            if file_name.to_string_lossy().to_lowercase().contains(&font_family.to_lowercase()) {
                                if let Ok(font_data) = fs::read(&path) {
                                    let mut fonts = egui::FontDefinitions::default();
                                    fonts.font_data.insert(
                                        font_family.to_string(),
                                        egui::FontData::from_owned(font_data),
                                    );
                                    
                                    // Replace the proportional font family
                                    fonts
                                        .families
                                        .get_mut(&egui::FontFamily::Proportional)
                                        .unwrap()
                                        .insert(0, font_family.to_string());
                                    
                                    ctx.set_fonts(fonts);
                                    return true;
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    
    false
}

fn apply_theme(ctx: &egui::Context, theme: &Theme) {
    let mut visuals = egui::Visuals::dark();
    
    let bg_rgb = theme.hex_to_rgb(&theme.background);
    let border_rgb = theme.hex_to_rgb(&theme.border_color);
    
    visuals.panel_fill = egui::Color32::from_rgb(
        (bg_rgb[0] * 255.0) as u8,
        (bg_rgb[1] * 255.0) as u8,
        (bg_rgb[2] * 255.0) as u8,
    );
    
    visuals.window_fill = egui::Color32::from_rgb(
        (bg_rgb[0] * 255.0) as u8,
        (bg_rgb[1] * 255.0) as u8,
        (bg_rgb[2] * 255.0) as u8,
    );
    
    visuals.window_stroke.color = egui::Color32::from_rgb(
        (border_rgb[0] * 255.0) as u8,
        (border_rgb[1] * 255.0) as u8,
        (border_rgb[2] * 255.0) as u8,
    );
    
    visuals.window_stroke.width = 1.0;
    
    ctx.set_visuals(visuals);
}

fn get_config_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap();
    PathBuf::from(home).join(".config").join("flint")
}

fn create_default_theme(theme_path: &PathBuf) {
    let default_theme = r#"# Flint Theme Configuration
# Use hex colors like #RRGGBB

# Main window colors
background=#1e1e1e
text_color=#d4d4d4
selection_bg=#0078d4
selection_text=#ffffff
border_color=#3c3c3c

# Font settings
# Use "System" for default font, or any installed font name like:
# - "ZedMono Nerd Font Mono" 
# - "Fira Code"
# - "JetBrains Mono"
# - "Cascadia Code"
font_size=16
font_family=System

# Border radius
border_radius=0
"#;
    
    if let Some(parent) = theme_path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let _ = fs::write(theme_path, default_theme);
}

fn launch_app(name: &str) {
    let _ = Command::new("gtk-launch").arg(name).spawn();
}

fn scan_desktop_apps() -> Vec<String> {
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
                        for line in content.lines() {
                            if line.starts_with("Name=") {
                                apps.push(line["Name=".len()..].to_string());
                                break;
                            }
                        }
                    }
                }
            }
        }
    }

    apps.sort();
    apps.dedup();
    apps
}

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([800.0, 500.0])
            .with_decorations(false)
            .with_always_on_top(),
        ..Default::default()
    };

    eframe::run_native(
        "Flint",
        options,
        Box::new(|cc| {
            // Load custom font at startup if specified in config
            let app = FlintApp::default();
            if app.theme.font_family != "System" {
                load_system_font(&cc.egui_ctx, &app.theme.font_family);
            }
            
            cc.egui_ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
            Box::new(app)
        }),
    )
}