use serde::{Deserialize, Serialize};

// ── Constants ──────────────────────────────────────────────

pub fn default_base_path() -> String {
    std::env::var("SGD_BASE_PATH").unwrap_or_else(|_| "./sgd_data".to_string())
}
pub const DB_FILENAME: &str = "documents.db";
pub const SETTINGS_FILENAME: &str = "settings.json";
pub const FILES_SUBDIR: &str = "files";

const DEFAULT_CATEGORIES_INTERNAL: &[(&str, &str, &str)] = &[
    ("PDF", "Documentos PDF", "\u{1F4C4}"),
    ("Excel", "Hojas de calculo Excel", "\u{1F4CA}"),
    ("Docs", "Documentos de Word", "\u{1F4DD}"),
    ("Presentaciones", "Presentaciones PowerPoint", "\u{1F4F9}"),
];

pub fn default_categories() -> &'static [(&'static str, &'static str, &'static str)] {
    DEFAULT_CATEGORIES_INTERNAL
}

pub fn is_default_category(name: &str) -> bool {
    default_categories().iter().any(|(n, _, _)| *n == name)
}

pub fn file_type_to_category_name(ext: &str) -> Option<&'static str> {
    match ext {
        "pdf" => Some("PDF"),
        "xlsx" | "xls" => Some("Excel"),
        "docx" => Some("Docs"),
        "pptx" => Some("Presentaciones"),
        _ => None,
    }
}

pub const CATEGORY_ICONS: &[&str] = &["\u{1F4C1}", "\u{1F4C2}", "\u{1F5C2}", "\u{1F4CE}", "\u{1F4DD}", "\u{1F3F7}"];

pub const MONTH_NAMES_ES: [&str; 12] = ["Enero", "Febrero", "Marzo", "Abril", "Mayo", "Junio", "Julio", "Agosto", "Septiembre", "Octubre", "Noviembre", "Diciembre"];
pub const MONTH_NAMES_EN: [&str; 12] = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];
pub const DAY_NAMES_ES: [&str; 7] = ["Lu", "Ma", "Mi", "Ju", "Vi", "Sa", "Do"];
pub const DAY_NAMES_EN: [&str; 7] = ["Mo", "Tu", "We", "Th", "Fr", "Sa", "Su"];

pub const FILE_TYPE_STYLES: &[(&str, &str, (u8, u8, u8))] = &[
    ("pdf", "\u{1F4C4}", (200, 50, 50)),
    ("xlsx", "\u{1F4CA}", (30, 140, 60)),
    ("xls", "\u{1F4CA}", (30, 140, 60)),
    ("docx", "\u{1F4DD}", (40, 80, 160)),
    ("pptx", "\u{1F4F9}", (200, 100, 30)),
];
pub const DEFAULT_FILE_TYPE_STYLE: (&str, (u8, u8, u8)) = ("\u{1F4C4}", (128, 128, 128));

pub fn category_icon(name: &str) -> &'static str {
    default_categories().iter()
        .find(|(n, _, _)| *n == name)
        .map(|(_, _, icon)| *icon)
        .unwrap_or("")
}



#[derive(Debug, Clone, Default)]
pub struct Document {
    pub id: String,
    pub name: String,
    pub file_type: String,
    pub file_path: String,
    pub original_name: String,
    pub size: i64,
    pub description: String,
    pub notes: String,
    pub checksum: String,
    pub created_at: String,
    pub updated_at: String,
    pub favorite: bool,
    pub deleted_at: Option<String>,
    pub content_text: String,
}

#[derive(Debug, Clone, Default)]
pub struct Category {
    pub id: String,
    pub name: String,
    pub description: String,
    pub icon: String,
}

#[derive(Debug, Clone, Default)]
pub struct Template {
    pub id: String,
    pub name: String,
    pub description: String,
}

#[derive(Debug, Clone, Default)]
#[allow(dead_code)]
pub struct HistoryEntry {
    pub id: String,
    pub action_type: String,
    pub action_label: String,
    pub document_id: Option<String>,
    pub timestamp: String,
}

