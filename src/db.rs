use rusqlite::{Connection, Result, params};
use std::collections::HashMap;

use crate::models::{AutoRule, Category, Document, DocumentRelation, DocumentVersion, HistoryEntry, Reminder, Template};

fn migrate(conn: &Connection) -> Result<()> {
    let cols: Vec<String> = conn
        .prepare("PRAGMA table_info(documents)")?
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<Result<Vec<_>>>()?;

    if !cols.contains(&"favorite".to_string()) {
        conn.execute_batch("ALTER TABLE documents ADD COLUMN favorite INTEGER NOT NULL DEFAULT 0")?;
    }
    if !cols.contains(&"deleted_at".to_string()) {
        conn.execute_batch("ALTER TABLE documents ADD COLUMN deleted_at TEXT")?;
    }
    if !cols.contains(&"content_text".to_string()) {
        conn.execute_batch("ALTER TABLE documents ADD COLUMN content_text TEXT DEFAULT ''")?;
    }
    if !cols.contains(&"notes".to_string()) {
        conn.execute_batch("ALTER TABLE documents ADD COLUMN notes TEXT DEFAULT ''")?;
    }
    if !cols.contains(&"checksum".to_string()) {
        conn.execute_batch("ALTER TABLE documents ADD COLUMN checksum TEXT DEFAULT ''")?;
    }

    let cat_cols: Vec<String> = conn
        .prepare("PRAGMA table_info(categories)")?
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<Result<Vec<_>>>()?;
    if !cat_cols.contains(&"icon".to_string()) {
        conn.execute_batch("ALTER TABLE categories ADD COLUMN icon TEXT DEFAULT ''")?;
    }

    Ok(())
}

