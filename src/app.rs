use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Instant;

use calamine::Reader;
use chrono::Datelike;
use eframe::egui;
use rusqlite::Connection;

use crate::db;
use crate::models::{
    AutoRule, Category, category_icon, CATEGORY_ICONS, DAY_NAMES_EN, DAY_NAMES_ES, DB_FILENAME,
    DEFAULT_FILE_TYPE_STYLE, default_base_path, default_categories, default_category_names,
    FILE_TYPE_STYLES, MONTH_NAMES_EN, MONTH_NAMES_ES, Document, DocumentRelation,
    DocumentVersion, file_type_to_category_name, HistoryEntry, is_default_category, Language,
    Reminder, SETTINGS_FILENAME, Settings, Template, Theme, UndoAction,
};
use crate::storage::Storage;

/// Placeholder para i18n. Actualmente siempre devuelve español.
/// En el futuro, implementar lookup real con el Language proporcionado.
fn tr(_lang: Language, es: &'static str, _en: &'static str) -> &'static str {
    es
}

macro_rules! tr_fmt {
    ($l:expr, $es:literal, $en:literal $(, $arg:expr)* $(,)?) => {{
        match $l {
            Language::Spanish => format!($es $(, $arg)*),
            Language::English => format!($en $(, $arg)*),
        }
    }};
}

const SIZE_UNITS: &[&str] = &["B", "KB", "MB", "GB"];

fn format_size(size: i64) -> String {
    let mut s = size as f64;
    for &unit in SIZE_UNITS {
        if s < 1024.0 || unit == "GB" {
            return if unit == "B" { format!("{} {}", s as i64, unit) }
                   else { format!("{:.1} {}", s, unit) };
        }
        s /= 1024.0;
    }
    format!("{} GB", s)
}

fn month_name_es(m: u32) -> &'static str {
    MONTH_NAMES_ES.get(m as usize - 1).copied().unwrap_or("")
}
fn month_name_en(m: u32) -> &'static str {
    MONTH_NAMES_EN.get(m as usize - 1).copied().unwrap_or("")
}
fn month_name(l: Language, m: u32) -> &'static str {
    match l { Language::Spanish => month_name_es(m), Language::English => month_name_en(m) }
}



fn get_extension(path: impl AsRef<Path>) -> String {
    path.as_ref().extension().unwrap_or_default().to_string_lossy().to_lowercase()
}

fn get_file_stem(path: impl AsRef<Path>) -> String {
    path.as_ref().file_stem().unwrap_or_default().to_string_lossy().to_string()
}

fn file_type_style(ft: &str) -> (&'static str, egui::Color32) {
    let (icon, (r, g, b)) = FILE_TYPE_STYLES.iter().find(|(t, _, _)| *t == ft)
        .map(|(_, icon, rgb)| (*icon, *rgb))
        .unwrap_or(DEFAULT_FILE_TYPE_STYLE);
    (icon, egui::Color32::from_rgb(r, g, b))
}

fn days_in_month(year: i32, month: u32) -> u32 {
    let next = if month == 12 { chrono::NaiveDate::from_ymd_opt(year + 1, 1, 1) }
               else { chrono::NaiveDate::from_ymd_opt(year, month + 1, 1) };
    next.and_then(|d| d.pred_opt()).map(|d| d.day()).unwrap_or(0)
}

fn extract_text(path: &Path) -> String {
    let ext = get_extension(path);
    match ext.as_str() {
        "pdf" => pdf_extract::extract_text(path).unwrap_or_default(),
        "xlsx" | "xls" => {
            if let Ok(mut wb) = calamine::open_workbook_auto(path) {
                let mut text = String::new();
                let sheet_names = wb.sheet_names().to_vec();
                for name in sheet_names {
                    if let Ok(range) = wb.worksheet_range(&name) {
                        for row in range.rows() {
                            for cell in row {
                                text.push_str(&format!("{} ", cell.to_string()));
                            }
                            text.push('\n');
                        }
                    }
                }
                text
            } else { String::new() }
        }
        _ => String::new(),
    }
}

const HIGHLIGHT_MARKER: &str = "\u{1F7E2}";

