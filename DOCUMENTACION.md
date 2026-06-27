# SGD - Sistema de Gestión Documental

## Descripción General

SGD es un sistema de escritorio para la administración, control y almacenamiento de
documentos digitales (PDF, Excel, DOCX, PPTX). Desarrollado en Rust con la interfaz gráfica egui,
almacenamiento local SQLite y sistema de archivos con subdirectorios por carpeta.

---

## Arquitectura

```
sgd-local/
├── Cargo.toml
├── DOCUMENTACION.md
├── src/
│   ├── main.rs       # Punto de entrada, configura ventana nativa
│   ├── app.rs        # UI completa (eframe + egui), estado global, diálogos
│   ├── models.rs     # Structs de datos (Document, Category, Template, etc.)
│   ├── db.rs         # Capa de base de datos SQLite (CRUD + migraciones)
│   └── storage.rs    # Manejo físico de archivos (importar, eliminar, subdirectorios)
└── sgd_data/          # Generado en tiempo de ejecución
    ├── documents.db   # Base de datos SQLite
    ├── settings.json  # Configuración persistente (tema, idioma, accesibilidad)
    └── files/         # Archivos importados en subdirectorios por carpeta
        ├── PDF/
        ├── Excel/
        ├── Docs/
        ├── Presentaciones/
        └── ...
```

### Flujo de datos

```
Usuario (UI egui)
    │
    ▼
app.rs ──► db.rs ──► documents.db (SQLite)
    │              └── history, categories, templates, document_categories,
    │                  document_relations, auto_rules, reminders, document_versions
    │
    ▼
storage.rs ──► sgd_data/files/<carpeta>/ (copia del archivo)
```

---

## Base de Datos (SQLite)

### Tabla `documents`

| Columna        | Tipo   | Descripción                              |
| -------------- | ------ | ---------------------------------------- |
| id             | TEXT   | UUID v4, clave primaria                  |
| name           | TEXT   | Nombre visible del documento             |
| file_type      | TEXT   | Extensión (pdf, xlsx, xls, docx, pptx)   |
| file_path      | TEXT   | Ruta relativa dentro de `sgd_data/files/` |
| original_name  | TEXT   | Nombre original del archivo              |
| size           | INT    | Tamaño en bytes                          |
| description    | TEXT   | Descripción opcional                     |
| notes          | TEXT   | Notas/anotaciones del usuario            |
| checksum       | TEXT   | SHA256 del archivo al importar           |
| created_at     | TEXT   | Fecha/hora de ingreso (YYYY-MM-DD HH:MM:SS) |
| updated_at     | TEXT   | Fecha/hora de última modificación        |
| favorite       | INT    | 0/1 — marcado como favorito             |
| deleted_at     | TEXT   | NULL o fecha de eliminación (soft delete) |
| content_text   | TEXT   | Texto extraído del PDF/Excel/docx/pptx   |

### Tabla `categories`

| Columna     | Tipo | Descripción                    |
| ----------- | ---- | ------------------------------ |
| id          | TEXT | UUID v4, clave primaria        |
| name        | TEXT | Nombre único de la carpeta     |
| description | TEXT | Descripción opcional           |
| icon        | TEXT | Icono emoji (ej. "📁")         |

### Tabla `document_categories`

| Columna      | Tipo | Descripción                               |
| ------------ | ---- | ----------------------------------------- |
| document_id  | TEXT | FK → documents(id) ON DELETE CASCADE      |
| category_id  | TEXT | FK → categories(id) ON DELETE CASCADE     |
| PRIMARY KEY  |      | (document_id, category_id)                |

### Tabla `templates`

| Columna     | Tipo | Descripción                    |
| ----------- | ---- | ------------------------------ |
| id          | TEXT | UUID v4, clave primaria        |
| name        | TEXT | Nombre único de la plantilla   |
| description | TEXT | Texto que auto-llena al usarla |

### Tabla `history`