pub fn init_db(db_path: &str) -> Result<Connection> {
    let conn = Connection::open(db_path)?;
    conn.execute_batch(
        "
        PRAGMA journal_mode=WAL;
        PRAGMA foreign_keys=ON;

        CREATE TABLE IF NOT EXISTS documents (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            file_type TEXT NOT NULL,
            file_path TEXT NOT NULL,
            original_name TEXT NOT NULL,
            size INTEGER NOT NULL DEFAULT 0,
            description TEXT DEFAULT '',
            notes TEXT DEFAULT '',
            checksum TEXT DEFAULT '',
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            favorite INTEGER NOT NULL DEFAULT 0,
            deleted_at TEXT,
            content_text TEXT DEFAULT ''
        );

        CREATE TABLE IF NOT EXISTS categories (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL UNIQUE,
            description TEXT DEFAULT '',
            icon TEXT DEFAULT ''
        );

        CREATE TABLE IF NOT EXISTS document_categories (
            document_id TEXT NOT NULL,
            category_id TEXT NOT NULL,
            PRIMARY KEY (document_id, category_id),
            FOREIGN KEY (document_id) REFERENCES documents(id) ON DELETE CASCADE,
            FOREIGN KEY (category_id) REFERENCES categories(id) ON DELETE CASCADE
        );

        CREATE TABLE IF NOT EXISTS templates (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL UNIQUE,
            description TEXT DEFAULT ''
        );

        CREATE TABLE IF NOT EXISTS history (
            id TEXT PRIMARY KEY,
            action_type TEXT NOT NULL,
            action_label TEXT NOT NULL,
            document_id TEXT,
            timestamp TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS document_relations (
            id TEXT PRIMARY KEY,
            source_id TEXT NOT NULL,
            target_id TEXT NOT NULL,
            relation_type TEXT NOT NULL DEFAULT 'related',
            FOREIGN KEY (source_id) REFERENCES documents(id) ON DELETE CASCADE,
            FOREIGN KEY (target_id) REFERENCES documents(id) ON DELETE CASCADE
        );

        CREATE TABLE IF NOT EXISTS auto_rules (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            pattern TEXT NOT NULL,
            category_id TEXT NOT NULL,
            FOREIGN KEY (category_id) REFERENCES categories(id) ON DELETE CASCADE
        );

        CREATE TABLE IF NOT EXISTS reminders (
            id TEXT PRIMARY KEY,
            document_id TEXT NOT NULL,
            note TEXT DEFAULT '',
            due_date TEXT NOT NULL,
            done INTEGER NOT NULL DEFAULT 0,
            FOREIGN KEY (document_id) REFERENCES documents(id) ON DELETE CASCADE
        );

        CREATE TABLE IF NOT EXISTS document_versions (
            id TEXT PRIMARY KEY,
            document_id TEXT NOT NULL,
            file_path TEXT NOT NULL,
            size INTEGER NOT NULL DEFAULT 0,
            checksum TEXT DEFAULT '',
            created_at TEXT NOT NULL,
            FOREIGN KEY (document_id) REFERENCES documents(id) ON DELETE CASCADE
        );

        CREATE INDEX IF NOT EXISTS idx_docs_deleted ON documents(deleted_at);
        CREATE INDEX IF NOT EXISTS idx_docs_deleted_created ON documents(deleted_at, created_at DESC);
        CREATE INDEX IF NOT EXISTS idx_docs_favorite ON documents(favorite) WHERE deleted_at IS NULL;
        CREATE INDEX IF NOT EXISTS idx_docs_updated ON documents(updated_at) WHERE deleted_at IS NULL;
        CREATE INDEX IF NOT EXISTS idx_doc_cats_cat ON document_categories(category_id);
        CREATE INDEX IF NOT EXISTS idx_doc_cats_doc ON document_categories(document_id);
        CREATE INDEX IF NOT EXISTS idx_history_ts ON history(timestamp);
        CREATE INDEX IF NOT EXISTS idx_reminders_doc ON reminders(document_id);
        CREATE INDEX IF NOT EXISTS idx_reminders_done ON reminders(done);
        CREATE INDEX IF NOT EXISTS idx_versions_doc ON document_versions(document_id);
        CREATE INDEX IF NOT EXISTS idx_relations_src ON document_relations(source_id);
        CREATE INDEX IF NOT EXISTS idx_relations_tgt ON document_relations(target_id);
    ",
    )?;
    migrate(&conn)?;
    Ok(conn)
}

pub fn ensure_default_categories(conn: &Connection) -> Result<()> {
    let defaults: [(&str, &str, &str); 4] = [
        ("PDF", "Documentos PDF", "\u{1F4C4}"),
        ("Excel", "Hojas de calculo Excel", "\u{1F4CA}"),
        ("Docs", "Documentos de Word", "\u{1F4DD}"),
        ("Presentaciones", "Presentaciones PowerPoint", "\u{1F4F9}"),
    ];
    for (name, desc, icon) in defaults {
        let exists: bool = conn.query_row(
            "SELECT COUNT(*) FROM categories WHERE name=?1", params![name],
            |row| row.get::<_, i64>(0),
        ).map(|c| c > 0).unwrap_or(false);
        if !exists {
            let id = uuid::Uuid::new_v4().to_string();
            conn.execute(
                "INSERT INTO categories (id, name, description, icon) VALUES (?1, ?2, ?3, ?4)",
                params![id, name, desc, icon],
            )?;
        }
    }
    Ok(())
}

fn row_to_document(row: &rusqlite::Row) -> rusqlite::Result<Document> {
    Ok(Document {
        id: row.get(0)?,
        name: row.get(1)?,
        file_type: row.get(2)?,
        file_path: row.get(3)?,
        original_name: row.get(4)?,
        size: row.get(5)?,
        description: row.get(6)?,
        notes: row.get(7)?,
        checksum: row.get(8)?,
        created_at: row.get(9)?,
        updated_at: row.get(10)?,
        favorite: row.get::<_, i64>(11)? != 0,
        deleted_at: row.get(12)?,
        content_text: row.get(13)?,
    })
}

const DOC_COLS: &str = "id, name, file_type, file_path, original_name, size, description, notes, checksum, created_at, updated_at, favorite, deleted_at, content_text";

// Document CRUD

pub fn insert_document(conn: &Connection, doc: &Document) -> Result<()> {
    conn.execute(
        &format!("INSERT INTO documents ({}) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14)", DOC_COLS),
        params![
            doc.id, doc.name, doc.file_type, doc.file_path, doc.original_name,
            doc.size, doc.description, doc.notes, doc.checksum,
            doc.created_at, doc.updated_at,
            doc.favorite as i64, doc.deleted_at, doc.content_text,
        ],
    )?;
    Ok(())
}

pub fn update_document(conn: &Connection, doc: &Document) -> Result<()> {
    conn.execute(
        "UPDATE documents SET name=?1, description=?2, notes=?3, updated_at=?4, favorite=?5, content_text=?6 WHERE id=?7",
        params![doc.name, doc.description, doc.notes, doc.updated_at, doc.favorite as i64, doc.content_text, doc.id],
    )?;
    Ok(())
}

pub fn toggle_favorite(conn: &Connection, id: &str) -> Result<bool> {
    let fav: bool = conn.query_row(
        "SELECT favorite FROM documents WHERE id=?1", params![id], |row| row.get::<_, i64>(0),
    ).map(|v| v != 0)?;
    let new = !fav;
    conn.execute("UPDATE documents SET favorite=?1 WHERE id=?2", params![new as i64, id])?;
    Ok(new)
}

pub fn soft_delete_document(conn: &Connection, id: &str) -> Result<()> {
    let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    conn.execute("UPDATE documents SET deleted_at=?1 WHERE id=?2", params![now, id])?;
    Ok(())
}

pub fn batch_soft_delete(conn: &Connection, ids: &[String]) -> Result<()> {
    if ids.is_empty() { return Ok(()); }
    let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let placeholders: Vec<String> = ids.iter().enumerate().map(|(i, _)| format!("?{}", i + 2)).collect();
    let sql = format!("UPDATE documents SET deleted_at=?1 WHERE id IN ({})", placeholders.join(","));
    let mut stmt = conn.prepare(&sql)?;
    let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = vec![Box::new(now)];
    for id in ids { params.push(Box::new(id.clone())); }
    let param_refs: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|p| p.as_ref()).collect();
    stmt.execute(param_refs.as_slice())?;
    Ok(())
}