fn highlight_text(text: &str, query: &str) -> String {
    if query.is_empty() { return text.to_string(); }
    let lower_text = text.to_lowercase();
    let lower_query = query.to_lowercase();
    let mut result = String::new();
    let mut last_end = 0;
    for (start, _) in lower_text.match_indices(&lower_query) {
        if start > last_end {
            result.push_str(&text[last_end..start]);
        }
        result.push_str(HIGHLIGHT_MARKER);
        result.push_str(&text[start..start + query.len()]);
        result.push_str(HIGHLIGHT_MARKER);
        last_end = start + query.len();
    }
    if last_end < text.len() {
        result.push_str(&text[last_end..]);
    }
    result
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SortField { Name, Date, Size, Type }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SortOrder { Ascending, Descending }

#[derive(Debug, Clone, PartialEq, Eq)]
enum SidebarSection { All, Favorites, Recent, Trash, Category(String) }

#[derive(Debug, Clone, PartialEq)]
struct FilterState {
    enabled: bool,
    pdf: bool,
    excel: bool,
    docs: bool,
    pptx: bool,
    size_min: String,
    size_max: String,
    date_from: String,
    date_to: String,
}

impl Default for FilterState {
    fn default() -> Self {
        Self { enabled: false, pdf: true, excel: true, docs: true, pptx: true,
               size_min: String::new(), size_max: String::new(),
               date_from: String::new(), date_to: String::new() }
    }
}



pub struct SgdApp {
    db: Connection,
    storage: Storage,
    settings: Settings,
    settings_path: PathBuf,

    documents: Vec<Document>,
    categories: Vec<Category>,
    category_counts: HashMap<String, i64>,
    all_counts: HashMap<String, i64>,
    templates: Vec<Template>,
    history_dates: Vec<String>,
    history_entries: Vec<HistoryEntry>,

    search_query: String,
    sidebar_section: SidebarSection,
    filter_state: FilterState,

    sort_field: SortField,
    sort_order: SortOrder,

    // Dialogs
    show_add_dialog: bool,
    show_category_dialog: bool,
    show_edit_dialog: bool,
    show_settings_dialog: bool,
    show_template_dialog: bool,
    show_history_dialog: bool,
    show_theme_selector: bool,
    show_categories_popup: bool,
    show_add_cat_popup: bool,
    show_stats_dialog: bool,
    show_filters_dialog: bool,
    show_relations_dialog: bool,
    show_auto_rules_dialog: bool,
    show_reminders_dialog: bool,
    show_versions_dialog: bool,
    show_backup_dialog: bool,
    preview_doc_id: String,  // used by relations, reminders, versions

    // Add dialog
    add_name: String,
    add_description: String,
    add_file_path: Option<PathBuf>,
    add_selected_cats: Vec<String>,
    add_selected_template: Option<String>,

    // Category
    new_cat_name: String,
    new_cat_description: String,
    new_cat_icon: String,

    // Template
    new_tpl_name: String,
    new_tpl_description: String,

    // Edit
    edit_doc_id: String,
    edit_name: String,
    edit_description: String,
    edit_notes: String,
    edit_selected_cats: Vec<String>,

    // Multi-select
    selected_docs: std::collections::HashSet<String>,

    // Relations
    relations_for_current: Vec<DocumentRelation>,
    related_docs: Vec<Document>,
    all_docs_for_relation: Vec<Document>,
    new_relation_doc_id: String,
    new_relation_type: String,

    // Auto rules
    auto_rules: Vec<AutoRule>,
    new_rule_name: String,
    new_rule_pattern: String,
    new_rule_cat_id: String,

    // Reminders
    reminders_for_current: Vec<Reminder>,
    doc_ids_with_reminders: HashSet<String>,
    new_reminder_note: String,
    new_reminder_date: String,
    pending_reminders: Vec<Reminder>,

    // Versions
    versions_for_current: Vec<DocumentVersion>,

    // Undo
    undo_stack: Vec<UndoAction>,

    // Folder watcher
    watch_rx: Option<mpsc::Receiver<notify::Event>>,
    #[allow(dead_code)]
    watch_handle: Option<std::thread::JoinHandle<()>>,

    // Status
    status_message: String,
    status_clear_at: Option<Instant>,
    needs_refresh: bool,
    showing_trash: bool,

    // History calendar
    history_year: i32,
    history_month: u32,
    history_selected_day: Option<u32>,

    recent_docs: Vec<Document>,
}

impl SgdApp {
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let base_path = PathBuf::from(default_base_path());
        let storage = Storage::new(&base_path);
        storage.init()?;

        let settings_path = base_path.join(SETTINGS_FILENAME);
        let settings = Settings::load(&settings_path);

        let db_path = base_path.join(DB_FILENAME);
        let db = db::init_db(db_path.to_str().ok_or("Invalid database path (non-UTF8)")?)?;
        db::ensure_default_categories(&db)?;

        // Auto-clean trash
        if settings.trash_auto_delete_days > 0 {
            if let Ok(docs) = db::delete_trashed_older_than(&db, settings.trash_auto_delete_days) {
                for doc in docs {
                    let _ = storage.delete_file(&doc.file_path);
                }
            }
        }

        let categories = db::get_all_categories(&db)?;
        let category_counts = db::get_document_counts_by_category(&db)?;
        let documents = db::get_all_documents(&db)?;
        let templates = db::get_all_templates(&db)?;
        let history_dates = db::get_history_dates(&db).unwrap_or_default();
        let recent_docs = db::get_recent_documents(&db, 5).unwrap_or_default();
        let all_counts = db::get_all_document_counts(&db).unwrap_or_default();
        let auto_rules = db::get_all_auto_rules(&db).unwrap_or_default();
        let pending_reminders = db::get_all_pending_reminders(&db).unwrap_or_default();

        // Start folder watcher if enabled
        let (watch_rx, watch_handle) = if settings.watch_folder_enabled && !settings.watch_folder_path.is_empty() {
            Self::start_watcher(&settings.watch_folder_path)
        } else { (None, None) };

        let now = chrono::Local::now();
        let history_year = now.year();
        let history_month = now.month();

        let app = Self {
            db, storage, settings, settings_path,
            documents, categories, category_counts, all_counts,
            templates, history_dates,
            history_entries: Vec::new(),
            search_query: String::new(),
            sidebar_section: SidebarSection::All,
            filter_state: FilterState::default(),
            sort_field: SortField::Date,
            sort_order: SortOrder::Descending,
            show_add_dialog: false,
            show_category_dialog: false,
            show_edit_dialog: false,
            show_settings_dialog: false,
            show_template_dialog: false,
            show_history_dialog: false,
            show_theme_selector: false,
            show_categories_popup: false,
            show_add_cat_popup: false,
            show_stats_dialog: false,
            show_filters_dialog: false,
            show_relations_dialog: false,
            show_auto_rules_dialog: false,
            show_reminders_dialog: false,
            show_versions_dialog: false,
            show_backup_dialog: false,
            preview_doc_id: String::new(),
            add_name: String::new(),
            add_description: String::new(),
            add_file_path: None,
            add_selected_cats: Vec::new(),
            add_selected_template: None,
            new_cat_name: String::new(),
            new_cat_description: String::new(),
            new_cat_icon: String::new(),
            new_tpl_name: String::new(),
            new_tpl_description: String::new(),
            edit_doc_id: String::new(),
            edit_name: String::new(),
            edit_description: String::new(),
            edit_notes: String::new(),
            edit_selected_cats: Vec::new(),
            selected_docs: std::collections::HashSet::new(),
            relations_for_current: Vec::new(),
            related_docs: Vec::new(),
            all_docs_for_relation: Vec::new(),
            new_relation_doc_id: String::new(),
            new_relation_type: "related".to_string(),
            auto_rules,
            new_rule_name: String::new(),
            new_rule_pattern: String::new(),
            new_rule_cat_id: String::new(),
            reminders_for_current: Vec::new(),
            doc_ids_with_reminders: HashSet::new(),
            new_reminder_note: String::new(),
            new_reminder_date: String::new(),
            pending_reminders,
            versions_for_current: Vec::new(),
            undo_stack: Vec::new(),
            watch_rx,
            watch_handle,
            status_message: String::new(),
            status_clear_at: None,
            needs_refresh: false,
            showing_trash: false,
            history_year, history_month, history_selected_day: None,
            recent_docs,
        };
        app.ensure_category_dirs();
        Ok(app)
    }

    fn start_watcher(path: &str) -> (Option<mpsc::Receiver<notify::Event>>, Option<std::thread::JoinHandle<()>>) {
        use notify::{Config, RecommendedWatcher, RecursiveMode, Watcher};
        let (tx, rx) = mpsc::channel();
        match RecommendedWatcher::new(move |res| {
            if let Ok(event) = res { let _ = tx.send(event); }
        }, Config::default()) {
            Ok(mut watcher) => {
                let watch_path = path.to_string();
                let handle = std::thread::spawn(move || {
                    if let Ok(p) = PathBuf::from(&watch_path).canonicalize() {
                        let _ = watcher.watch(&p, RecursiveMode::Recursive);
                    }
                    loop { std::thread::sleep(std::time::Duration::from_secs(1)); }
                });
                (Some(rx), Some(handle))
            }
            Err(_) => (None, None),
        }
    }

    fn set_status(&mut self, msg: String) {
        self.status_message = msg;
        self.status_clear_at = Some(Instant::now() + std::time::Duration::from_secs(6));
    }

    fn status_msg(&mut self, l: Language, es: String, en: String) {
        self.set_status(match l { Language::Spanish => es, Language::English => en });
    }

    fn log_history(&self, action_type: &str, action_label: &str, document_id: Option<&str>) {
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
        let _ = db::insert_history(&self.db, &id, action_type, action_label, document_id, &now);
    }

    fn push_undo(&mut self, action: UndoAction) {
        self.undo_stack.push(action);
        if self.undo_stack.len() > 50 { self.undo_stack.remove(0); }
    }

    fn refresh_data(&mut self) {
        self.categories = db::get_all_categories(&self.db).unwrap_or_default();
        self.category_counts = db::get_document_counts_by_category(&self.db).unwrap_or_default();
        self.all_counts = db::get_all_document_counts(&self.db).unwrap_or_default();
        self.templates = db::get_all_templates(&self.db).unwrap_or_default();
        self.history_dates = db::get_history_dates(&self.db).unwrap_or_default();
        self.recent_docs = db::get_recent_documents(&self.db, 5).unwrap_or_default();
        self.auto_rules = db::get_all_auto_rules(&self.db).unwrap_or_default();
        self.pending_reminders = db::get_all_pending_reminders(&self.db).unwrap_or_default();
        self.ensure_category_dirs();
        self.selected_docs.clear();

        if let SidebarSection::Category(ref cat_id) = self.sidebar_section {
            if !self.categories.iter().any(|c| c.id == *cat_id) {
                self.sidebar_section = SidebarSection::All;
            }
        }
        self.load_documents();
    }

    fn load_documents(&mut self) {
        self.documents = match &self.sidebar_section {
            SidebarSection::Trash => db::get_trashed_documents(&self.db).unwrap_or_default(),
            SidebarSection::Favorites => db::get_favorite_documents(&self.db).unwrap_or_default(),
            SidebarSection::Recent => db::get_recent_documents(&self.db, 20).unwrap_or_default(),
            SidebarSection::Category(cat_id) => {
                if !self.search_query.is_empty() {
                    db::search_documents_by_category(&self.db, &self.search_query, cat_id).unwrap_or_default()
                } else {
                    db::get_documents_by_category(&self.db, cat_id).unwrap_or_default()
                }
            }
            SidebarSection::All => {
                if !self.search_query.is_empty() {
                    db::search_documents(&self.db, &self.search_query).unwrap_or_default()
                } else {
                    db::get_all_documents(&self.db).unwrap_or_default()
                }
            }
        };
        // Apply filters
        if self.filter_state.enabled {
            let f = &self.filter_state;
            let size_min = f.size_min.parse::<i64>().ok();
            let size_max = f.size_max.parse::<i64>().ok();
            self.documents.retain(|d| {
                let type_ok = (f.pdf && d.file_type == "pdf")
                    || (f.excel && (d.file_type == "xlsx" || d.file_type == "xls"))
                    || (f.docs && d.file_type == "docx")
                    || (f.pptx && d.file_type == "pptx");
                let size_ok = size_min.map_or(true, |min| d.size >= min)
                    && size_max.map_or(true, |max| d.size <= max);
                let date_ok = if f.date_from.is_empty() && f.date_to.is_empty() {
                    true
                } else {
                    let d_date = &d.created_at[..10];
                    let from_ok = f.date_from.is_empty() || d_date >= f.date_from.as_str();
                    let to_ok = f.date_to.is_empty() || d_date <= f.date_to.as_str();
                    from_ok && to_ok
                };
                type_ok && size_ok && date_ok
            });
        }
        self.doc_ids_with_reminders = db::get_all_pending_reminders(&self.db).unwrap_or_default()
            .into_iter().map(|r| r.document_id).collect();
        self.showing_trash = matches!(self.sidebar_section, SidebarSection::Trash);
        self.sort_documents();
    }

    fn sort_documents(&mut self) {
        self.documents.sort_by(|a, b| {
            let cmp = match self.sort_field {
                SortField::Name => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
                SortField::Date => a.created_at.cmp(&b.created_at),
                SortField::Size => a.size.cmp(&b.size),
                SortField::Type => a.file_type.cmp(&b.file_type),
            };
            match self.sort_order { SortOrder::Ascending => cmp, SortOrder::Descending => cmp.reverse() }
        });
    }

    fn save_settings(&self) { self.settings.save(&self.settings_path); }

    fn sanitize_folder_name(name: &str) -> String {
        name.to_lowercase().chars().map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' }).collect::<String>()
    }

    fn resolve_storage_subdir(&self, category_ids: &[String], source_path: &Path) -> String {
        if let Some(first_cat_id) = category_ids.first() {
            if let Some(cat) = self.categories.iter().find(|c| &c.id == first_cat_id) {
                return Self::sanitize_folder_name(&cat.name);
            }
        }
        file_type_to_category_name(&get_extension(source_path))
            .map(|name| name.to_lowercase())
            .unwrap_or_else(|| "otros".to_string())
    }

    fn ensure_category_dirs(&self) {
        for cat in &self.categories {
            let _ = self.storage.ensure_subdir(&Self::sanitize_folder_name(&cat.name));
        }
        for name in default_category_names() {
            let _ = self.storage.ensure_subdir(&name.to_lowercase());
        }
        let _ = self.storage.ensure_subdir("otros");
    }

    fn auto_select_categories_for_path(&mut self, path: &Path) {
        self.add_selected_cats.clear();
        let ext = get_extension(path);
        // First check auto-rules
        if let Ok(matched) = db::match_auto_rules(&self.db, &path.to_string_lossy()) {
            if !matched.is_empty() {
                self.add_selected_cats = matched;
                return;
            }
        }
        if let Some(cat_name) = file_type_to_category_name(&ext) {
            if let Some(cat) = self.categories.iter().find(|c| c.name == cat_name) {
                self.add_selected_cats.push(cat.id.clone());
            }
        }
    }

    fn import_document(&mut self, source_path: &Path, name: &str, description: &str, category_ids: &[String]) -> bool {
        let l = self.settings.language;
        let subdir = self.resolve_storage_subdir(category_ids, source_path);
        let result = match self.storage.import_file(source_path, &subdir) {
            Ok((rel_path, original_name, size)) => {
                let file_type = get_extension(source_path);
                let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
                let id = uuid::Uuid::new_v4().to_string();
                let content_text = extract_text(source_path);
                let checksum = self.storage.calculate_checksum(source_path).unwrap_or_default();

                let doc = Document {
                    id: id.clone(), name: name.to_string(), file_type,
                    file_path: rel_path, original_name, size,
                    description: description.to_string(), notes: String::new(),
                    checksum,
                    created_at: now.clone(), updated_at: now.clone(),
                    favorite: false, deleted_at: None, content_text,
                };

                if db::insert_document(&self.db, &doc).is_ok() {
                    if !category_ids.is_empty() {
                        let _ = db::set_document_categories(&self.db, &id, category_ids);
                    }
                    let label = format!("{} '{}'", tr(l, "Documento agregado", "Document added"), doc.name);
                    self.log_history("add", &label, Some(&id));
                    self.status_msg(l,
                        format!("Documento '{}' agregado exitosamente.", doc.name),
                        format!("Document '{}' added successfully.", doc.name),
                    );
                    if self.settings.auto_open_after_import {
                        let full_path = self.storage.get_full_path(&doc.file_path);
                        let _ = opener::open(full_path);
                    }
                    true
                } else {
                    self.status_msg(l,
                        "Error al guardar el documento.".to_string(),
                        "Error saving document.".to_string(),
                    );
                    false
                }
            }
            Err(e) => {
                self.status_msg(l,
                        format!("Error al importar archivo: {}", e),
                        format!("Error importing file: {}", e),
                    );
                false
            }
        };
        self.needs_refresh = true;
        result
    }

    fn import_document_full(&mut self, source_path: &Path) -> bool {
        let ext = get_extension(source_path);
        if !matches!(ext.as_str(), "pdf" | "xlsx" | "xls" | "docx" | "pptx") { return false; }
        let name = get_file_stem(source_path);
        let mut cats = Vec::new();
        if let Ok(matched) = db::match_auto_rules(&self.db, &source_path.to_string_lossy()) {
            if !matched.is_empty() { cats = matched; }
        }
        if cats.is_empty() {
            if let Some(cat_name) = file_type_to_category_name(&ext) {
                if let Some(cat) = self.categories.iter().find(|c| c.name == cat_name) {
                    cats.push(cat.id.clone());
                }
            }
        }
        self.import_document(source_path, &name, "", &cats)
    }

    fn handle_dropped_files(&mut self, ctx: &egui::Context) {
        let dropped = ctx.input(|i| i.raw.dropped_files.clone());
        if dropped.is_empty() { return; }
        let l = self.settings.language;
        for file in &dropped {
            if let Some(path) = &file.path {
                if path.is_dir() {
                    self.import_folder_recursive(path);
                    continue;
                }
                let ext = get_extension(path);
                if !matches!(ext.as_str(), "pdf" | "xlsx" | "xls" | "docx" | "pptx") {
                    self.status_msg(l,
                        format!("Formato no soportado: '.{}'", ext),
                        format!("Unsupported format: '.{}'", ext),
                    );
                    continue;
                }
                if self.show_add_dialog {
                    let name = get_file_stem(path);
                    self.add_file_path = Some(path.clone());
                    self.add_name = name;
                    self.add_description.clear();
                    self.add_selected_template = None;
                    self.auto_select_categories_for_path(path);
                    self.status_message.clear();
                    self.status_clear_at = None;
                } else {
                    self.import_document_full(path);
                }
            }
        }
    }

    fn import_folder_recursive(&mut self, dir: &Path) -> usize {
        let mut count = 0;
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    count += self.import_folder_recursive(&path);
                } else {
                    if self.import_document_full(&path) {
                        count += 1;
                    }
                }
            }
        }
        count
    }

    fn handle_watcher_events(&mut self) {
        let paths: Vec<PathBuf> = if let Some(ref rx) = self.watch_rx {
            let mut p = Vec::new();
            while let Ok(event) = rx.try_recv() {
                for path in &event.paths {
                    if path.is_file() {
                        let ext = get_extension(path);
                        if matches!(ext.as_str(), "pdf" | "xlsx" | "xls" | "docx" | "pptx") {
                            if !self.documents.iter().any(|d| d.original_name == path.file_name().unwrap_or_default().to_string_lossy().to_string()) {
                                p.push(path.clone());
                            }
                        }
                    }
                }
            }
            p
        } else { Vec::new() };
        for path in paths {
            self.import_document_full(&path);
        }
    }

    fn handle_keyboard(&mut self, ctx: &egui::Context) {
        ctx.input(|i| {
            if i.key_pressed(egui::Key::N) && i.modifiers.ctrl {
                self.add_file_path = None;
                self.add_name.clear();
                self.add_description.clear();
                self.add_selected_cats.clear();
                self.add_selected_template = None;
                self.show_add_dialog = true;
            }
            if i.key_pressed(egui::Key::Z) && i.modifiers.ctrl {
                self.undo_last_action();
            }
        });
    }

    fn undo_last_action(&mut self) {
        let l = self.settings.language;
        if let Some(action) = self.undo_stack.pop() {
            match action.action_type.as_str() {
                "delete" => {
                    if let Some(doc) = &action.old_doc {
                        let _ = db::insert_document(&self.db, doc);
                        if let Some(cats) = &action.old_categories {
                            let _ = db::set_document_categories(&self.db, &doc.id, cats);
                        }
                        self.status_msg(l,
                        format!("Deshecho: '{}' restaurado.", doc.name),
                        format!("Undone: '{}' restored.", doc.name),
                    );
                    }
                }
                "trash" => {
                    if let Some(ref id) = action.document_id {
                        let _ = db::restore_document(&self.db, id);
                        self.status_msg(l,
                            "Deshecho: documento restaurado de papelera.".to_string(),
                            "Undone: document restored from trash.".to_string(),
                        );
                    }
                }
                _ => {
                    self.status_msg(l,
                        "No se puede deshacer esta acción.".to_string(),
                        "Cannot undo this action.".to_string(),
                    );
                }
            }
            self.needs_refresh = true;
        } else {
            self.status_msg(l,
                "No hay acciones para deshacer.".to_string(),
                "No actions to undo.".to_string(),
            );
        }
    }
}