| Columna      | Tipo   | Descripción                              |
| ------------ | ------ | ---------------------------------------- |
| id           | TEXT   | UUID v4, clave primaria                  |
| action_type  | TEXT   | Tipo: add, edit, delete, category_add, etc. |
| action_label | TEXT   | Texto legible: "Documento agregado: X"   |
| document_id  | TEXT   | FK opcional → documents(id)              |
| timestamp    | TEXT   | Fecha/hora (YYYY-MM-DD HH:MM:SS)         |

### Tabla `document_relations`

| Columna       | Tipo | Descripción                               |
| ------------- | ---- | ----------------------------------------- |
| id            | TEXT | UUID v4, clave primaria                   |
| source_id     | TEXT | FK → documents(id) ON DELETE CASCADE      |
| target_id     | TEXT | FK → documents(id) ON DELETE CASCADE      |
| relation_type | TEXT | "related", "duplicate", "supersedes"      |

### Tabla `auto_rules`

| Columna      | Tipo | Descripción                               |
| ------------ | ---- | ----------------------------------------- |
| id           | TEXT | UUID v4, clave primaria                   |
| name         | TEXT | Nombre descriptivo de la regla            |
| pattern      | TEXT | Texto a buscar en el nombre del archivo   |
| category_id  | TEXT | FK → categories(id) ON DELETE CASCADE     |

### Tabla `reminders`

| Columna      | Tipo | Descripción                               |
| ------------ | ---- | ----------------------------------------- |
| id           | TEXT | UUID v4, clave primaria                   |
| document_id  | TEXT | FK → documents(id) ON DELETE CASCADE      |
| note         | TEXT | Texto del recordatorio                    |
| due_date     | TEXT | Fecha límite (YYYY-MM-DD)                 |
| done         | INT  | 0/1 — completado o pendiente              |

### Tabla `document_versions`

| Columna      | Tipo | Descripción                               |
| ------------ | ---- | ----------------------------------------- |
| id           | TEXT | UUID v4, clave primaria                   |
| document_id  | TEXT | FK → documents(id) ON DELETE CASCADE      |
| file_path    | TEXT | Ruta de la versión anterior               |
| size         | INT  | Tamaño en bytes                           |
| checksum     | TEXT | SHA256 de la versión                      |
| created_at   | TEXT | Fecha de la versión                       |

---

## Componentes

### `models.rs` — Estructuras de datos

- **Document**: con `Default`, incluye `favorite` (bool), `deleted_at` (Option\<String\>), `content_text` (String — texto extraído del PDF/Excel/docx), `notes` (String), `checksum` (String — SHA256, streaming con `sha2`)
- **Category**: `id`, `name`, `description`, `icon` (String — emoji)
- **UndoAction**: histórico de acciones para Ctrl+Z
- **DocumentRelation**: enlaces entre documentos
- **AutoRule**: reglas de categorización automática por patrón
- **Reminder**: recordatorios por documento
- **DocumentVersion**: versiones anteriores de un documento
- **Theme** (enum, 15 variantes): Claro, Oscuro, Alto Contraste, Bosque, Océano, Atardecer, Medianoche, Lavanda, Coral, Grafito, Retro, Terminal, Halloween 🎃, Navidad 🎄, El Ari 🟣
  - `Theme::list()` → `Vec<(Theme, nombre_es, nombre_en)>`
  - `Theme::to_visuals()` → `egui::Visuals`
  - `Theme::preview_color()` → `egui::Color32`
  - `Theme::preview_colors()` → `Vec<Color32>` — para temas bicolor
- **Language** (enum): Spanish, English
- **Settings**: theme, language, confirm_delete, auto_open_after_import, font_size, reduced_motion, table_density, show_column_{type,size,date}, trash_auto_delete_days, backup_{enabled,interval_hours,path}, watch_folder_{enabled,path}
  - `Settings::load(path)` / `save(path)` → JSON persistente

### `db.rs` — Capa de datos