pub fn batch_permanently_delete(conn: &Connection, storage: &crate::storage::Storage, ids: &[String]) -> Result<()> {
    if ids.is_empty() { return Ok(()); }
    for id in ids {
        if let Ok(doc) = conn.query_row(
            &format!("SELECT {} FROM documents WHERE id=?1", DOC_COLS), params![id],
            row_to_document,
        ) {
            let _ = storage.delete_file(&doc.file_path);
        }
    }
    let placeholders: Vec<String> = ids.iter().enumerate().map(|(i, _)| format!("?{}", i + 1)).collect();
    let sql = format!("DELETE FROM documents WHERE id IN ({})", placeholders.join(","));
    conn.execute(&sql, rusqlite::params_from_iter(ids))?;
    Ok(())
}

pub fn restore_document(conn: &Connection, id: &str) -> Result<()> {
    conn.execute("UPDATE documents SET deleted_at=NULL WHERE id=?1", params![id])?;
    Ok(())
}

pub fn permanently_delete_document(conn: &Connection, id: &str) -> Result<()> {
    // CASCADE handles document_versions, reminders, document_relations, document_categories
    conn.execute("DELETE FROM documents WHERE id=?1", params![id])?;
    Ok(())
}

pub fn delete_trashed_older_than(conn: &Connection, days: i64) -> Result<Vec<Document>> {
    let threshold = (chrono::Local::now() - chrono::Duration::days(days))
        .format("%Y-%m-%d %H:%M:%S").to_string();
    let mut stmt = conn.prepare(
        &format!("SELECT {} FROM documents WHERE deleted_at IS NOT NULL AND deleted_at < ?1", DOC_COLS),
    )?;
    let docs: Vec<Document> = stmt.query_map(params![threshold], row_to_document)?
        .collect::<Result<Vec<_>>>()?;
    // CASCADE handles document_versions, reminders, document_relations, document_categories
    conn.execute("DELETE FROM documents WHERE deleted_at IS NOT NULL AND deleted_at < ?1", params![threshold])?;
    Ok(docs)
}