impl eframe::App for SgdApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.settings.reduced_motion {
        } else {
            ctx.request_repaint_after(std::time::Duration::from_millis(250));
        }
        ctx.set_visuals(self.settings.theme.to_visuals());

        if let Some(clear_at) = self.status_clear_at {
            if Instant::now() >= clear_at { self.status_message.clear(); self.status_clear_at = None; }
        }

        let l = self.settings.language;
        self.handle_keyboard(ctx);
        self.handle_dropped_files(ctx);
        self.handle_watcher_events();

        if self.needs_refresh {
            self.refresh_data();
            self.needs_refresh = false;
        }

        // Font size
        let font_size = self.settings.font_size;
        let row_h = self.settings.table_density;

        // ---------- TOP BAR ----------
        egui::TopBottomPanel::top("top_bar").show(ctx, |ui| {
            ui.add_space(font_size * 0.5);
            ui.horizontal(|ui| {
                ui.label("\u{1F50D}");
                let search_resp = ui.add_sized([180.0, font_size + 4.0],
                    egui::TextEdit::singleline(&mut self.search_query)
                        .hint_text(tr(l, "Buscar...", "Search..."))
                        .font(egui::TextStyle::Body)
                );
                if search_resp.changed() {
                    self.sidebar_section = SidebarSection::All;
                    self.needs_refresh = true;
                }
                ui.add_space(4.0);
                if ui.button("\u{1F50D}").clicked() {
                    self.sidebar_section = SidebarSection::All;
                    self.needs_refresh = true;
                }
                if ui.button("\u{1F3B0}").clicked() {
                    self.show_filters_dialog = !self.show_filters_dialog;
                }
                if self.filter_state.enabled {
                    ui.label("\u{1F7E2}");
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.add_space(6.0);
                    let add_btn = egui::Button::new(format!("\u{2795} {}", tr(l, "Agregar", "Add")))
                        .fill(egui::Color32::from_rgb(60, 150, 240))
                        .stroke(egui::Stroke::NONE).rounding(egui::Rounding::same(5.0));
                    if ui.add(add_btn).clicked() {
                        self.add_file_path = None; self.add_name.clear();
                        self.add_description.clear(); self.add_selected_cats.clear();
                        self.add_selected_template = None; self.show_add_dialog = true;
                    }
                });
            });
            ui.add_space(font_size * 0.5);
        });

        // ---------- SIDEBAR ----------
        egui::SidePanel::left("categories_panel")
            .resizable(true).default_width(200.0)
            .show(ctx, |ui| {
                egui::Frame::none().inner_margin(egui::Margin::symmetric(8.0, 10.0)).show(ui, |ui| {
                    ui.heading(tr(l, "Explorar", "Browse"));
                    ui.separator();
                    ui.add_space(4.0);

                    let sections = vec![
                        (SidebarSection::All, "\u{1F4C1}", tr(l, "Todos", "All"), "all"),
                        (SidebarSection::Favorites, "\u{2B50}", tr(l, "Favoritos", "Favorites"), "favorites"),
                        (SidebarSection::Recent, "\u{1F552}", tr(l, "Recientes", "Recent"), "none"),
                        (SidebarSection::Trash, "\u{1F5D1}", tr(l, "Papelera", "Trash"), "trash"),
                    ];

                    for (section, icon, label, count_key) in sections {
                        let is_selected = self.sidebar_section == section;
                        let full_w = ui.available_width();
                        let (id, rect) = ui.allocate_space(egui::vec2(full_w, row_h.max(24.0)));
                        let resp = ui.interact(rect, id, egui::Sense::click());
                        if is_selected {
                            ui.painter().rect_filled(rect, egui::Rounding::same(4.0), ui.visuals().selection.bg_fill);
                        }
                        let tc = ui.visuals().text_color();
                        ui.painter().text(egui::pos2(rect.min.x + 8.0, rect.center().y), egui::Align2::LEFT_CENTER,
                            format!("{} {}", icon, label), egui::FontId::proportional(font_size), tc);
                        let count = self.all_counts.get(count_key).copied().unwrap_or(0);
                        if count_key != "none" && count > 0 {
                            let count_str = format!("{}", count);
                            let count_w = count_str.len() as f32 * 8.0 + 6.0;
                            let cx = rect.max.x - 8.0 - count_w / 2.0;
                            let cr = egui::Rect::from_center_size(egui::pos2(cx, rect.center().y), egui::vec2(16.0, 14.0));
                            ui.painter().rect_filled(cr, egui::Rounding::same(7.0), egui::Color32::from_rgb(80, 80, 160));
                            ui.painter().text(cr.center(), egui::Align2::CENTER_CENTER,
                                &count_str, egui::FontId::proportional(10.0), egui::Color32::WHITE);
                        }
                        if resp.clicked() {
                            self.sidebar_section = section;
                            self.search_query.clear();
                            self.needs_refresh = true;
                        }
                    }

                    // Management items
                    ui.add_space(6.0);
                    ui.separator();
                    let mgmt_items = vec![
                        ("\u{1F3F7}", tr(l, "Carpetas", "Categories"), "categories" as &str),
                        ("\u{1F4CB}", tr(l, "Plantillas", "Templates"), "templates"),
                        ("\u{1F4DC}", tr(l, "Historial", "History"), "history"),
                        ("\u{1F4CA}", tr(l, "Estadísticas", "Statistics"), "stats"),
                        ("\u{1F4E5}", tr(l, "Copia Respaldo", "Backup"), "backup"),
                    ];
                    for (icon, label, kind) in mgmt_items {
                        let full_w = ui.available_width();
                        let (rid, rrect) = ui.allocate_space(egui::vec2(full_w, row_h.max(24.0)));
                        let rresp = ui.interact(rrect, rid, egui::Sense::click());
                        let tc = ui.visuals().text_color();
                        ui.painter().text(egui::pos2(rrect.min.x + 8.0, rrect.center().y), egui::Align2::LEFT_CENTER,
                            format!("{} {}", icon, label), egui::FontId::proportional(font_size), tc);
                        if rresp.clicked() {
                            match kind {
                                "categories" => { self.new_cat_name.clear(); self.new_cat_description.clear(); self.new_cat_icon.clear(); self.show_category_dialog = true; }
                                "templates" => { self.new_tpl_name.clear(); self.new_tpl_description.clear(); self.show_template_dialog = true; }
                                "history" => { self.history_selected_day = None; self.history_entries.clear(); let now = chrono::Local::now(); self.history_year = now.year(); self.history_month = now.month(); self.show_history_dialog = true; }
                                "stats" => { self.show_stats_dialog = true; }
                                "backup" => { self.show_backup_dialog = true; }
                                _ => {}
                            }
                        }
                    }

                    // Categories section
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        ui.heading(tr(l, "Carpetas", "Categories"));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let btn_w = 24.0;
                            let (bid, brect) = ui.allocate_space(egui::vec2(btn_w, btn_w));
                            let bresp = ui.interact(brect, bid, egui::Sense::click());
                            ui.painter().text(brect.center(), egui::Align2::CENTER_CENTER, "...",
                                egui::FontId::proportional(16.0), ui.visuals().text_color());
                            if bresp.clicked() { self.show_categories_popup = !self.show_categories_popup; }
                        });
                    });
                    ui.separator();
                    ui.add_space(4.0);

                    egui::ScrollArea::vertical()
                        .id_source("cat_scroll")
                        .max_height(80.0)
                        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::VisibleWhenNeeded)
                        .show(ui, |ui| {
                            for (main_name, _, _) in default_categories() {
                                if let Some(cat) = self.categories.iter().find(|c| c.name == *main_name) {
                                    let count = self.category_counts.get(&cat.id).copied().unwrap_or(0);
                                    let is_selected = matches!(&self.sidebar_section, SidebarSection::Category(id) if id == &cat.id);
                                    let cat_w = ui.available_width();
                                    let (cid, crect) = ui.allocate_space(egui::vec2(cat_w, row_h.max(24.0)));
                                    let cresp = ui.interact(crect, cid, egui::Sense::click());
                                    if is_selected { ui.painter().rect_filled(crect, egui::Rounding::same(4.0), ui.visuals().selection.bg_fill); }
                                    let tc = ui.visuals().text_color();
                                    ui.painter().text(egui::pos2(crect.min.x + 8.0, crect.center().y), egui::Align2::LEFT_CENTER,
                                        format!("{} {}", category_icon(cat.name.as_str()), cat.name), egui::FontId::proportional(font_size), tc);
                                    ui.painter().text(egui::pos2(crect.max.x - 8.0, crect.center().y), egui::Align2::RIGHT_CENTER,
                                        &format!("{}", count), egui::FontId::proportional(font_size - 2.0), egui::Color32::GRAY);
                                    if cresp.clicked() {
                                        self.sidebar_section = SidebarSection::Category(cat.id.clone());
                                        self.search_query.clear();
                                        self.needs_refresh = true;
                                    }
                                }
                            }
                        });

                    // Settings at bottom
                    ui.add_space(ui.available_height().max(0.0) - 40.0);
                    ui.separator();
                    let full_w = ui.available_width();
                    let (sid, srect) = ui.allocate_space(egui::vec2(full_w, row_h.max(24.0)));
                    let sresp = ui.interact(srect, sid, egui::Sense::click());
                    let tc = ui.visuals().text_color();
                    ui.painter().text(egui::pos2(srect.min.x + 8.0, srect.center().y), egui::Align2::LEFT_CENTER,
                        format!("\u{2699} {}", tr(l, "Configuración", "Settings")), egui::FontId::proportional(font_size), tc);
                    if sresp.clicked() { self.show_settings_dialog = true; }
                });
            });

        // ---------- CENTRAL PANEL ----------
        egui::CentralPanel::default().show(ctx, |ui| {
            egui::Frame::none().inner_margin(egui::Margin::same(12.0)).show(ui, |ui| {
                if self.documents.is_empty() {
                    ui.add_space(60.0);
                    ui.vertical_centered(|ui| {
                        let msg = if self.showing_trash {
                            tr(l, "La papelera está vacía", "The trash is empty")
                        } else {
                            tr(l, "No se encontraron documentos", "No documents found")
                        };
                        ui.heading(msg);
                        if !self.showing_trash && self.search_query.is_empty() && matches!(self.sidebar_section, SidebarSection::All) {
                            ui.label(tr(l, "Haz clic en '+ Agregar' o arrastra un archivo.", "Click '+ Add' or drag a file."));
                        }
                    });
                } else {
                    // Header bar with count, sort, actions
                    ui.horizontal(|ui| {
                        let count = self.documents.len();
                        let section_name = match &self.sidebar_section {
                            SidebarSection::All => tr(l, "documento(s)", "document(s)"),
                            SidebarSection::Favorites => tr(l, "favorito(s)", "favorite(s)"),
                            SidebarSection::Recent => tr(l, "reciente(s)", "recent"),
                            SidebarSection::Trash => tr(l, "en papelera", "in trash"),
                            SidebarSection::Category(_) => tr(l, "documento(s)", "document(s)"),
                        };
                        ui.label(format!("{} {}", count, section_name));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let order_icon = match self.sort_order { SortOrder::Ascending => "\u{1F53C}", SortOrder::Descending => "\u{1F53D}" };
                            if ui.button(order_icon).clicked() {
                                self.sort_order = match self.sort_order { SortOrder::Ascending => SortOrder::Descending, SortOrder::Descending => SortOrder::Ascending };
                                self.sort_documents();
                            }
                            egui::ComboBox::from_id_source("sort_combo")
                                .selected_text(match self.sort_field {
                                    SortField::Name => tr(l, "Nombre", "Name"),
                                    SortField::Date => tr(l, "Fecha", "Date"),
                                    SortField::Size => tr(l, "Tamaño", "Size"),
                                    SortField::Type => tr(l, "Tipo", "Type"),
                                })
                                .show_ui(ui, |sub_ui| {
                                    for (field, label) in &[(SortField::Name, tr(l, "Nombre", "Name")), (SortField::Date, tr(l, "Fecha", "Date")), (SortField::Size, tr(l, "Tamaño", "Size")), (SortField::Type, tr(l, "Tipo", "Type"))] {
                                        if sub_ui.selectable_label(self.sort_field == *field, *label).clicked() { self.sort_field = *field; self.sort_documents(); }
                                    }
                                });
                            ui.label(tr(l, "Ordenar:", "Sort:"));
                        });
                    });

                    // Batch action bar
                    if !self.selected_docs.is_empty() {
                        ui.add_space(4.0);
                        ui.horizontal(|ui| {
                            let sel_count = self.selected_docs.len();
                            ui.label(tr_fmt!(l, "{} seleccionado(s)", "{} selected", sel_count));
                            if !self.showing_trash {
                                if ui.button(format!("\u{1F5D1} {}", tr(l, "Mover a Papelera", "Move to Trash"))).clicked() {
                                    let ids: Vec<String> = self.selected_docs.iter().cloned().collect();
                                    for id in &ids {
                                        if let Some(doc) = self.documents.iter().find(|d| d.id == *id) {
                                            let label = format!("{} '{}'", tr(l, "Documento movido a papelera", "Document moved to trash"), doc.name);
                                            self.log_history("trash", &label, Some(id));
                                        }
                                    }
                                    let _ = db::batch_soft_delete(&self.db, &ids);
                                    self.selected_docs.clear();
                                    self.needs_refresh = true;
                                }
                                if ui.button(format!("\u{1F4CB} {}", tr(l, "Cambiar Carpeta", "Change Category"))).clicked() {
                                    let first = self.selected_docs.iter().next().cloned().unwrap_or_default();
                                    let first_clone = first.clone();
                                    self.edit_doc_id = first;
                                    if let Some(doc) = self.documents.iter().find(|d| d.id == first_clone) {
                                        self.edit_name = doc.name.clone();
                                        self.edit_description = doc.description.clone();
                                        self.edit_notes = doc.notes.clone();
                                        self.edit_selected_cats = db::get_document_category_ids(&self.db, &first_clone).unwrap_or_default();
                                    }
                                    self.show_edit_dialog = true;
                                }
                                if ui.button(format!("\u{1F4E5} {}", tr(l, "Exportar", "Export"))).clicked() {
                                    let dest = rfd::FileDialog::new().set_title(tr(l, "Seleccionar carpeta de destino", "Select destination folder")).pick_folder();
                                    if let Some(dir) = dest {
                                        for id in self.selected_docs.clone() {
                                            if let Some(doc) = self.documents.iter().find(|d| d.id == id) {
                                                let src = self.storage.get_full_path(&doc.file_path);
                                                let dest_path = dir.join(&doc.original_name);
                                                let _ = std::fs::copy(&src, &dest_path);
                                            }
                                        }
                                        self.set_status(tr_fmt!(l, "Exportados {} documento(s).", "Exported {} document(s).", sel_count));
                                    }
                                }
                            } else {
                                if ui.button(format!("\u{1F5D1} {}", tr(l, "Eliminar permanentemente", "Delete permanently"))).clicked() {
                                    let ids: Vec<String> = self.selected_docs.iter().cloned().collect();
                                    let _ = db::batch_permanently_delete(&self.db, &self.storage, &ids);
                                    self.selected_docs.clear();
                                    self.needs_refresh = true;
                                }
                            }
                            if ui.button(tr(l, "Deseleccionar", "Deselect")).clicked() {
                                self.selected_docs.clear();
                            }
                        });
                        ui.add_space(4.0);
                    }

                    ui.add_space(8.0);
                    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                        let show_type = self.settings.show_column_type;
                        let show_size = self.settings.show_column_size;
                        let show_date = self.settings.show_column_date;

                        egui::Grid::new("documents_grid").striped(true).min_col_width(50.0).show(ui, |ui| {
                            // Header row
                            ui.strong(" ");
                            ui.strong(tr(l, "Nombre", "Name"));
                            if show_type { ui.strong(tr(l, "Tipo", "Type")); }
                            if show_size { ui.strong(tr(l, "Tamaño", "Size")); }
                            if show_date { ui.strong(tr(l, "Fecha", "Date")); }
                            ui.strong(format!("+ {}", tr(l, "Acciones", "Actions")));
                            ui.end_row();

                            let docs = self.documents.clone();
                            for doc in docs.iter() {
                                let is_selected = self.selected_docs.contains(&doc.id);

                                // Checkbox
                                let cb = ui.checkbox(&mut (is_selected.clone()), " ");
                                if cb.clicked() {
                                    if is_selected { self.selected_docs.remove(&doc.id); }
                                    else { self.selected_docs.insert(doc.id.clone()); }
                                }

                                // Name (with highlight)
                                let display_name = if !self.search_query.is_empty() {
                                    highlight_text(&doc.name, &self.search_query)
                                } else { doc.name.clone() };
                                let name_resp = ui.horizontal(|ui| {
                                    let resp = ui.label(egui::RichText::new(&display_name).size(font_size));
                                    if self.doc_ids_with_reminders.contains(&doc.id) {
                                        ui.colored_label(egui::Color32::RED, "!");
                                    }
                                    resp
                                });
                                let resp = name_resp.inner;
                                if resp.double_clicked() {
                                    let full_path = self.storage.get_full_path(&doc.file_path);
                                    if full_path.exists() { let _ = opener::open(full_path); }
                                    self.recent_docs.retain(|d| d.id != doc.id);
                                    let _ = db::update_document(&self.db, &Document {
                                        updated_at: chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
                                        ..doc.clone()
                                    });
                                    self.needs_refresh = true;
                                }

                                // Type
                                if show_type {
                                    let (type_icon, type_color) = file_type_style(&doc.file_type);
                                    ui.colored_label(type_color, type_icon);
                                }

                                // Size
                                if show_size { ui.label(format_size(doc.size)); }

                                // Date
                                if show_date { ui.label(&doc.created_at[..10]); }

                                // Kebab menu
                                ui.menu_button("+", |ui| {
                                    if self.showing_trash {
                                        if ui.button(format!("\u{1F504} {}", tr(l, "Restaurar", "Restore"))).clicked() {
                                            let _ = db::restore_document(&self.db, &doc.id);
                                            let label = format!("{} '{}'", tr(l, "Documento restaurado", "Document restored"), doc.name);
                                            self.log_history("restore", &label, Some(&doc.id));
                                            self.status_msg(l,
                        format!("'{}' restaurado.", doc.name),
                        format!("'{}' restored.", doc.name),
                    );
                                            self.needs_refresh = true;
                                            ui.close_menu();
                                        }
                                        if ui.button(format!("\u{1F5D1} {}", tr(l, "Eliminar permanentemente", "Delete permanently"))).clicked() {
                                            if let Some(doc_clone) = self.documents.iter().find(|d| d.id == doc.id).cloned() {
                                                let cats = db::get_document_category_ids(&self.db, &doc_clone.id).unwrap_or_default();
                                                self.push_undo(UndoAction {
                                                    action_type: "delete".to_string(),
                                                    description: format!("Delete '{}'", doc_clone.name),
                                                    document_id: Some(doc_clone.id.clone()),
                                                    old_doc: Some(doc_clone),
                                                    old_categories: Some(cats),
                                                });
                                            }
                                            let _ = self.storage.delete_file(&doc.file_path);
                                            let doc_name = doc.name.clone();
                                            let doc_id = doc.id.clone();
                                            let _ = db::permanently_delete_document(&self.db, &doc_id);
                                            let label = format!("{} '{}'", tr(l, "Documento eliminado", "Document deleted"), doc_name);
                                            self.log_history("delete", &label, Some(&doc_id));
                                            self.needs_refresh = true;
                                            ui.close_menu();
                                        }
                                    } else {
                                        let show_star = true;
                                        if show_star {
                                            let star = if doc.favorite { "\u{2B50}" } else { "\u{2606}" };
                                            if ui.button(format!("{} {}", star, tr(l, "Favorito", "Favorite"))).clicked() {
                                                let _ = db::toggle_favorite(&self.db, &doc.id);
                                                self.needs_refresh = true;
                                                ui.close_menu();
                                            }
                                        }
                                        if ui.button(format!("\u{1F441} {}", tr(l, "Abrir", "Open"))).clicked() {
                                            let full_path = self.storage.get_full_path(&doc.file_path);
                                            if full_path.exists() {
                                                if opener::open(&full_path).is_err() {
                                                    self.status_msg(l,
                        format!("No se pudo abrir '{}'", doc.original_name),
                        format!("Could not open '{}'", doc.original_name),
                    );
                                                }
                                            } else {
                                                self.status_msg(l,
                        format!("Archivo no encontrado: {}", doc.original_name),
                        format!("File not found: {}", doc.original_name),
                    );
                                            }
                                            ui.close_menu();
                                        }
                                        if ui.button(format!("\u{1F4C1} {}", tr(l, "Ubicar", "Locate"))).clicked() {
                                            let full_path = self.storage.get_full_path(&doc.file_path);
                                            if let Some(parent) = full_path.parent() {
                                                if opener::open(parent).is_err() {
                                                    self.set_status(tr(l, "No se pudo abrir la carpeta.", "Could not open folder.").to_string());
                                                }
                                            }
                                            ui.close_menu();
                                        }
                                        if ui.button(format!("\u{1F4DD} {}", tr(l, "Editar", "Edit"))).clicked() {
                                            self.edit_doc_id = doc.id.clone(); self.edit_name = doc.name.clone();
                                            self.edit_description = doc.description.clone(); self.edit_notes = doc.notes.clone();
                                            self.edit_selected_cats = db::get_document_category_ids(&self.db, &doc.id).unwrap_or_default();
                                            self.show_edit_dialog = true;
                                            ui.close_menu();
                                        }
                                        if ui.button(format!("\u{1F517} {}", tr(l, "Relaciones", "Relations"))).clicked() {
                                            self.preview_doc_id = doc.id.clone();
                                            self.relations_for_current = db::get_relations_for_document(&self.db, &doc.id).unwrap_or_default();
                                            self.load_related_docs();
                                            self.show_relations_dialog = true;
                                            ui.close_menu();
                                        }
                                        if ui.button(format!("\u{1F4C3} {}", tr(l, "Versiones", "Versions"))).clicked() {
                                            self.preview_doc_id = doc.id.clone();
                                            self.versions_for_current = db::get_versions_for_document(&self.db, &doc.id).unwrap_or_default();
                                            self.show_versions_dialog = true;
                                            ui.close_menu();
                                        }
                                        if ui.button(format!("\u{23F0} {}", tr(l, "Recordatorio", "Reminder"))).clicked() {
                                            self.preview_doc_id = doc.id.clone();
                                            self.reminders_for_current = db::get_reminders_for_document(&self.db, &doc.id).unwrap_or_default();
                                            self.new_reminder_note.clear();
                                            self.new_reminder_date = chrono::Local::now().format("%Y-%m-%d").to_string();
                                            self.show_reminders_dialog = true;
                                            ui.close_menu();
                                        }
                                        if ui.button(format!("\u{1F4E5} {}", tr(l, "Exportar Copia", "Export Copy"))).clicked() {
                                            let src_path = self.storage.get_full_path(&doc.file_path);
                                            if src_path.exists() {
                                                let dest = rfd::FileDialog::new()
                                                    .set_file_name(&doc.original_name)
                                                    .save_file();
                                                if let Some(dest_path) = dest {
                                                    match std::fs::copy(&src_path, &dest_path) {
                                                        Ok(_) => self.set_status(tr(l, "Documento exportado con éxito.", "Document exported successfully.").to_string()),
                                                        Err(e) => self.status_msg(l,
                        format!("Error al exportar: {}", e),
                        format!("Export error: {}", e),
                    ),
                                                    }
                                                }
                                            } else {
                                                self.set_status(tr(l, "Archivo de origen no encontrado.", "Source file not found.").to_string());
                                            }
                                            ui.close_menu();
                                        }
                                        if ui.button(format!("\u{1F5D1} {}", tr(l, "Mover a Papelera", "Move to Trash"))).clicked() {
                                            let do_delete = if self.settings.confirm_delete {
                                                let confirm = rfd::MessageDialog::new()
                                                    .set_title(tr(l, "Confirmar", "Confirm"))
                                                    .set_description(&tr_fmt!(l, "Mover '{}' a la papelera?", "Move '{}' to trash?", doc.name))
                                                    .set_buttons(rfd::MessageButtons::YesNo).show();
                                                confirm == rfd::MessageDialogResult::Yes
                                            } else { true };
                                            if do_delete {
                                                self.push_undo(UndoAction {
                                                    action_type: "trash".to_string(),
                                                    description: format!("Trash '{}'", doc.name),
                                                    document_id: Some(doc.id.clone()),
                                                    old_doc: None,
                                                    old_categories: None,
                                                });
                                                let _ = db::soft_delete_document(&self.db, &doc.id);
                                                let label = format!("{} '{}'", tr(l, "Documento movido a papelera", "Document moved to trash"), doc.name);
                                                self.log_history("trash", &label, Some(&doc.id));
                                                self.status_msg(l,
                        format!("'{}' movido a papelera.", doc.name),
                        format!("'{}' moved to trash.", doc.name),
                    );
                                                self.needs_refresh = true;
                                            }
                                            ui.close_menu();
                                        }
                                    }
                                });
                                ui.end_row();
                            }
                        });
                    });
                }
            });
        });

        // ---------- STATUS BAR ----------
        egui::TopBottomPanel::bottom("status_bar").show(ctx, |ui| {
            if !self.undo_stack.is_empty() {
                ui.horizontal(|ui| {
                    ui.label(&self.status_message);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(tr(l, "Ctrl+Z: Deshacer", "Ctrl+Z: Undo"));
                    });
                });
            } else {
                ui.horizontal(|ui| { ui.add_space(8.0); ui.label(&self.status_message); });
            }
        });

        // ---------- DIALOGS ----------
        self.render_add_dialog(ctx);
        self.render_edit_dialog(ctx);
        self.render_category_dialog(ctx);
        self.render_template_dialog(ctx);
        self.render_history_dialog(ctx);
        self.render_settings_dialog(ctx);
        self.render_theme_selector(ctx);
        self.render_stats_dialog(ctx);
        self.render_categories_popup(ctx);
        self.render_add_cat_popup(ctx);
        self.render_filters_dialog(ctx);
        self.render_relations_dialog(ctx);
        self.render_versions_dialog(ctx);
        self.render_auto_rules_dialog(ctx);
        self.render_reminders_dialog(ctx);
        self.render_versions_dialog(ctx);
        self.render_backup_dialog(ctx);
    }
}