**Inicialización:**
- `init_db(db_path)` → crea tablas, migra columnas faltantes y agrega **12 índices** (FKs, `deleted_at`, `favorite`, `updated_at`, `timestamp`, `done`)
- `ensure_default_categories(&conn)` → crea "PDF" 📄, "Excel" 📊, "Docs" 📝, "Presentaciones" 📹 (solo si no existen)

**Documentos:** insert, update, toggle_favorite, soft_delete, restore, permanently_delete (usa CASCADE), batch_soft_delete, batch_permanently_delete, get_all, get_trashed, get_favorite, search, search_documents_by_category, get_by_category, get_recent

**Carpetas:** insert, delete, get_all, set/get document categories (con transacción), get_counts

**Plantillas:** insert, delete, get_all

**Historial:** insert, get_by_date, get_dates

**Relaciones:** insert_relation, delete_relation, get_relations_for_document

**Reglas:** insert_auto_rule, delete_auto_rule, get_all_auto_rules, match_auto_rules (por nombre de archivo)

**Recordatorios:** insert_reminder, update_reminder_done, delete_reminder, get_reminders_for_document, get_all_pending_reminders

**Versiones:** insert_document_version, get_versions_for_document

**Estadísticas:** get_count_by_month, get_count_by_type, get_total_size, get_all_document_counts (con COUNT(*))

**Utilidades:** delete_trashed_older_than (vaciado automático de papelera, usa CASCADE)

### `storage.rs` — Sistema de archivos

```
sgd_data/
└── files/
    ├── PDF/     ├── Excel/     ├── Docs/     └── ...
```

- `Storage::new(base_path)` — configura rutas
- `init()` — crea directorios base
- `ensure_subdir(name)` — crea subdirectorio si no existe
- `import_file(source_path, subdir)` → `(rel_path, original_name, size)`
- `delete_file(relative_path)` — elimina archivo
- `get_full_path(relative_path)` — resuelve ruta absoluta
- `copy_to(relative_path, dest_path)` — copia a ubicación externa
- `calculate_checksum(path)` → SHA256 del archivo (lectura por chunks de 64KB, evita OOM)
- `backup_all(dest_dir)` — copia DB, settings y toda la carpeta files

### `app.rs` — Interfaz de usuario

**Ventanas y diálogos:**

| Diálogo                  | Botón / acceso                 | Descripción                              |
| ------------------------ | ------------------------------ | ---------------------------------------- |
| Agregar Documento        | ➕ Agregar (barra) / Ctrl+N     | Drag & drop, seleccionar archivo o carpeta completa |
| Editar Documento         | + menú → Editar                | Nombre, descripción, notas, carpeta      |
| Gestionar Carpetas       | 🏷️ Carpetas (sidebar)          | Crear/eliminar con icono                 |
| Gestionar Plantillas     | 📋 Plantillas (sidebar)        | Crear/eliminar plantillas                |
| Historial                | 📜 Historial (sidebar)         | Calendario + listado de acciones         |
| Estadísticas             | 📊 Estadísticas (sidebar)      | Gráficos por tipo y mes, totales         |
| Configuración            | ⚙️ Config (sidebar abajo)      | Tema, idioma, comportamiento, accesibilidad, columnas, papelera, vigilancia |
| Filtros Avanzados        | 🔎 en barra superior           | Tipo, tamaño, fecha (ventana propia)     |
| Relaciones               | + menú → Relaciones            | Vincular documentos                      |
| Reglas Auto-Categorización| 🏷️ (desde Config)             | Patrones por nombre de archivo           |
| Recordatorios            | + menú → Recordatorio          | Fecha + nota + checklist                 |
| Versiones                | + menú → Versiones             | Historial de versiones, restaurar        |
| Copia Respaldo           | 📥 (sidebar)                   | Backup manual o automático               |
| Carpeta (popup)          | "..." en sidebar/Agregar       | Carpetas no predeterminadas              |
| Temas (popup)            | "+ otros" en Config            | Temas adicionales                        |