// Querying

pub fn get_all_documents(conn: &Connection) -> Result<Vec<Document>> {
    let mut stmt = conn.prepare(
        &format!("SELECT {} FROM documents WHERE deleted_at IS NULL ORDER BY created_at DESC", DOC_COLS),
    )?;
    let docs = stmt.query_map([], row_to_document)?.collect::<Result<Vec<_>>>()?;
    Ok(docs)
}

pub fn get_trashed_documents(conn: &Connection) -> Result<Vec<Document>> {
    let mut stmt = conn.prepare(
        &format!("SELECT {} FROM documents WHERE deleted_at IS NOT NULL ORDER BY deleted_at DESC", DOC_COLS),
    )?;
    let docs = stmt.query_map([], row_to_document)?.collect::<Result<Vec<_>>>()?;
    Ok(docs)
}

pub fn get_favorite_documents(conn: &Connection) -> Result<Vec<Document>> {
    let mut stmt = conn.prepare(
        &format!("SELECT {} FROM documents WHERE favorite=1 AND deleted_at IS NULL ORDER BY created_at DESC", DOC_COLS),
    )?;
    let docs = stmt.query_map([], row_to_document)?.collect::<Result<Vec<_>>>()?;
    Ok(docs)
}

pub fn search_documents(conn: &Connection, query: &str) -> Result<Vec<Document>> {
    let pattern = format!("%{}%", query);
    let mut stmt = conn.prepare(
        &format!("SELECT {} FROM documents WHERE deleted_at IS NULL AND (name LIKE ?1 OR description LIKE ?1 OR original_name LIKE ?1 OR content_text LIKE ?1 OR notes LIKE ?1) ORDER BY created_at DESC", DOC_COLS),
    )?;
    let docs = stmt.query_map(params![pattern], row_to_document)?.collect::<Result<Vec<_>>>()?;
    Ok(docs)
}

pub fn search_documents_by_category(conn: &Connection, query: &str, category_id: &str) -> Result<Vec<Document>> {
    let pattern = format!("%{}%", query);
    let mut stmt = conn.prepare(
        &format!("SELECT d.{} FROM documents d JOIN document_categories dc ON d.id = dc.document_id WHERE dc.category_id = ?1 AND d.deleted_at IS NULL AND (d.name LIKE ?2 OR d.description LIKE ?2 OR d.original_name LIKE ?2 OR d.content_text LIKE ?2 OR d.notes LIKE ?2) ORDER BY d.created_at DESC", DOC_COLS),
    )?;
    let docs = stmt.query_map(params![category_id, pattern], row_to_document)?.collect::<Result<Vec<_>>>()?;
    Ok(docs)
}

pub fn get_documents_by_category(conn: &Connection, category_id: &str) -> Result<Vec<Document>> {
    let mut stmt = conn.prepare(
        &format!("SELECT d.{} FROM documents d JOIN document_categories dc ON d.id = dc.document_id WHERE dc.category_id = ?1 AND d.deleted_at IS NULL ORDER BY d.created_at DESC", DOC_COLS),
    )?;
    let docs = stmt.query_map(params![category_id], row_to_document)?.collect::<Result<Vec<_>>>()?;
    Ok(docs)
}

pub fn get_recent_documents(conn: &Connection, limit: i64) -> Result<Vec<Document>> {
    let mut stmt = conn.prepare(
        &format!("SELECT {} FROM documents WHERE deleted_at IS NULL ORDER BY updated_at DESC LIMIT ?1", DOC_COLS),
    )?;
    let docs = stmt.query_map(params![limit], row_to_document)?.collect::<Result<Vec<_>>>()?;
    Ok(docs)
}