// =================== IMPL METHODS ===================

impl SgdApp {
    fn load_related_docs(&mut self) {
        let ids: Vec<String> = self.relations_for_current.iter().map(|r| {
            if r.source_id == self.preview_doc_id { r.target_id.clone() } else { r.source_id.clone() }
        }).collect();
        self.related_docs = self.documents.iter().filter(|d| ids.contains(&d.id)).cloned().collect();
        self.all_docs_for_relation = self.documents.iter().filter(|d| d.id != self.preview_doc_id).cloned().collect();
    }

    // ---------- ADD DIALOG ----------
    fn render_add_dialog(&mut self, ctx: &egui::Context) {
        let mut open = self.show_add_dialog;
        let l = self.settings.language;
        egui::Window::new(tr(l, "Agregar Documento", "Add Document"))
            .open(&mut open).resizable(false).default_size([480.0, 520.0])
            .show(ctx, |ui| {
                if self.add_file_path.is_none() {
                    let zone = egui::Frame::none()
                        .fill(egui::Color32::from_rgb(242, 242, 252))
                        .stroke(egui::Stroke::new(1.5, egui::Color32::from_rgb(130, 130, 210)));
                    zone.show(ui, |ui| {
                        ui.allocate_ui(egui::vec2(ui.available_size().x, 110.0), |ui| {
                            ui.vertical_centered(|ui| { ui.add_space(28.0); ui.heading("\u{2193}"); ui.label(tr(l, "Arrastra tu archivo aquí", "Drag your file here")); });
                        });
                    });
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if ui.button(tr(l, "Seleccionar Archivo...", "Select File...")).clicked() {
                            let file = rfd::FileDialog::new()
                                .add_filter(tr(l, "Documentos", "Documents"), &["pdf", "xlsx", "xls", "docx", "pptx"])
                                .set_title(tr(l, "Seleccionar un documento", "Select a document")).pick_file();
                            if let Some(path) = file {
                                let stem = get_file_stem(&path);
                                self.add_file_path = Some(path.clone()); self.add_name = stem; self.auto_select_categories_for_path(&path);
                            }
                        }
                        if ui.button(format!("\u{1F4C2} {}", tr(l, "Importar Carpeta...", "Import Folder..."))).clicked() {
                            let folder = rfd::FileDialog::new()
                                .set_title(tr(l, "Seleccionar carpeta", "Select folder")).pick_folder();
                            if let Some(dir) = folder {
                                let l = self.settings.language;
                                let c = self.import_folder_recursive(&dir);
                                self.status_msg(l,
                        format!("Carpeta importada: {} documentos agregados.", c),
                        format!("Folder imported: {} documents added.", c),
                    );
                                self.needs_refresh = true;
                                self.show_add_dialog = false;
                            }
                        }
                    });
                } else if let Some(ref file_path) = self.add_file_path.clone() {
                    let fname = file_path.file_name().unwrap_or_default().to_string_lossy().to_string();
                    ui.horizontal(|ui| {
                        ui.colored_label(egui::Color32::from_rgb(0, 120, 0), tr(l, "Archivo seleccionado:", "Selected file:"));
                        ui.label(&fname);
                        if ui.button(tr(l, "Cambiar", "Change")).clicked() {
                            let file = rfd::FileDialog::new()
                                .add_filter(tr(l, "Documentos", "Documents"), &["pdf", "xlsx", "xls", "docx", "pptx"])
                                .set_title(tr(l, "Seleccionar un documento", "Select a document")).pick_file();
                            if let Some(new_path) = file {
                                let stem = get_file_stem(&new_path);
                                self.add_file_path = Some(new_path.clone()); self.add_name = stem; self.add_description.clear();
                                self.add_selected_template = None; self.auto_select_categories_for_path(&new_path);
                            }
                        }
                    });
                    ui.add_space(8.0);
                    if !self.templates.is_empty() {
                        let tpl_name = self.add_selected_template.as_ref().and_then(|id| self.templates.iter().find(|t| t.id == *id)).map(|t| t.name.as_str()).unwrap_or("");
                        egui::ComboBox::from_label(tr(l, "Plantilla:", "Template:"))
                            .selected_text(tpl_name).show_ui(ui, |ui| {
                                for tpl in &self.templates {
                                    if ui.selectable_label(self.add_selected_template.as_deref() == Some(&tpl.id), &tpl.name).clicked() {
                                        self.add_selected_template = Some(tpl.id.clone()); self.add_description = tpl.description.clone();
                                    }
                                }
                            });
                        ui.add_space(4.0);
                    }
                    ui.label(tr(l, "Nombre del Documento:", "Document Name:"));
                    ui.text_edit_singleline(&mut self.add_name);
                    ui.add_space(5.0);
                    ui.label(tr(l, "Descripción:", "Description:"));
                    ui.text_edit_multiline(&mut self.add_description);
                    ui.add_space(5.0);
                    ui.label(tr(l, "Carpeta:", "Category:"));
                    if self.categories.is_empty() {
                        ui.colored_label(egui::Color32::GRAY, tr(l, "(Aún no hay carpetas.)", "(No categories yet.)"));
                    } else {
                        ui.horizontal_wrapped(|ui| {
                            for name in default_category_names() {
                                if let Some(cat) = self.categories.iter().find(|c| c.name == *name) {
                                    let is_selected = self.add_selected_cats.contains(&cat.id);
                                                    let icon_flag = category_icon(cat.name.as_str());
                                    let bg = ui.visuals().widgets.inactive.bg_fill;
                                    let fg = ui.visuals().selection.bg_fill;
                                    let btn = egui::Button::new(format!("{} {}", icon_flag, cat.name))
                                        .min_size(egui::vec2(80.0, 28.0))
                                        .fill(if is_selected { fg } else { bg });
                                    if ui.add(btn).clicked() {
                                        self.add_selected_cats.clear();
                                        self.add_selected_cats.push(cat.id.clone());
                                    }
                                }
                            }
                            let more_btn = egui::Button::new("...")
                                .min_size(egui::vec2(28.0, 28.0));
                            if ui.add(more_btn).clicked() {
                                self.show_add_cat_popup = !self.show_add_cat_popup;
                            }
                        });
                    }
                    ui.add_space(10.0);
                    ui.horizontal(|ui| {
                        if ui.button(tr(l, "Cancelar", "Cancel")).clicked() { self.show_add_dialog = false; }
                        if ui.add_enabled(!self.add_name.is_empty() && self.add_file_path.is_some(), egui::Button::new(tr(l, "Guardar", "Save"))).clicked() {
                            if let Some(src) = self.add_file_path.clone() {
                                let name = self.add_name.clone(); let desc = self.add_description.clone();
                                let cats = self.add_selected_cats.clone();
                                self.import_document(&src, &name, &desc, &cats);
                                self.show_add_dialog = false;
                            }
                        }
                    });
                }
            });
        self.show_add_dialog = open;
    }

    // ---------- EDIT DIALOG ----------
    fn render_edit_dialog(&mut self, ctx: &egui::Context) {
        let mut open = self.show_edit_dialog;
        let l = self.settings.language;
        egui::Window::new(tr(l, "Editar Documento", "Edit Document"))
            .open(&mut open).resizable(false).default_size([480.0, 420.0])
            .show(ctx, |ui| {
                ui.label(tr(l, "Nombre del Documento:", "Document Name:"));
                ui.text_edit_singleline(&mut self.edit_name);
                ui.add_space(5.0);
                ui.label(tr(l, "Descripción:", "Description:"));
                ui.text_edit_multiline(&mut self.edit_description);
                ui.add_space(5.0);
                ui.label(tr(l, "Notas:", "Notes:"));
                ui.text_edit_multiline(&mut self.edit_notes);
                ui.add_space(5.0);
                ui.label(tr(l, "Carpeta:", "Category:"));
                if self.categories.is_empty() {
                    ui.colored_label(egui::Color32::GRAY, tr(l, "(No hay carpetas disponibles)", "(No categories available)"));
                } else {
                    egui::ScrollArea::vertical().max_height(120.0).show(ui, |ui| {
                        for cat in &self.categories {
                            let is_selected = self.edit_selected_cats.contains(&cat.id);
                            if ui.selectable_label(is_selected, &cat.name).clicked() {
                                self.edit_selected_cats.clear();
                                self.edit_selected_cats.push(cat.id.clone());
                            }
                        }
                    });
                }
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    if ui.button(tr(l, "Cancelar", "Cancel")).clicked() { self.show_edit_dialog = false; }
                    if ui.add_enabled(!self.edit_name.is_empty(), egui::Button::new(tr(l, "Guardar Cambios", "Save Changes"))).clicked() {
                        let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
                        let original = self.documents.iter().find(|d| d.id == self.edit_doc_id).cloned();
                        let mut doc = original.unwrap_or_else(|| Document {
                            id: String::new(), name: String::new(), file_type: String::new(),
                            file_path: String::new(), original_name: String::new(), size: 0,
                            description: String::new(), notes: String::new(), checksum: String::new(),
                            created_at: String::new(), updated_at: String::new(),
                            favorite: false, deleted_at: None, content_text: String::new(),
                        });
                        doc.name = self.edit_name.clone();
                        doc.description = self.edit_description.clone();
                        doc.notes = self.edit_notes.clone();
                        doc.updated_at = now;
                        if db::update_document(&self.db, &doc).is_ok() {
                            let _ = db::set_document_categories(&self.db, &self.edit_doc_id, &self.edit_selected_cats);
                            let label = format!("{} '{}'", tr(l, "Documento editado", "Document edited"), doc.name);
                            self.log_history("edit", &label, Some(&doc.id));
                            self.status_msg(l,
                        format!("Documento '{}' actualizado.", doc.name),
                        format!("Document '{}' updated.", doc.name),
                    );
                        } else {
                            self.set_status(tr(l, "Error al actualizar el documento.", "Error updating document.").to_string());
                        }
                        self.needs_refresh = true;
                        self.show_edit_dialog = false;
                    }
                });
            });
        self.show_edit_dialog = open;
    }

    // ---------- CATEGORY DIALOG ----------
    fn render_category_dialog(&mut self, ctx: &egui::Context) {
        let mut open = self.show_category_dialog;
        let l = self.settings.language;
        egui::Window::new(tr(l, "Gestionar Carpetas", "Manage Categories"))
            .open(&mut open).resizable(false).default_size([450.0, 400.0])
            .show(ctx, |ui| {
                egui::Frame::none().fill(ui.visuals().selection.bg_fill.gamma_multiply(0.3)).rounding(4.0).inner_margin(egui::Margin::same(6.0)).show(ui, |ui| {
                ui.horizontal(|ui| { ui.label("\u{1F3F7}"); ui.heading(tr(l, "Carpetas Existentes", "Existing Categories")); });
                });
                ui.add_space(4.0);
                if self.categories.is_empty() {
                    ui.colored_label(egui::Color32::GRAY, tr(l, "Aún no se han creado carpetas.", "No categories created yet."));
                } else {
                    let cats = self.categories.clone();
                    egui::ScrollArea::vertical().max_height(180.0).show(ui, |ui| {
                        for cat in &cats {
                            ui.horizontal(|ui| {
                                ui.label(if cat.icon.is_empty() { "  ".to_string() } else { format!("{} ", cat.icon) });
                                ui.label(&cat.name);
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    if ui.button(tr(l, "Eliminar", "Delete")).clicked() {
                                        let count = self.category_counts.get(&cat.id).copied().unwrap_or(0);
                                        let do_delete = if count > 0 && self.settings.confirm_delete {
                                            rfd::MessageDialog::new()
                                                .set_title(tr(l, "Confirmar Eliminación", "Confirm Delete"))
                                                .set_description(&tr_fmt!(l, "La carpeta '{}' tiene {} documento(s). ¿Eliminar de todas formas?", "Category '{}' has {} document(s). Delete it anyway?", cat.name, count)).set_buttons(rfd::MessageButtons::YesNo).show()
                                                == rfd::MessageDialogResult::Yes
                                        } else { true };
                                        if do_delete {
                                            let label = format!("{} '{}'", tr(l, "Carpeta eliminada", "Category deleted"), cat.name);
                                            self.log_history("category_del", &label, None);
                                            let _ = db::delete_category(&self.db, &cat.id);
                                            self.needs_refresh = true;
                                        }
                                    }
                                });
                            });
                        }
                    });
                }
                ui.add_space(12.0);
                ui.heading(tr(l, "Agregar Nueva Carpeta", "Add New Category"));
                ui.separator();
                ui.label(tr(l, "Nombre:", "Name:"));
                ui.text_edit_singleline(&mut self.new_cat_name);
                ui.label(tr(l, "Descripción:", "Description:"));
                ui.text_edit_singleline(&mut self.new_cat_description);
                ui.add_space(6.0);
                ui.label(tr(l, "Icono:", "Icon:"));
                ui.horizontal(|ui| {
                    for icon in CATEGORY_ICONS {
                        let selected = self.new_cat_icon == *icon;
                        let btn = egui::Button::new(*icon)
                            .min_size(egui::vec2(32.0, 32.0))
                            .fill(if selected { ui.visuals().selection.bg_fill } else { ui.visuals().widgets.inactive.bg_fill });
                        if ui.add(btn).clicked() { self.new_cat_icon = if selected { String::new() } else { icon.to_string() }; }
                    }
                });
                ui.add_space(6.0);
                if ui.add_enabled(!self.new_cat_name.is_empty(), egui::Button::new(tr(l, "Agregar Carpeta", "Add Category"))).clicked() {
                    let cat = Category { id: uuid::Uuid::new_v4().to_string(), name: self.new_cat_name.trim().to_string(), description: self.new_cat_description.clone(), icon: self.new_cat_icon.clone() };
                    match db::insert_category(&self.db, &cat) {
                        Ok(_) => {
                            let label = format!("{} '{}'", tr(l, "Carpeta creada", "Category created"), cat.name);
                            self.log_history("category_add", &label, None);
                            self.status_msg(l,
                        format!("Carpeta '{}' creada.", cat.name),
                        format!("Category '{}' created.", cat.name),
                    );
                            self.new_cat_name.clear(); self.new_cat_description.clear(); self.new_cat_icon.clear(); self.needs_refresh = true;
                        }
                        Err(e) => self.status_msg(l,
                        format!("Error al crear carpeta: {}", e),
                        format!("Error creating category: {}", e),
                    ),
                    }
                }
            });
        self.show_category_dialog = open;
    }

    // ---------- TEMPLATE DIALOG ----------
    fn render_template_dialog(&mut self, ctx: &egui::Context) {
        let mut open = self.show_template_dialog;
        let l = self.settings.language;
        egui::Window::new(tr(l, "Gestionar Plantillas", "Manage Templates"))
            .open(&mut open).resizable(false).default_size([450.0, 400.0])
            .show(ctx, |ui| {
                ui.heading(tr(l, "Plantillas Existentes", "Existing Templates"));
                ui.separator();
                if self.templates.is_empty() {
                    ui.colored_label(egui::Color32::GRAY, tr(l, "Aún no se han creado plantillas.", "No templates created yet."));
                } else {
                    let tpls = self.templates.clone();
                    egui::ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
                        for tpl in &tpls {
                            ui.group(|ui| {
                                ui.horizontal(|ui| {
                                    ui.strong(&tpl.name);
                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                        if ui.button(tr(l, "Eliminar", "Delete")).clicked() {
                                            let label = format!("{} '{}'", tr(l, "Plantilla eliminada", "Template deleted"), tpl.name);
                                            self.log_history("template_del", &label, None);
                                            let _ = db::delete_template(&self.db, &tpl.id);
                                            self.needs_refresh = true;
                                        }
                                    });
                                });
                                if !tpl.description.is_empty() {
                                    ui.label(egui::RichText::new(&tpl.description).size(11.0).color(egui::Color32::GRAY));
                                }
                            });
                            ui.add_space(4.0);
                        }
                    });
                }
                ui.add_space(12.0);
                ui.heading(tr(l, "Agregar Nueva Plantilla", "Add New Template"));
                ui.separator();
                ui.label(tr(l, "Nombre:", "Name:"));
                ui.text_edit_singleline(&mut self.new_tpl_name);
                ui.label(tr(l, "Descripción (se rellenará automáticamente):", "Description (will auto-fill):"));
                ui.text_edit_multiline(&mut self.new_tpl_description);
                ui.add_space(6.0);
                if ui.add_enabled(!self.new_tpl_name.is_empty(), egui::Button::new(tr(l, "Agregar Plantilla", "Add Template"))).clicked() {
                    let tpl = Template { id: uuid::Uuid::new_v4().to_string(), name: self.new_tpl_name.trim().to_string(), description: self.new_tpl_description.clone() };
                    match db::insert_template(&self.db, &tpl) {
                        Ok(_) => {
                            let label = format!("{} '{}'", tr(l, "Plantilla creada", "Template created"), tpl.name);
                            self.log_history("template_add", &label, None);
                            self.status_msg(l,
                        format!("Plantilla '{}' creada.", tpl.name),
                        format!("Template '{}' created.", tpl.name),
                    );
                            self.new_tpl_name.clear(); self.new_tpl_description.clear(); self.needs_refresh = true;
                        }
                        Err(e) => self.status_msg(l,
                        format!("Error al crear plantilla: {}", e),
                        format!("Error creating template: {}", e),
                    ),
                    }
                }
            });
        self.show_template_dialog = open;
    }

    // ---------- HISTORY DIALOG ----------
    fn render_history_dialog(&mut self, ctx: &egui::Context) {
        let mut open = self.show_history_dialog;
        let l = self.settings.language;
        egui::Window::new(tr(l, "Historial de Acciones", "Action History"))
            .open(&mut open).resizable(false).default_size([460.0, 480.0])
            .show(ctx, |ui| {
                egui::Frame::none().fill(ui.visuals().selection.bg_fill.gamma_multiply(0.3)).rounding(4.0).inner_margin(egui::Margin::same(6.0)).show(ui, |ui| {
                ui.horizontal(|ui| { ui.label("\u{1F4DC}"); ui.heading(tr(l, "Historial de Acciones", "Action History")); });
                });
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if ui.button("<").clicked() {
                        if self.history_month == 1 { self.history_month = 12; self.history_year -= 1; }
                        else { self.history_month -= 1; }
                        self.history_selected_day = None; self.history_entries.clear();
                    }
                    let month_name = month_name(l, self.history_month);
                    ui.label(format!("{} {}", month_name, self.history_year));
                    if ui.button(">").clicked() {
                        if self.history_month == 12 { self.history_month = 1; self.history_year += 1; }
                        else { self.history_month += 1; }
                        self.history_selected_day = None; self.history_entries.clear();
                    }
                });
                ui.add_space(4.0);
                let day_names = match l { Language::Spanish => &DAY_NAMES_ES, Language::English => &DAY_NAMES_EN };
                egui::Grid::new("calendar_grid").striped(false).min_col_width(36.0).max_col_width(36.0).show(ui, |ui| {
                    for d in day_names { ui.label(egui::RichText::new(*d).size(11.0).strong()); }
                    ui.end_row();
                    let total_days = days_in_month(self.history_year, self.history_month);
                    let first = chrono::NaiveDate::from_ymd_opt(self.history_year, self.history_month, 1);
                    let start_weekday = first.map(|d| d.weekday().num_days_from_monday()).unwrap_or(0) as usize;
                    let mut day = 1i32;
                    for _row in 0..6 {
                        if day > total_days as i32 { break; }
                        for col in 0..7 {
                            if _row == 0 && col < start_weekday { ui.label(""); continue; }
                            if day > total_days as i32 { ui.label(""); continue; }
                            let date_str = format!("{:04}-{:02}-{:02}", self.history_year, self.history_month, day);
                            let has_entries = self.history_dates.contains(&date_str);
                            let is_selected = self.history_selected_day == Some(day as u32);
                            let mut label = egui::RichText::new(format!("{}", day));
                            if has_entries { label = label.color(egui::Color32::from_rgb(0, 80, 180)).strong(); }
                            if is_selected { label = label.background_color(egui::Color32::from_rgb(180, 200, 255)); }
                            if ui.selectable_label(is_selected, label).clicked() {
                                self.history_selected_day = Some(day as u32);
                                self.history_entries = db::get_history_by_date(&self.db, &date_str).unwrap_or_default();
                            }
                            day += 1;
                        }
                        ui.end_row();
                    }
                });
                ui.add_space(8.0); ui.separator(); ui.add_space(4.0);
                if self.history_entries.is_empty() {
                    let day = self.history_selected_day.map(|d| format!("{}-{:02}-{:02}", self.history_year, self.history_month, d));
                    if let Some(ref d) = day { ui.colored_label(egui::Color32::GRAY, tr_fmt!(l, "Sin actividad el {}", "No activity on {}", d)); }
                    else { ui.colored_label(egui::Color32::GRAY, tr(l, "Selecciona un día para ver el historial.", "Select a day to view history.")); }
                } else {
                    let day = self.history_selected_day.unwrap_or(0);
                    ui.label(tr_fmt!(l, "Actividad del {}-{:02}-{:02}:", "Activity on {:04}-{:02}-{:02}:", self.history_year, self.history_month, day));
                    ui.add_space(4.0);
                    egui::ScrollArea::vertical().max_height(180.0).show(ui, |ui| {
                        for entry in &self.history_entries {
                            let time = if entry.timestamp.len() >= 16 { &entry.timestamp[11..16] } else { &entry.timestamp };
                            ui.horizontal(|ui| { ui.label(egui::RichText::new(time).size(11.0).monospace()); ui.label(&entry.action_label); });
                        }
                    });
                }
            });
        self.show_history_dialog = open;
    }

    // ---------- SETTINGS DIALOG ----------
    fn render_settings_dialog(&mut self, ctx: &egui::Context) {
        let mut open = self.show_settings_dialog;
        let l = self.settings.language;
        egui::Window::new(tr(l, "Configuración", "Settings"))
            .open(&mut open).resizable(false).default_size([420.0, 500.0])
            .show(ctx, |ui| {
                egui::Frame::none().fill(ui.visuals().selection.bg_fill.gamma_multiply(0.3)).rounding(4.0).inner_margin(egui::Margin::same(6.0)).show(ui, |ui| {
                ui.horizontal(|ui| { ui.label("\u{2699}"); ui.heading(tr(l, "Configuración", "Settings")); });
                });
                ui.add_space(6.0);

                ui.group(|ui| {
                ui.heading(tr(l, "Tema", "Theme"));
                ui.separator();
                let old_theme = self.settings.theme;
                let all_themes = Theme::list();
                let max_main = 3;
                ui.horizontal_wrapped(|ui| {
                    for (theme, es_name, en_name) in all_themes.iter().take(max_main) {
                        let display = match l { Language::Spanish => *es_name, Language::English => *en_name };
                        let colors = theme.preview_colors();
                        let selected = self.settings.theme == *theme;
                        let bg = ui.visuals().widgets.inactive.bg_fill;
                        let frame = egui::Frame::none().fill(bg).rounding(4.0).inner_margin(egui::Margin::same(4.0));
                        frame.show(ui, |ui| {
                            ui.vertical(|ui| {
                                let (rect, response) = ui.allocate_exact_size(egui::vec2(24.0, 24.0), egui::Sense::click());
                                let painter = ui.painter();
                                if colors.len() == 1 { painter.rect_filled(rect, egui::Rounding::same(4.0), colors[0]); }
                                else {
                                    let half = rect.width() / colors.len() as f32;
                                    for (i, c) in colors.iter().enumerate() {
                                        let r = egui::Rect::from_min_size(egui::pos2(rect.min.x + i as f32 * half, rect.min.y), egui::vec2(half, rect.height()));
                                        painter.rect_filled(r, egui::Rounding::same(4.0), *c);
                                    }
                                }
                                if selected { painter.rect_stroke(rect, egui::Rounding::same(4.0), (2.0, ui.visuals().selection.stroke.color)); }
                                ui.add_space(2.0);
                                let _ = ui.selectable_label(selected, display);
                                if response.clicked() { self.settings.theme = *theme; }
                            });
                        });
                    }
                });
                if all_themes.len() > max_main {
                    ui.horizontal(|ui| { ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| { if ui.button(tr(l, "+ otros", "+ others")).clicked() { self.show_theme_selector = true; } }); });
                }
                if old_theme != self.settings.theme { self.save_settings(); ctx.request_repaint(); }
                });

                ui.add_space(6.0);
                ui.group(|ui| {
                ui.heading(tr(l, "Comportamiento", "Behavior"));
                ui.separator();
                let old_confirm = self.settings.confirm_delete;
                ui.checkbox(&mut self.settings.confirm_delete, tr(l, "Confirmar antes de eliminar", "Confirm before deleting"));
                if old_confirm != self.settings.confirm_delete { self.save_settings(); }
                let old_auto = self.settings.auto_open_after_import;
                ui.checkbox(&mut self.settings.auto_open_after_import, tr(l, "Abrir documento al importar", "Open document after import"));
                if old_auto != self.settings.auto_open_after_import { self.save_settings(); }
                });

                ui.add_space(6.0);
                ui.group(|ui| {
                ui.heading(tr(l, "Accesibilidad", "Accessibility"));
                ui.separator();
                let old_font = self.settings.font_size;
                ui.horizontal(|ui| {
                    ui.label(tr(l, "Tamaño de fuente:", "Font size:"));
                    ui.add(egui::Slider::new(&mut self.settings.font_size, 10.0..=24.0));
                });
                if old_font != self.settings.font_size { self.save_settings(); }
                let old_motion = self.settings.reduced_motion;
                ui.checkbox(&mut self.settings.reduced_motion, tr(l, "Reducir animaciones", "Reduce animations"));
                if old_motion != self.settings.reduced_motion { self.save_settings(); }
                });

                ui.add_space(6.0);
                ui.group(|ui| {
                ui.heading(tr(l, "Columnas y Tabla", "Columns & Table"));
                ui.separator();
                let old_col_type = self.settings.show_column_type;
                ui.checkbox(&mut self.settings.show_column_type, tr(l, "Mostrar columna Tipo", "Show Type column"));
                if old_col_type != self.settings.show_column_type { self.save_settings(); }
                let old_col_size = self.settings.show_column_size;
                ui.checkbox(&mut self.settings.show_column_size, tr(l, "Mostrar columna Tamaño", "Show Size column"));
                if old_col_size != self.settings.show_column_size { self.save_settings(); }
                let old_col_date = self.settings.show_column_date;
                ui.checkbox(&mut self.settings.show_column_date, tr(l, "Mostrar columna Fecha", "Show Date column"));
                if old_col_date != self.settings.show_column_date { self.save_settings(); }
                let old_dens = self.settings.table_density;
                ui.horizontal(|ui| {
                    ui.label(tr(l, "Densidad:", "Density:"));
                    ui.add(egui::Slider::new(&mut self.settings.table_density, 18.0..=40.0));
                });
                if old_dens != self.settings.table_density { self.save_settings(); }
                });

                ui.add_space(6.0);
                ui.group(|ui| {
                ui.heading(tr(l, "Papelera", "Trash"));
                ui.separator();
                let old_days = self.settings.trash_auto_delete_days;
                ui.horizontal(|ui| {
                    ui.label(tr(l, "Auto-eliminar después de (días):", "Auto-delete after (days):"));
                    ui.add(egui::Slider::new(&mut self.settings.trash_auto_delete_days, 0..=365));
                });
                if old_days != self.settings.trash_auto_delete_days { self.save_settings(); }
                });

                ui.add_space(6.0);
                ui.group(|ui| {
                ui.heading(tr(l, "Vigilancia de Carpeta", "Folder Watch"));
                ui.separator();
                let old_watch_en = self.settings.watch_folder_enabled;
                ui.checkbox(&mut self.settings.watch_folder_enabled, tr(l, "Vigilar carpeta para importación automática", "Watch folder for auto import"));
                if old_watch_en != self.settings.watch_folder_enabled { self.save_settings(); }
                let old_watch_path = self.settings.watch_folder_path.clone();
                ui.horizontal(|ui| {
                    ui.label(tr(l, "Ruta:", "Path:"));
                    if ui.button(tr(l, "Seleccionar...", "Select...")).clicked() {
                        if let Some(dir) = rfd::FileDialog::new().pick_folder() {
                            self.settings.watch_folder_path = dir.to_string_lossy().to_string();
                        }
                    }
                });
                if old_watch_path != self.settings.watch_folder_path { self.save_settings(); }
                });
            });
        self.show_settings_dialog = open;
    }

    // ---------- THEME SELECTOR ----------
    fn render_theme_selector(&mut self, ctx: &egui::Context) {
        let mut open = self.show_theme_selector;
        let l = self.settings.language;
        egui::Window::new(tr(l, "Todos los Temas", "All Themes"))
            .open(&mut open).resizable(false).default_size([300.0, 300.0])
            .show(ctx, |ui| {
                ui.heading(tr(l, "Selecciona un Tema", "Select a Theme"));
                ui.separator();
                for (theme, es_name, en_name) in Theme::list().iter().skip(3) {
                    let display = match l { Language::Spanish => *es_name, Language::English => *en_name };
                    let colors = theme.preview_colors();
                    let selected = self.settings.theme == *theme;
                    let bg = ui.visuals().widgets.inactive.bg_fill;
                    let frame = egui::Frame::none().fill(bg).rounding(4.0).inner_margin(egui::Margin::same(4.0));
                    frame.show(ui, |ui| {
                        ui.horizontal(|ui| {
                            let (rect, response) = ui.allocate_exact_size(egui::vec2(24.0, 24.0), egui::Sense::click());
                            let painter = ui.painter();
                            if colors.len() == 1 { painter.rect_filled(rect, egui::Rounding::same(4.0), colors[0]); }
                            else {
                                let half = rect.width() / colors.len() as f32;
                                for (i, c) in colors.iter().enumerate() {
                                    let r = egui::Rect::from_min_size(egui::pos2(rect.min.x + i as f32 * half, rect.min.y), egui::vec2(half, rect.height()));
                                    painter.rect_filled(r, egui::Rounding::same(4.0), *c);
                                }
                            }
                            if selected { painter.rect_stroke(rect, egui::Rounding::same(4.0), (2.0, ui.visuals().selection.stroke.color)); }
                            ui.add_space(8.0);
                            let _ = ui.selectable_label(selected, display);
                            if response.clicked() { self.settings.theme = *theme; self.save_settings(); ctx.request_repaint(); }
                        });
                    });
                }
            });
        self.show_theme_selector = open;
    }

    // ---------- STATS DIALOG ----------
    fn render_stats_dialog(&mut self, ctx: &egui::Context) {
        let mut open = self.show_stats_dialog;
        let l = self.settings.language;
        egui::Window::new(tr(l, "Estadísticas", "Statistics"))
            .open(&mut open).resizable(false).default_size([420.0, 380.0])
            .show(ctx, |ui| {
                egui::Frame::none().fill(ui.visuals().selection.bg_fill.gamma_multiply(0.3)).rounding(4.0).inner_margin(egui::Margin::same(6.0)).show(ui, |ui| {
                ui.horizontal(|ui| { ui.label("\u{1F4CA}"); ui.heading(tr(l, "Estadísticas", "Statistics")); });
                });
                ui.add_space(6.0);
                let by_month = db::get_document_count_by_month(&self.db).unwrap_or_default();
                let by_type = db::get_document_count_by_type(&self.db).unwrap_or_default();
                let total_size = db::get_total_document_size(&self.db).unwrap_or(0);
                let total_docs: i64 = self.documents.len() as i64;

                ui.heading(tr(l, "Resumen", "Summary"));
                ui.separator();
                ui.label(tr_fmt!(l, "Total documentos: {}", "Total documents: {}", total_docs));
                ui.label(tr_fmt!(l, "Tamaño total: {}", "Total size: {}", format_size(total_size)));
                let pending = self.pending_reminders.len();
                if pending > 0 {
                    ui.label(tr_fmt!(l, "Recordatorios pendientes: {}", "Pending reminders: {}", pending));
                }
                ui.add_space(12.0);

                ui.heading(tr(l, "Por Tipo", "By Type"));
                ui.separator();
                for (ft, count) in &by_type {
                    let pct = if total_docs > 0 { *count as f64 / total_docs as f64 } else { 0.0 };
                    ui.horizontal(|ui| {
                        let bar_w = (ui.available_width() - 80.0).max(20.0).min(ui.available_width());
                        ui.label(format!("{:.1}  ", ft));
                        let (_id, rect) = ui.allocate_space(egui::vec2(bar_w * pct as f32, 16.0));
                        let bar_color = file_type_style(&ft).1;
                        ui.painter().rect_filled(rect, egui::Rounding::same(3.0), bar_color);
                        ui.label(format!("{}", count));
                    });
                }
                ui.add_space(12.0);

                ui.heading(tr(l, "Por Mes", "By Month"));
                ui.separator();
                let max_count = by_month.iter().map(|(_, c)| *c).max().unwrap_or(1).max(1);
                for (month, count) in &by_month {
                    let pct = *count as f64 / max_count as f64;
                    ui.horizontal(|ui| {
                        let bar_w = (ui.available_width() - 80.0).max(20.0).min(ui.available_width());
                        ui.label(format!("{}  ", month));
                        let sel_bg = ui.visuals().selection.bg_fill;
                        let (_id, rect) = ui.allocate_space(egui::vec2(bar_w * pct as f32, 14.0));
                        ui.painter().rect_filled(rect, egui::Rounding::same(3.0), sel_bg);
                        ui.label(format!("{}", count));
                    });
                }
            });
        self.show_stats_dialog = open;
    }

    // ---------- PREVIEW DIALOG ----------
    // ---------- CATEGORIES POPUP ----------
    fn render_categories_popup(&mut self, ctx: &egui::Context) {
        let mut open = self.show_categories_popup;
        let l = self.settings.language;
        let other_cats: Vec<_> = self.categories.iter().filter(|c| !is_default_category(&c.name)).cloned().collect();
        egui::Window::new(tr(l, "Todas las Carpetas", "All Categories"))
            .open(&mut open).resizable(false).default_size([250.0, 300.0])
            .show(ctx, |ui| {
                if other_cats.is_empty() {
                    ui.colored_label(egui::Color32::GRAY, tr(l, "No hay otras carpetas.", "No other categories."));
                } else {
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        for cat in &other_cats {
                            let count = self.category_counts.get(&cat.id).copied().unwrap_or(0);
                            let is_selected = matches!(&self.sidebar_section, SidebarSection::Category(id) if id == &cat.id);
                            let full_w = ui.available_width();
                            let (cid, crect) = ui.allocate_space(egui::vec2(full_w, 30.0));
                            let cresp = ui.interact(crect, cid, egui::Sense::click());
                            if is_selected { ui.painter().rect_filled(crect, egui::Rounding::same(4.0), ui.visuals().selection.bg_fill); }
                            let tc = ui.visuals().text_color();
                            let cat_icon = if cat.icon.is_empty() { "" } else { &cat.icon };
                            ui.painter().text(egui::pos2(crect.min.x + 8.0, crect.center().y), egui::Align2::LEFT_CENTER,
                                format!("{} {}", cat_icon, cat.name), egui::FontId::proportional(14.0), tc);
                            ui.painter().text(egui::pos2(crect.max.x - 8.0, crect.center().y), egui::Align2::RIGHT_CENTER,
                                &format!("{}", count), egui::FontId::proportional(12.0), egui::Color32::GRAY);
                            if cresp.clicked() {
                                self.sidebar_section = SidebarSection::Category(cat.id.clone());
                                self.search_query.clear();
                                self.needs_refresh = true;
                            }
                        }
                    });
                }
            });
        self.show_categories_popup = open;
    }

    // ---------- ADD CATEGORY POPUP ----------
    fn render_add_cat_popup(&mut self, ctx: &egui::Context) {
        let mut open = self.show_add_cat_popup;
        let l = self.settings.language;
        let other_cats: Vec<_> = self.categories.iter().filter(|c| !is_default_category(&c.name)).cloned().collect();
        let result = egui::Window::new(tr(l, "Otras Carpetas", "Other Categories"))
            .open(&mut open).resizable(false).default_size([220.0, 200.0])
            .show(ctx, |ui| {
                if other_cats.is_empty() {
                    ui.colored_label(egui::Color32::GRAY, tr(l, "No hay otras carpetas.", "No other categories."));
                } else {
                    for cat in &other_cats {
                        let is_selected = self.add_selected_cats.contains(&cat.id);
                        let cat_icon = if cat.icon.is_empty() { "" } else { &cat.icon };
                        if ui.selectable_label(is_selected, format!("{} {}", cat_icon, cat.name)).clicked() {
                            self.add_selected_cats.clear();
                            self.add_selected_cats.push(cat.id.clone());
                        }
                    }
                }
            });
        if result.is_none() { open = false; }
        self.show_add_cat_popup = open;
    }

    // ---------- FILTERS DIALOG ----------
    fn render_filters_dialog(&mut self, ctx: &egui::Context) {
        let mut open = self.show_filters_dialog;
        let l = self.settings.language;
        egui::Window::new(tr(l, "Filtros Avanzados", "Advanced Filters"))
            .open(&mut open).resizable(false).default_size([320.0, 340.0])
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.heading(tr(l, "Filtros", "Filters"));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.checkbox(&mut self.filter_state.enabled, tr(l, "Activar", "Enable"));
                    });
                });
                ui.separator();

                ui.group(|ui| {
                ui.label(tr(l, "Tipo de archivo:", "File type:"));
                ui.checkbox(&mut self.filter_state.pdf, "PDF");
                ui.checkbox(&mut self.filter_state.excel, "Excel");
                ui.checkbox(&mut self.filter_state.docs, "DOCX");
                ui.checkbox(&mut self.filter_state.pptx, "PPTX");
                });

                ui.group(|ui| {
                ui.label(tr(l, "Tamaño (bytes):", "Size (bytes):"));
                ui.horizontal(|ui| {
                    ui.label(tr(l, "Min:", "Min:"));
                    ui.add(egui::TextEdit::singleline(&mut self.filter_state.size_min).desired_width(80.0));
                    ui.label(tr(l, "Max:", "Max:"));
                    ui.add(egui::TextEdit::singleline(&mut self.filter_state.size_max).desired_width(80.0));
                });
                });

                ui.group(|ui| {
                ui.label(tr(l, "Rango de fechas:", "Date range:"));
                ui.horizontal(|ui| {
                    ui.label(tr(l, "Desde:", "From:"));
                    ui.add(egui::TextEdit::singleline(&mut self.filter_state.date_from).hint_text("YYYY-MM-DD").desired_width(100.0));
                });
                ui.horizontal(|ui| {
                    ui.label(tr(l, "Hasta:", "To:"));
                    ui.add(egui::TextEdit::singleline(&mut self.filter_state.date_to).hint_text("YYYY-MM-DD").desired_width(100.0));
                });
                });

                ui.add_space(8.0);
                if ui.button(tr(l, "Aplicar Filtros", "Apply Filters")).clicked() {
                    self.filter_state.enabled = true;
                    self.needs_refresh = true;
                }
                if ui.button(tr(l, "Limpiar Filtros", "Clear Filters")).clicked() {
                    self.filter_state = FilterState::default();
                    self.needs_refresh = true;
                }
            });
        self.show_filters_dialog = open;
    }

    // ---------- RELATIONS DIALOG ----------
    fn render_relations_dialog(&mut self, ctx: &egui::Context) {
        let mut open = self.show_relations_dialog;
        let l = self.settings.language;
        egui::Window::new(tr(l, "Relaciones", "Relations"))
            .open(&mut open).resizable(false).default_size([380.0, 350.0])
            .show(ctx, |ui| {
                ui.heading(tr(l, "Documentos Relacionados", "Related Documents"));
                ui.separator();
                if self.related_docs.is_empty() {
                    ui.colored_label(egui::Color32::GRAY, tr(l, "Sin relaciones.", "No relations."));
                } else {
                    egui::ScrollArea::vertical().max_height(150.0).show(ui, |ui| {
                        let related = self.related_docs.clone();
                        for doc in &related {
                            ui.horizontal(|ui| {
                                ui.label(&doc.name);
                                if ui.button(tr(l, "Eliminar Relación", "Remove Relation")).clicked() {
                                    let rel = self.relations_for_current.iter().find(|r| {
                                        (r.source_id == self.preview_doc_id && r.target_id == doc.id)
                                        || (r.target_id == self.preview_doc_id && r.source_id == doc.id)
                                    }).cloned();
                                    if let Some(r) = rel {
                                        let _ = db::delete_relation(&self.db, &r.id);
                                        self.relations_for_current = db::get_relations_for_document(&self.db, &self.preview_doc_id).unwrap_or_default();
                                        self.load_related_docs();
                                    }
                                }
                            });
                        }
                    });
                }
                ui.add_space(12.0);
                ui.heading(tr(l, "Agregar Relación", "Add Relation"));
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label(tr(l, "Documento:", "Document:"));
                    let other_docs: Vec<_> = self.all_docs_for_relation.clone();
                    let selected_name = other_docs.iter().find(|d| d.id == self.new_relation_doc_id).map(|d| d.name.as_str()).unwrap_or("");
                    egui::ComboBox::from_id_source("rel_doc_combo")
                        .selected_text(selected_name)
                        .show_ui(ui, |ui| {
                            for doc in &other_docs {
                                if ui.selectable_label(self.new_relation_doc_id == doc.id, &doc.name).clicked() {
                                    self.new_relation_doc_id = doc.id.clone();
                                }
                            }
                        });
                });
                ui.horizontal(|ui| {
                    ui.label(tr(l, "Tipo:", "Type:"));
                    ui.radio_value(&mut self.new_relation_type, "related".to_string(), tr(l, "Relacionado", "Related"));
                    ui.radio_value(&mut self.new_relation_type, "duplicate".to_string(), tr(l, "Duplicado", "Duplicate"));
                    ui.radio_value(&mut self.new_relation_type, "supersedes".to_string(), tr(l, "Reemplaza", "Supersedes"));
                });
                if ui.add_enabled(!self.new_relation_doc_id.is_empty(),
                    egui::Button::new(tr(l, "Agregar Relación", "Add Relation"))).clicked()
                {
                    let rel = DocumentRelation {
                        id: uuid::Uuid::new_v4().to_string(),
                        source_id: self.preview_doc_id.clone(),
                        target_id: self.new_relation_doc_id.clone(),
                        relation_type: self.new_relation_type.clone(),
                    };
                    let _ = db::insert_relation(&self.db, &rel);
                    self.new_relation_doc_id.clear();
                    self.relations_for_current = db::get_relations_for_document(&self.db, &self.preview_doc_id).unwrap_or_default();
                    self.load_related_docs();
                }
            });
        self.show_relations_dialog = open;
    }

    // ---------- AUTO RULES DIALOG ----------
    fn render_auto_rules_dialog(&mut self, ctx: &egui::Context) {
        let mut open = self.show_auto_rules_dialog;
        let l = self.settings.language;
        egui::Window::new(tr(l, "Reglas de Auto-Categorización", "Auto-Categorization Rules"))
            .open(&mut open).resizable(false).default_size([380.0, 350.0])
            .show(ctx, |ui| {
                ui.heading(tr(l, "Reglas Existentes", "Existing Rules"));
                ui.separator();
                if self.auto_rules.is_empty() {
                    ui.colored_label(egui::Color32::GRAY, tr(l, "Aún no hay reglas.", "No rules yet."));
                } else {
                    let rules = self.auto_rules.clone();
                    egui::ScrollArea::vertical().max_height(180.0).show(ui, |ui| {
                        for rule in &rules {
                            let cat_name = self.categories.iter().find(|c| c.id == rule.category_id).map(|c| c.name.as_str()).unwrap_or("?");
                            ui.horizontal(|ui| {
                                ui.label(format!("'{}' contiene \"{}\" -> {}", rule.name, rule.pattern, cat_name));
                                if ui.button(tr(l, "Eliminar", "Delete")).clicked() {
                                    let _ = db::delete_auto_rule(&self.db, &rule.id);
                                    self.needs_refresh = true;
                                }
                            });
                        }
                    });
                }
                ui.add_space(12.0);
                ui.heading(tr(l, "Agregar Regla", "Add Rule"));
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label(tr(l, "Nombre:", "Name:"));
                    ui.text_edit_singleline(&mut self.new_rule_name);
                });
                ui.horizontal(|ui| {
                    ui.label(tr(l, "Texto en nombre:", "Text in name:"));
                    ui.text_edit_singleline(&mut self.new_rule_pattern);
                });
                ui.horizontal(|ui| {
                    ui.label(tr(l, "Carpeta:", "Category:"));
                    let selected_name = self.categories.iter().find(|c| c.id == self.new_rule_cat_id).map(|c| c.name.as_str()).unwrap_or("");
                    egui::ComboBox::from_id_source("rule_cat_combo")
                        .selected_text(selected_name)
                        .show_ui(ui, |ui| {
                            for cat in &self.categories {
                                if ui.selectable_label(self.new_rule_cat_id == cat.id, &cat.name).clicked() {
                                    self.new_rule_cat_id = cat.id.clone();
                                }
                            }
                        });
                });
                if ui.add_enabled(!self.new_rule_name.is_empty() && !self.new_rule_pattern.is_empty() && !self.new_rule_cat_id.is_empty(),
                    egui::Button::new(tr(l, "Agregar Regla", "Add Rule"))).clicked()
                {
                    let rule = AutoRule {
                        id: uuid::Uuid::new_v4().to_string(),
                        name: self.new_rule_name.trim().to_string(),
                        pattern: self.new_rule_pattern.trim().to_string(),
                        category_id: self.new_rule_cat_id.clone(),
                    };
                    let _ = db::insert_auto_rule(&self.db, &rule);
                    self.new_rule_name.clear(); self.new_rule_pattern.clear(); self.new_rule_cat_id.clear();
                    self.needs_refresh = true;
                }
            });
        self.show_auto_rules_dialog = open;
    }

    // ---------- REMINDERS DIALOG ----------
    fn render_reminders_dialog(&mut self, ctx: &egui::Context) {
        let mut open = self.show_reminders_dialog;
        let l = self.settings.language;
        egui::Window::new(tr(l, "Recordatorios", "Reminders"))
            .open(&mut open).resizable(false).default_size([350.0, 320.0])
            .show(ctx, |ui| {
                ui.heading(tr(l, "Recordatorios para este documento", "Reminders for this document"));
                ui.separator();
                if self.reminders_for_current.is_empty() {
                    ui.colored_label(egui::Color32::GRAY, tr(l, "Sin recordatorios.", "No reminders."));
                } else {
                    let rems = self.reminders_for_current.clone();
                    egui::ScrollArea::vertical().max_height(150.0).show(ui, |ui| {
                        for rem in &rems {
                            ui.horizontal(|ui| {
                                let mut done = rem.done;
                                if ui.checkbox(&mut done, "").clicked() {
                                    let _ = db::update_reminder_done(&self.db, &rem.id, done);
                                    self.needs_refresh = true;
                                }
                                ui.label(format!("{} - {}", rem.due_date, rem.note));
                                if ui.button(tr(l, "Eliminar", "Delete")).clicked() {
                                    let _ = db::delete_reminder(&self.db, &rem.id);
                                    self.needs_refresh = true;
                                }
                            });
                        }
                    });
                }
                ui.add_space(12.0);
                ui.heading(tr(l, "Agregar Recordatorio", "Add Reminder"));
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label(tr(l, "Fecha:", "Date:"));
                    ui.text_edit_singleline(&mut self.new_reminder_date);
                });
                ui.horizontal(|ui| {
                    ui.label(tr(l, "Nota:", "Note:"));
                    ui.text_edit_singleline(&mut self.new_reminder_note);
                });
                if ui.add_enabled(!self.new_reminder_date.is_empty(),
                    egui::Button::new(tr(l, "Agregar Recordatorio", "Add Reminder"))).clicked()
                {
                    let rem = Reminder {
                        id: uuid::Uuid::new_v4().to_string(),
                        document_id: self.preview_doc_id.clone(),
                        note: self.new_reminder_note.clone(),
                        due_date: self.new_reminder_date.clone(),
                        done: false,
                    };
                    let _ = db::insert_reminder(&self.db, &rem);
                    self.new_reminder_note.clear();
                    self.reminders_for_current = db::get_reminders_for_document(&self.db, &self.preview_doc_id).unwrap_or_default();
                    self.needs_refresh = true;
                }
            });
        self.show_reminders_dialog = open;
    }

    // ---------- VERSIONS DIALOG ----------
    fn render_versions_dialog(&mut self, ctx: &egui::Context) {
        let mut open = self.show_versions_dialog;
        let l = self.settings.language;
        egui::Window::new(tr(l, "Versiones", "Versions"))
            .open(&mut open).resizable(false).default_size([400.0, 300.0])
            .show(ctx, |ui| {
                ui.heading(tr(l, "Versiones Anteriores", "Previous Versions"));
                ui.separator();
                if self.versions_for_current.is_empty() {
                    ui.colored_label(egui::Color32::GRAY, tr(l, "No hay versiones anteriores.", "No previous versions."));
                } else {
                    let vers = self.versions_for_current.clone();
                    let preview_id = self.preview_doc_id.clone();
                    let doc_opt = self.documents.iter().find(|d| d.id == preview_id).cloned();
                    egui::ScrollArea::vertical().max_height(220.0).show(ui, |ui| {
                        for ver in &vers {
                            ui.horizontal(|ui| {
                                ui.label(format!("{} ({})", &ver.created_at[..10], format_size(ver.size)));
                                if ui.button(tr(l, "Restaurar", "Restore")).clicked() {
                                    if let Some(ref doc) = doc_opt {
                                        let src = self.storage.get_full_path(&ver.file_path);
                                        let dest = self.storage.get_full_path(&doc.file_path);
                                        if src.exists() {
                                            let _ = std::fs::copy(&src, &dest);
                                            self.set_status(tr(l, "Versión restaurada.", "Version restored.").to_string());
                                            let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
                                            let new_ver = DocumentVersion {
                                                id: uuid::Uuid::new_v4().to_string(),
                                                document_id: doc.id.clone(),
                                                file_path: doc.file_path.clone(),
                                                size: doc.size,
                                                checksum: doc.checksum.clone(),
                                                created_at: now,
                                            };
                                            let _ = db::insert_document_version(&self.db, &new_ver);
                                            self.versions_for_current = db::get_versions_for_document(&self.db, &preview_id).unwrap_or_default();
                                        }
                                    }
                                }
                            });
                        }
                    });
                }
            });
        self.show_versions_dialog = open;
    }

    // ---------- BACKUP DIALOG ----------
    fn render_backup_dialog(&mut self, ctx: &egui::Context) {
        let mut open = self.show_backup_dialog;
        let l = self.settings.language;
        egui::Window::new(tr(l, "Copia de Respaldo", "Backup"))
            .open(&mut open).resizable(false).default_size([380.0, 280.0])
            .show(ctx, |ui| {
                egui::Frame::none().fill(ui.visuals().selection.bg_fill.gamma_multiply(0.3)).rounding(4.0).inner_margin(egui::Margin::same(6.0)).show(ui, |ui| {
                ui.horizontal(|ui| { ui.label("\u{1F4E5}"); ui.heading(tr(l, "Copia de Respaldo", "Backup")); });
                });
                ui.add_space(6.0);

                ui.group(|ui| {
                ui.heading(tr(l, "Respaldo Manual", "Manual Backup"));
                ui.separator();
                if ui.button(format!("\u{1F4E5} {}", tr(l, "Crear Respaldo", "Create Backup"))).clicked() {
                    let dest = rfd::FileDialog::new()
                        .set_title(tr(l, "Seleccionar destino del respaldo", "Select backup destination"))
                        .pick_folder();
                    if let Some(dir) = dest {
                        match self.storage.backup_all(&dir) {
                            Ok(_) => self.set_status(tr(l, "Respaldo creado con éxito.", "Backup created successfully.").to_string()),
                            Err(e) => self.status_msg(l,
                        format!("Error al crear respaldo: {}", e),
                        format!("Backup error: {}", e),
                    ),
                        }
                    }
                }
                });

                ui.add_space(8.0);
                ui.group(|ui| {
                ui.heading(tr(l, "Respaldo Automático", "Automatic Backup"));
                ui.separator();
                let old_backup_en = self.settings.backup_enabled;
                ui.checkbox(&mut self.settings.backup_enabled, tr(l, "Activar respaldo automático", "Enable automatic backup"));
                if old_backup_en != self.settings.backup_enabled { self.save_settings(); }
                let old_interval = self.settings.backup_interval_hours;
                ui.horizontal(|ui| {
                    ui.label(tr(l, "Intervalo (horas):", "Interval (hours):"));
                    ui.add(egui::Slider::new(&mut self.settings.backup_interval_hours, 1..=168));
                });
                if old_interval != self.settings.backup_interval_hours { self.save_settings(); }
                let old_backup_path = self.settings.backup_path.clone();
                ui.horizontal(|ui| {
                    ui.label(tr(l, "Carpeta destino:", "Destination folder:"));
                    if ui.button(tr(l, "Seleccionar...", "Select...")).clicked() {
                        if let Some(dir) = rfd::FileDialog::new().pick_folder() {
                            self.settings.backup_path = dir.to_string_lossy().to_string();
                        }
                    }
                });
                if old_backup_path != self.settings.backup_path { self.save_settings(); }
                if self.settings.backup_enabled && !self.settings.backup_path.is_empty() {
                    if ui.button(tr(l, "Ejecutar Respaldo Ahora", "Run Backup Now")).clicked() {
                        match self.storage.backup_all(Path::new(&self.settings.backup_path)) {
                            Ok(_) => self.set_status(tr(l, "Respaldo automático ejecutado.", "Automatic backup completed.").to_string()),
                            Err(e) => self.status_msg(l,
                        format!("Error en respaldo: {}", e),
                        format!("Backup error: {}", e),
                    ),
                        }
                    }
                }
                });
            });
        self.show_backup_dialog = open;
    }
}