#[derive(Debug, Clone)]
pub struct DocumentRelation {
    pub id: String,
    pub source_id: String,
    pub target_id: String,
    pub relation_type: String,
}

#[derive(Debug, Clone)]
pub struct AutoRule {
    pub id: String,
    pub name: String,
    pub pattern: String,
    pub category_id: String,
}

#[derive(Debug, Clone)]
pub struct Reminder {
    pub id: String,
    pub document_id: String,
    pub note: String,
    pub due_date: String,
    pub done: bool,
}

#[derive(Debug, Clone)]
pub struct DocumentVersion {
    pub id: String,
    pub document_id: String,
    pub file_path: String,
    pub size: i64,
    pub checksum: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Default)]
pub struct UndoAction {
    pub action_type: String,
    #[allow(dead_code)]
    pub description: String,
    pub document_id: Option<String>,
    pub old_doc: Option<Document>,
    pub old_categories: Option<Vec<String>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Theme {
    Light, Dark, HighContrast, Forest, Ocean, Sunset, Midnight, Lavender,
    Coral, Graphite, Retro, Terminal, Halloween, Navidad, ElAri,
}

impl Theme {
    pub fn list() -> Vec<(Theme, &'static str, &'static str)> {
        vec![
            (Theme::Light, "Claro", "Light"),
            (Theme::Dark, "Oscuro", "Dark"),
            (Theme::HighContrast, "Alto Contraste", "High Contrast"),
            (Theme::Forest, "Bosque", "Forest"),
            (Theme::Ocean, "Oceano", "Ocean"),
            (Theme::Sunset, "Atardecer", "Sunset"),
            (Theme::Midnight, "Medianoche", "Midnight"),
            (Theme::Lavender, "Lavanda", "Lavender"),
            (Theme::Coral, "Coral", "Coral"),
            (Theme::Graphite, "Grafito", "Graphite"),
            (Theme::Retro, "Retro", "Retro"),
            (Theme::Terminal, "Terminal", "Terminal"),
            (Theme::Halloween, "Halloween", "Halloween"),
            (Theme::Navidad, "Navidad", "Christmas"),
            (Theme::ElAri, "El Ari", "El Ari"),
        ]
    }

    pub fn preview_color(self) -> egui::Color32 {
        match self {
            Theme::Light => egui::Color32::from_rgb(235, 235, 235),
            Theme::Dark => egui::Color32::from_rgb(40, 40, 45),
            Theme::HighContrast => egui::Color32::WHITE,
            Theme::Forest => egui::Color32::from_rgb(70, 155, 70),
            Theme::Ocean => egui::Color32::from_rgb(55, 115, 195),
            Theme::Sunset => egui::Color32::from_rgb(215, 115, 45),
            Theme::Midnight => egui::Color32::from_rgb(28, 28, 65),
            Theme::Lavender => egui::Color32::from_rgb(155, 115, 200),
            Theme::Coral => egui::Color32::from_rgb(255, 127, 80),
            Theme::Graphite => egui::Color32::from_rgb(80, 80, 85),
            Theme::Retro => egui::Color32::from_rgb(210, 190, 160),
            Theme::Terminal => egui::Color32::from_rgb(0, 180, 0),
            Theme::Halloween => egui::Color32::from_rgb(200, 80, 0),
            Theme::Navidad => egui::Color32::from_rgb(180, 40, 40),
            Theme::ElAri => egui::Color32::from_rgb(97, 6, 59),
        }
    }

    pub fn preview_colors(self) -> Vec<egui::Color32> {
        match self {
            Theme::Halloween => vec![
                egui::Color32::from_rgb(200, 80, 0),
                egui::Color32::from_rgb(80, 0, 0),
            ],
            Theme::Navidad => vec![
                egui::Color32::from_rgb(180, 40, 40),
                egui::Color32::from_rgb(40, 140, 40),
            ],
            _ => vec![self.preview_color()],
        }
    }