// Category CRUD

pub fn insert_category(conn: &Connection, cat: &Category) -> Result<()> {
    conn.execute(
        "INSERT INTO categories (id, name, description, icon) VALUES (?1, ?2, ?3, ?4)",
        params![cat.id, cat.name, cat.description, cat.icon],
    )?;
    Ok(())
}

pub fn delete_category(conn: &Connection, id: &str) -> Result<()> {
    conn.execute("DELETE FROM document_categories WHERE category_id=?1", params![id])?;
    conn.execute("DELETE FROM categories WHERE id=?1", params![id])?;
    Ok(())
}

pub fn get_all_categories(conn: &Connection) -> Result<Vec<Category>> {
    let mut stmt = conn.prepare("SELECT id, name, description, icon FROM categories ORDER BY name")?;
    let cats = stmt.query_map([], |row| {
        Ok(Category { id: row.get(0)?, name: row.get(1)?, description: row.get(2)?, icon: row.get(3)? })
    })?.collect::<Result<Vec<_>>>()?;
    Ok(cats)
}

// Junction table

pub fn set_document_categories(conn: &Connection, doc_id: &str, category_ids: &[String]) -> Result<()> {
    let tx = conn.unchecked_transaction()?;
    tx.execute("DELETE FROM document_categories WHERE document_id=?1", params![doc_id])?;
    for cat_id in category_ids {
        tx.execute("INSERT INTO document_categories (document_id, category_id) VALUES (?1, ?2)", params![doc_id, cat_id])?;
    }
    tx.commit()?;
    Ok(())
}

pub fn get_document_category_ids(conn: &Connection, doc_id: &str) -> Result<Vec<String>> {
    let mut stmt = conn.prepare("SELECT category_id FROM document_categories WHERE document_id=?1")?;
    let ids = stmt.query_map(params![doc_id], |row| row.get(0))?.collect::<Result<Vec<String>>>()?;
    Ok(ids)
}

pub fn get_document_counts_by_category(conn: &Connection) -> Result<HashMap<String, i64>> {
    let mut stmt = conn.prepare(
        "SELECT c.id, COUNT(dc.document_id) as count FROM categories c LEFT JOIN document_categories dc ON c.id = dc.category_id GROUP BY c.id",
    )?;
    let counts = stmt.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
    })?.collect::<Result<HashMap<String, i64>>>()?;
    Ok(counts)
}

// Template CRUD

pub fn insert_template(conn: &Connection, tpl: &Template) -> Result<()> {
    conn.execute("INSERT INTO templates (id, name, description) VALUES (?1, ?2, ?3)", params![tpl.id, tpl.name, tpl.description])?;
    Ok(())
}

pub fn delete_template(conn: &Connection, id: &str) -> Result<()> {
    conn.execute("DELETE FROM templates WHERE id=?1", params![id])?;
    Ok(())
}

pub fn get_all_templates(conn: &Connection) -> Result<Vec<Template>> {
    let mut stmt = conn.prepare("SELECT id, name, description FROM templates ORDER BY name")?;
    let tpls = stmt.query_map([], |row| {
        Ok(Template { id: row.get(0)?, name: row.get(1)?, description: row.get(2)? })
    })?.collect::<Result<Vec<_>>>()?;
    Ok(tpls)
}

// History CRUD