**Sidebar:**
1. **Explorar**: Todos 📄 (+badge), Favoritos ⭐ (+badge), Recientes 🕒, Papelera 🗑️ (+badge)
2. Gestión: 🏷️ Carpetas, 📋 Plantillas, 📜 Historial, 📊 Estadísticas, 📥 Copia Respaldo
3. **Carpetas** (scroll, max 80px): PDF 📄, Excel 📊, Docs 📝, Presentaciones 📹 + "..."
4. ⚙️ Config (fijo al fondo)

**Atajos y QoL:**
- **Ctrl+N**: Agregar documento
- **Ctrl+Z**: Deshacer última acción (eliminación permanente o mover a papelera)
- Búsqueda reactiva con resaltado en tabla
- Indicador **"!" rojo** en documentos con recordatorios pendientes
- Selección múltiple con checkbox + acciones en lote (papelera/exportar/cambiar carpeta/eliminar)
- Columnas personalizables (mostrar/ocultar Tipo, Tamaño, Fecha)
- Densidad de tabla ajustable (compacta/normal/cómoda)
- Auto-categorización por tipo de archivo y por reglas personalizadas
- Checksums SHA256 al importar para detección de duplicados
- Papelera con vaciado automático según días de retención
- Vigilancia de carpeta para importación automática (vía `notify`)
- Importar carpeta completa arrastrando o desde botón
- Backup manual (seleccionar destino) o automático (intervalo configurable)

---

## Temas disponibles

| Tema          | Descripción                    | Base   | Color preview             |
| ------------- | ------------------------------ | ------ | ------------------------- |
| Claro         | Tema claro por defecto         | light  | ██ `rgb(235,235,235)`      |
| Oscuro        | Tema oscuro por defecto        | dark   | ██ `rgb(40,40,45)`         |
| Alto Contraste| Blanco/negro de alto contraste | light  | ██ `rgb(255,255,255)`      |
| Bosque        | Tonos verdes suaves            | light  | ██ `rgb(70,155,70)`        |
| Océano        | Tonos azules suaves            | light  | ██ `rgb(55,115,195)`       |
| Atardecer     | Tonos naranja/cálidos          | light  | ██ `rgb(215,115,45)`       |
| Medianoche    | Azul oscuro profundo           | dark   | ██ `rgb(28,28,65)`         |
| Lavanda       | Tonos púrpura suaves           | light  | ██ `rgb(155,115,200)`      |
| Coral         | Tonos coral/salmón             | light  | ██ `rgb(220,100,100)`      |
| Grafito       | Grises oscuros                 | dark   | ██ `rgb(60,60,65)`         |
| Retro         | Ámbar/verde retro              | dark   | ██ `rgb(200,160,50)`       |
| Terminal      | Verde sobre negro              | dark   | ██ `rgb(0,200,0)`          |
| Halloween 🎃 | Naranja + morado               | dark   | ██/██ `rgb(230,120,0)+rgb(140,0,200)` |
| Navidad 🎄   | Rojo + verde navideño          | dark   | ██/██ `rgb(200,30,30)+rgb(0,160,60)`  |
| El Ari 🟣    | Púrpura tiro (Tyrian) #61063B  | dark   | ██ `rgb(97,6,59)`         |

En Config, recuadros de 24×24 px con borde de selección. Temas bicolor partidos.

---

## Configuración persistente

Archivo `sgd_data/settings.json`:

```json
{
    "theme": "Light",
    "language": "Spanish",
    "confirm_delete": true,
    "auto_open_after_import": false,
    "font_size": 14.0,
    "reduced_motion": false,
    "table_density": 30.0,
    "show_column_type": true,
    "show_column_size": true,
    "show_column_date": true,
    "trash_auto_delete_days": 30,
    "backup_enabled": false,
    "backup_interval_hours": 24,
    "backup_path": "",
    "watch_folder_enabled": false,
    "watch_folder_path": ""
}
```

---

## Compilación y ejecución

```bash
cargo build
cargo build --release
cargo run
./target/release/sgd-local
```