    pub fn to_visuals(self) -> egui::Visuals {
        match self {
            Theme::Light => egui::Visuals::light(),
            Theme::Dark => egui::Visuals::dark(),
            Theme::HighContrast => {
                let mut v = egui::Visuals::light();
                v.widgets.noninteractive.bg_fill = egui::Color32::WHITE;
                v.override_text_color = Some(egui::Color32::BLACK);
                v.window_stroke = egui::Stroke::new(2.0, egui::Color32::BLACK);
                v
            }
            Theme::Forest => {
                let mut v = egui::Visuals::light();
                v.window_fill = egui::Color32::from_rgb(235, 248, 235);
                v.panel_fill = egui::Color32::from_rgb(225, 243, 225);
                v.hyperlink_color = egui::Color32::from_rgb(0, 100, 0);
                v.selection.bg_fill = egui::Color32::from_rgb(150, 200, 150);
                v
            }
            Theme::Ocean => {
                let mut v = egui::Visuals::light();
                v.window_fill = egui::Color32::from_rgb(235, 242, 255);
                v.panel_fill = egui::Color32::from_rgb(225, 235, 250);
                v.hyperlink_color = egui::Color32::from_rgb(0, 60, 140);
                v.selection.bg_fill = egui::Color32::from_rgb(140, 180, 230);
                v
            }
            Theme::Sunset => {
                let mut v = egui::Visuals::light();
                v.window_fill = egui::Color32::from_rgb(255, 240, 230);
                v.panel_fill = egui::Color32::from_rgb(255, 233, 218);
                v.hyperlink_color = egui::Color32::from_rgb(180, 60, 0);
                v.selection.bg_fill = egui::Color32::from_rgb(255, 180, 130);
                v
            }
            Theme::Midnight => {
                let mut v = egui::Visuals::dark();
                v.window_fill = egui::Color32::from_rgb(18, 18, 36);
                v.panel_fill = egui::Color32::from_rgb(24, 24, 48);
                v.hyperlink_color = egui::Color32::from_rgb(120, 160, 255);
                v.selection.bg_fill = egui::Color32::from_rgb(60, 60, 130);
                v
            }
            Theme::Lavender => {
                let mut v = egui::Visuals::light();
                v.window_fill = egui::Color32::from_rgb(248, 240, 255);
                v.panel_fill = egui::Color32::from_rgb(242, 232, 252);
                v.hyperlink_color = egui::Color32::from_rgb(100, 40, 160);
                v.selection.bg_fill = egui::Color32::from_rgb(200, 160, 230);
                v
            }
            Theme::Coral => {
                let mut v = egui::Visuals::light();
                v.window_fill = egui::Color32::from_rgb(255, 237, 230);
                v.panel_fill = egui::Color32::from_rgb(255, 225, 215);
                v.hyperlink_color = egui::Color32::from_rgb(200, 60, 30);
                v.selection.bg_fill = egui::Color32::from_rgb(255, 160, 130);
                v
            }
            Theme::Graphite => {
                let mut v = egui::Visuals::light();
                v.window_fill = egui::Color32::from_rgb(240, 240, 242);
                v.panel_fill = egui::Color32::from_rgb(230, 230, 234);
                v.hyperlink_color = egui::Color32::from_rgb(40, 40, 60);
                v.selection.bg_fill = egui::Color32::from_rgb(160, 160, 175);
                v.widgets.noninteractive.bg_fill = egui::Color32::from_rgb(220, 220, 225);
                v
            }
            Theme::Retro => {
                let mut v = egui::Visuals::light();
                v.window_fill = egui::Color32::from_rgb(245, 235, 215);
                v.panel_fill = egui::Color32::from_rgb(235, 222, 200);
                v.hyperlink_color = egui::Color32::from_rgb(120, 80, 30);
                v.selection.bg_fill = egui::Color32::from_rgb(200, 180, 140);
                v.widgets.noninteractive.bg_fill = egui::Color32::from_rgb(225, 212, 185);
                v
            }
            Theme::Terminal => {
                let mut v = egui::Visuals::dark();
                v.window_fill = egui::Color32::from_rgb(10, 20, 10);
                v.panel_fill = egui::Color32::from_rgb(15, 28, 15);
                v.hyperlink_color = egui::Color32::from_rgb(100, 255, 100);
                v.selection.bg_fill = egui::Color32::from_rgb(0, 100, 0);
                v.override_text_color = Some(egui::Color32::from_rgb(0, 220, 0));
                v.widgets.noninteractive.bg_fill = egui::Color32::from_rgb(20, 40, 20);
                v.widgets.inactive.bg_fill = egui::Color32::from_rgb(20, 50, 20);
                v.widgets.active.bg_fill = egui::Color32::from_rgb(0, 80, 0);
                v
            }
            Theme::Halloween => {
                let mut v = egui::Visuals::dark();
                v.window_fill = egui::Color32::from_rgb(20, 10, 10);
                v.panel_fill = egui::Color32::from_rgb(30, 15, 10);
                v.hyperlink_color = egui::Color32::from_rgb(255, 140, 0);
                v.selection.bg_fill = egui::Color32::from_rgb(180, 60, 0);
                v.override_text_color = Some(egui::Color32::from_rgb(255, 200, 150));
                v.widgets.noninteractive.bg_fill = egui::Color32::from_rgb(40, 20, 15);
                v.widgets.inactive.bg_fill = egui::Color32::from_rgb(60, 30, 20);
                v.widgets.active.bg_fill = egui::Color32::from_rgb(200, 80, 0);
                v
            }
            Theme::Navidad => {
                let mut v = egui::Visuals::dark();
                v.window_fill = egui::Color32::from_rgb(10, 20, 10);
                v.panel_fill = egui::Color32::from_rgb(15, 30, 15);
                v.hyperlink_color = egui::Color32::from_rgb(255, 200, 200);
                v.selection.bg_fill = egui::Color32::from_rgb(160, 40, 40);
                v.override_text_color = Some(egui::Color32::from_rgb(220, 255, 220));
                v.widgets.noninteractive.bg_fill = egui::Color32::from_rgb(20, 40, 20);
                v.widgets.inactive.bg_fill = egui::Color32::from_rgb(30, 60, 30);
                v.widgets.active.bg_fill = egui::Color32::from_rgb(180, 40, 40);
                v
            }
            Theme::ElAri => {
                let mut v = egui::Visuals::light();
                v.window_fill = egui::Color32::from_rgb(245, 235, 245);
                v.panel_fill = egui::Color32::from_rgb(240, 228, 240);
                v.hyperlink_color = egui::Color32::from_rgb(130, 20, 80);
                v.selection.bg_fill = egui::Color32::from_rgb(180, 100, 150);
                v.widgets.noninteractive.bg_fill = egui::Color32::from_rgb(235, 220, 235);
                v.override_text_color = Some(egui::Color32::from_rgb(60, 10, 40));
                v
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Language {
    Spanish,
    English,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Settings {
    pub theme: Theme,
    pub language: Language,
    pub confirm_delete: bool,
    pub auto_open_after_import: bool,
    pub font_size: f32,
    pub reduced_motion: bool,
    pub table_density: f32,
    pub show_column_type: bool,
    pub show_column_size: bool,
    pub show_column_date: bool,
    pub trash_auto_delete_days: i64,
    pub backup_enabled: bool,
    pub backup_interval_hours: i64,
    pub backup_path: String,
    pub watch_folder_enabled: bool,
    pub watch_folder_path: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: Theme::Light,
            language: Language::Spanish,
            confirm_delete: true,
            auto_open_after_import: false,
            font_size: 14.0,
            reduced_motion: false,
            table_density: 30.0,
            show_column_type: true,
            show_column_size: true,
            show_column_date: true,
            trash_auto_delete_days: 30,
            backup_enabled: false,
            backup_interval_hours: 24,
            backup_path: String::new(),
            watch_folder_enabled: false,
            watch_folder_path: String::new(),
        }
    }
}

impl Settings {
    pub fn load(path: &std::path::Path) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, path: &std::path::Path) {
        if let Ok(content) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(path, content);
        }
    }
}

pub fn default_category_names() -> Vec<&'static str> {
    default_categories().iter().map(|(name, _, _)| *name).collect()
}