pub fn insert_history(conn: &Connection, id: &str, action_type: &str, action_label: &str, document_id: Option<&str>, timestamp: &str) -> Result<()> {
    conn.execute("INSERT INTO history (id, action_type, action_label, document_id, timestamp) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![id, action_type, action_label, document_id, timestamp])?;
    Ok(())
}

pub fn get_history_by_date(conn: &Connection, date: &str) -> Result<Vec<HistoryEntry>> {
    let pattern = format!("{}%", date);
    let mut stmt = conn.prepare("SELECT id, action_type, action_label, document_id, timestamp FROM history WHERE timestamp LIKE ?1 ORDER BY timestamp DESC")?;
    let entries = stmt.query_map(params![pattern], |row| {
        Ok(HistoryEntry { id: row.get(0)?, action_type: row.get(1)?, action_label: row.get(2)?, document_id: row.get(3)?, timestamp: row.get(4)? })
    })?.collect::<Result<Vec<_>>>()?;
    Ok(entries)
}

pub fn get_history_dates(conn: &Connection) -> Result<Vec<String>> {
    let mut stmt = conn.prepare("SELECT DISTINCT substr(timestamp, 1, 10) as day FROM history ORDER BY day DESC")?;
    let dates = stmt.query_map([], |row| row.get::<_, String>(0))?.collect::<Result<Vec<_>>>()?;
    Ok(dates)
}

// Document Relations

pub fn insert_relation(conn: &Connection, rel: &DocumentRelation) -> Result<()> {
    conn.execute("INSERT INTO document_relations (id, source_id, target_id, relation_type) VALUES (?1, ?2, ?3, ?4)",
        params![rel.id, rel.source_id, rel.target_id, rel.relation_type])?;
    Ok(())
}

pub fn delete_relation(conn: &Connection, id: &str) -> Result<()> {
    conn.execute("DELETE FROM document_relations WHERE id=?1", params![id])?;
    Ok(())
}

pub fn get_relations_for_document(conn: &Connection, doc_id: &str) -> Result<Vec<DocumentRelation>> {
    let mut stmt = conn.prepare(
        "SELECT id, source_id, target_id, relation_type FROM document_relations WHERE source_id=?1 OR target_id=?1"
    )?;
    let list = stmt.query_map(params![doc_id], |row| {
        Ok(DocumentRelation { id: row.get(0)?, source_id: row.get(1)?, target_id: row.get(2)?, relation_type: row.get(3)? })
    })?.collect::<Result<Vec<_>>>()?;
    Ok(list)
}

// Auto Rules

pub fn insert_auto_rule(conn: &Connection, rule: &AutoRule) -> Result<()> {
    conn.execute("INSERT INTO auto_rules (id, name, pattern, category_id) VALUES (?1, ?2, ?3, ?4)",
        params![rule.id, rule.name, rule.pattern, rule.category_id])?;
    Ok(())
}

pub fn delete_auto_rule(conn: &Connection, id: &str) -> Result<()> {
    conn.execute("DELETE FROM auto_rules WHERE id=?1", params![id])?;
    Ok(())
}

pub fn get_all_auto_rules(conn: &Connection) -> Result<Vec<AutoRule>> {
    let mut stmt = conn.prepare("SELECT id, name, pattern, category_id FROM auto_rules ORDER BY name")?;
    let list = stmt.query_map([], |row| {
        Ok(AutoRule { id: row.get(0)?, name: row.get(1)?, pattern: row.get(2)?, category_id: row.get(3)? })
    })?.collect::<Result<Vec<_>>>()?;
    Ok(list)
}

pub fn match_auto_rules(conn: &Connection, file_name: &str) -> Result<Vec<String>> {
    let rules = get_all_auto_rules(conn)?;
    let lower = file_name.to_lowercase();
    let mut matched = Vec::new();
    for rule in &rules {
        if lower.contains(&rule.pattern.to_lowercase()) {
            matched.push(rule.category_id.clone());
        }
    }
    Ok(matched)
}

// Reminders

pub fn insert_reminder(conn: &Connection, rem: &Reminder) -> Result<()> {
    conn.execute("INSERT INTO reminders (id, document_id, note, due_date, done) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![rem.id, rem.document_id, rem.note, rem.due_date, rem.done as i64])?;
    Ok(())
}

pub fn update_reminder_done(conn: &Connection, id: &str, done: bool) -> Result<()> {
    conn.execute("UPDATE reminders SET done=?1 WHERE id=?2", params![done as i64, id])?;
    Ok(())
}

pub fn delete_reminder(conn: &Connection, id: &str) -> Result<()> {
    conn.execute("DELETE FROM reminders WHERE id=?1", params![id])?;
    Ok(())
}

pub fn get_reminders_for_document(conn: &Connection, doc_id: &str) -> Result<Vec<Reminder>> {
    let mut stmt = conn.prepare(
        "SELECT id, document_id, note, due_date, done FROM reminders WHERE document_id=?1 ORDER BY due_date"
    )?;
    let list = stmt.query_map(params![doc_id], |row| {
        Ok(Reminder {
            id: row.get(0)?, document_id: row.get(1)?, note: row.get(2)?,
            due_date: row.get(3)?, done: row.get::<_, i64>(4)? != 0,
        })
    })?.collect::<Result<Vec<_>>>()?;
    Ok(list)
}

pub fn get_all_pending_reminders(conn: &Connection) -> Result<Vec<Reminder>> {
    let mut stmt = conn.prepare(
        "SELECT id, document_id, note, due_date, done FROM reminders WHERE done=0 ORDER BY due_date"
    )?;
    let list = stmt.query_map([], |row| {
        Ok(Reminder {
            id: row.get(0)?, document_id: row.get(1)?, note: row.get(2)?,
            due_date: row.get(3)?, done: row.get::<_, i64>(4)? != 0,
        })
    })?.collect::<Result<Vec<_>>>()?;
    Ok(list)
}

// Document Versions

pub fn insert_document_version(conn: &Connection, ver: &DocumentVersion) -> Result<()> {
    conn.execute("INSERT INTO document_versions (id, document_id, file_path, size, checksum, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![ver.id, ver.document_id, ver.file_path, ver.size, ver.checksum, ver.created_at])?;
    Ok(())
}

pub fn get_versions_for_document(conn: &Connection, doc_id: &str) -> Result<Vec<DocumentVersion>> {
    let mut stmt = conn.prepare(
        "SELECT id, document_id, file_path, size, checksum, created_at FROM document_versions WHERE document_id=?1 ORDER BY created_at DESC"
    )?;
    let list = stmt.query_map(params![doc_id], |row| {
        Ok(DocumentVersion {
            id: row.get(0)?, document_id: row.get(1)?, file_path: row.get(2)?,
            size: row.get(3)?, checksum: row.get(4)?, created_at: row.get(5)?,
        })
    })?.collect::<Result<Vec<_>>>()?;
    Ok(list)
}

// Stats

pub fn get_document_count_by_month(conn: &Connection) -> Result<Vec<(String, i64)>> {
    let mut stmt = conn.prepare(
        "SELECT substr(created_at,1,7) as month, COUNT(*) as count FROM documents WHERE deleted_at IS NULL GROUP BY month ORDER BY month",
    )?;
    let data = stmt.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
    })?.collect::<Result<Vec<_>>>()?;
    Ok(data)
}

pub fn get_document_count_by_type(conn: &Connection) -> Result<Vec<(String, i64)>> {
    let mut stmt = conn.prepare(
        "SELECT file_type, COUNT(*) as count FROM documents WHERE deleted_at IS NULL GROUP BY file_type ORDER BY count DESC",
    )?;
    let data = stmt.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
    })?.collect::<Result<Vec<_>>>()?;
    Ok(data)
}

pub fn get_total_document_size(conn: &Connection) -> Result<i64> {
    conn.query_row("SELECT COALESCE(SUM(size),0) FROM documents WHERE deleted_at IS NULL", [], |row| row.get(0))
}

pub fn get_all_document_counts(conn: &Connection) -> Result<HashMap<String, i64>> {
    let mut stmt = conn.prepare(
        "SELECT 'all', COUNT(*) FROM documents WHERE deleted_at IS NULL
         UNION ALL
         SELECT 'favorites', COUNT(*) FROM documents WHERE favorite=1 AND deleted_at IS NULL
         UNION ALL
         SELECT 'trash', COUNT(*) FROM documents WHERE deleted_at IS NOT NULL"
    )?;
    let map = stmt.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
    })?.collect::<Result<HashMap<String, i64>>>()?;
    Ok(map)
}