**Requisitos:** Rust 2021 edition. Dependencias: eframe, egui, rusqlite (bundled), uuid, chrono, rfd, opener, serde, serde_json, pdf-extract, calamine, sha2, notify.

---

## Registro de cambios

### Sesión 1 — Creación inicial
- Estructura base, CRUD completo, SQLite + sistema de archivos

### Sesión 2 — Internacionalización
- Traducción Español/Inglés, drag & drop, auto-categorización PDF/Excel, 8 temas

### Sesión 3 — Historial
- Calendario, ordenamiento, doble-click, auto-limpieza mensajes

### Sesión 4 — UI/UX, iconos, búsqueda reactiva
- Selector de temas con color preview, búsqueda al instante

### Sesión 5 — Selector compacto de temas
- 3 principales + "+ otros"

### Sesión 6 — Favoritos, papelera, stats, preview, búsqueda por contenido/fecha
- Favoritos ⭐, Papelera 🗑️, Vista previa, extracción PDF/Excel, estadísticas

### Sesión 7 — Eliminación de export/import ZIP, reorganización del sidebar
- Sidebar con gestión, Config al fondo, barra superior limpia

### Sesión 8 — Carpetas, iconos, temas, sidebar
- "Categorías" → "Carpetas", iconos, 4 predeterminadas, subdirectorios, +4 temas, accesibilidad, calendario
- Auto-detección .docx/.pptx

### Sesión 10 — Vista previa eliminada, indicador de recordatorios, icono "+" y optimizaciones masivas
- **Vista previa eliminada** (redundante con editar/preview en tabla)
- **"!" rojo** en documentos con recordatorios pendientes
- **Icono de acciones** cambiado de ⋮ a +
- **12 índices SQL** agregados (FKs, `deleted_at`, `favorite`, `updated_at`, `timestamp`, `done`)
- **N+1 eliminados**: recordatorios (HashSet vía `get_all_pending_reminders`) y búsqueda por categoría (JOIN directo en SQL)
- **COUNT(*) en vez de carga completa** en `get_all_document_counts`
- **Transacción** en `set_document_categories`
- **CASCADE** simplifica `permanently_delete_document` y `delete_trashed_older_than`
- **Batch operations**: `batch_soft_delete` y `batch_permanently_delete` con `IN (...)` cláusula
- **Checksum**: migrado de `sha256` a `sha2` con lectura por chunks de 64KB (evita OOM)
- **Código muerto eliminado**: `set_document_checksum`, `search_documents_by_date`, `get_document_by_id`, `update_category`
- **`#[derive(Default)]`** en Document, Category, Template, HistoryEntry, UndoAction
- **`file_type_style()`** extraída como función (elimina duplicación de mapeo tipo/icono/color)
- **Bug corregido**: `import_folder_recursive` ahora retorna el conteo real (AtomicUsize nunca se incrementaba)
- **0 warnings** de compilación

### Sesión 9 — Features masivas
- **Filtros avanzados**: ventana propia con tipo/tamaño/fecha
- **Selección múltiple**: checkboxes + acciones en lote
- **Contadores badge** en sidebar
- **Columnas personalizables** + densidad de tabla
- **Notas/anotaciones** por documento
- **Checksums SHA256** al importar
- **Versiones**: historial + restaurar versiones anteriores
- **Relaciones**: vincular documentos (relacionado/duplicado/reemplaza)
- **Reglas de auto-categorización**: patrones por nombre
- **Recordatorios**: fecha + nota + checklist
- **Búsqueda guardada** (eliminada posteriormente por petición)
- **Resaltado de coincidencias** en vista previa
- **Deshacer** Ctrl+Z (50 acciones, papelera/eliminación)
- **Importar carpeta completa** (arrastrar o botón)
- **Vigilancia de carpeta** (`notify`, importación automática)
- **Backup manual/automático** (copia DB + settings + archivos)
- **Papelera vaciado automático** (días de retención configurable)
