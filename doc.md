# SGD - Sistema de Gestión Documental

## Arquitectura

App 100% Rust. **egui (inmediate-mode) = UI** | **rusqlite = Base de datos** | **sistema de archivos local para almacenamiento físico**.

Compila a un solo binario. Sin Python, sin runtime externo.

```
┌───────────────────────────────────────────────────┐
│  Rust (egui + eframe)                             │
│  ├── src/main.rs   — entry point, ventana 1100x700│
│  ├── src/app.rs    — UI completa + estado global   │
│  ├── src/db.rs     — capa SQLite (CRUD + migraciones)│
│  ├── src/models.rs — structs de datos              │
│  └── src/storage.rs— manejo físico de archivos     │
│                                                   │
│  SQLite ←─────────────────────────────────────────┤
│  sgd_data/documents.db                            │
│  sgd_data/files/ — archivos importados            │
└───────────────────────────────────────────────────┘
```

## Flujo de Datos

```
Usuario (UI egui)
    │
    ▼
app.rs ──► db.rs ──► documents.db (SQLite)
    │              └── categories, templates, history,
    │                  document_categories, document_relations,
    │                  auto_rules, reminders, document_versions
    │
    ▼
storage.rs ──► sgd_data/files/<carpeta>/ (copia del archivo)
```

## Base de Datos (SQLite)

### `documents`
| Columna | Tipo | Descripción |
|---------|------|-------------|
| `id` | TEXT | UUID v4 PK |
| `name` | TEXT | Nombre visible |
| `file_type` | TEXT | pdf / xlsx / xls / docx / pptx |
| `file_path` | TEXT | Ruta relativa en `sgd_data/files/` |
| `original_name` | TEXT | Nombre original del archivo |
| `size` | INT | Bytes |
| `description` | TEXT | Descripción opcional |
| `notes` | TEXT | Anotaciones del usuario |
| `checksum` | TEXT | SHA256 del archivo |
| `created_at` | TEXT | YYYY-MM-DD HH:MM:SS |
| `updated_at` | TEXT | Última modificación |
| `favorite` | INT | 0/1 |
| `deleted_at` | TEXT | NULL o fecha (soft delete) |
| `content_text` | TEXT | Texto extraído del archivo |

### `categories`
| Columna | Tipo | Descripción |
|---------|------|-------------|
| `id` | TEXT | UUID v4 PK |
| `name` | TEXT | Único |
| `description` | TEXT | Opcional |
| `icon` | TEXT | Emoji (ej. 📁) |

### `document_categories`
| Columna | Tipo | Descripción |
|---------|------|-------------|
| `document_id` | TEXT | FK → documents(id) CASCADE |
| `category_id` | TEXT | FK → categories(id) CASCADE |
| PK | | (document_id, category_id) |

### `templates`, `history`, `document_relations`, `auto_rules`, `reminders`, `document_versions`
Ver `DOCUMENTACION.md` o `src/db.rs` para schema completo. 8 tablas, 12 índices.

## Componentes

### `src/models.rs` — Estructuras de datos
- **Document**: con `Default`, favorite (bool), deleted_at (Option\<String\>), content_text, notes, checksum
- **Category**: id, name, description, icon
- **UndoAction**: historial para Ctrl+Z (hasta 50 acciones)
- **DocumentRelation**: enlaces entre documentos
- **AutoRule**: categorización automática por patrón en nombre
- **Reminder**: recordatorios por documento con fecha
- **DocumentVersion**: versiones anteriores
- **Theme** (enum, 15 variantes): Light, Dark, HighContrast, Forest, Ocean, Sunset, Midnight, Lavender, Coral, Graphite, Retro, Terminal, Halloween, Navidad, ElAri
  - `list()` → traducciones ES/EN
  - `to_visuals()` → `egui::Visuals` personalizados
  - `preview_color()` / `preview_colors()` → Color32
- **Language**: Spanish / English
- **Settings**: theme, language, font_size, columnas visibles, trash_auto_delete_days, backup, watch_folder
  - `load(path)` / `save(path)` → JSON persistente

### `src/db.rs` — Capa de datos
- **init_db**: crea tablas, migra columnas faltantes, 12 índices
- **ensure_default_categories**: PDF, Excel, Docs, Presentaciones
- CRUD completo para documentos, categorías, plantillas, historial, relaciones, reglas, recordatorios, versiones
- Batch operations: `batch_soft_delete`, `batch_permanently_delete`
- Estadísticas: counts por categoría/tipo/mes, tamaño total
- `delete_trashed_older_than`: vaciado automático de papelera con CASCADE

### `src/storage.rs` — Sistema de archivos
- **import_file**: copia a `sgd_data/files/<subdir>/<uuid>.<ext>`, retorna `(rel_path, original_name, size)`
- **calculate_checksum**: SHA256 por chunks de 64KB
- **backup_all**: copia DB + settings + files a destino
- **delete_file**, **get_full_path**, **copy_to**

### `src/app.rs` — Interfaz de usuario
**Diálogos principales:**
- Agregar Documento (+ / Ctrl+N): drag & drop, selector archivo, importar carpeta
- Editar Documento: nombre, descripción, notas, carpeta
- Gestionar Carpetas: crear/eliminar con icono
- Gestionar Plantillas, Historial (calendario), Estadísticas (gráficos por tipo/mes)
- Configuración: tema, idioma, comportamiento, columnas, papelera, vigilancia
- Filtros Avanzados: tipo, tamaño, fecha
- Relaciones, Reglas Auto-Categorización, Recordatorios, Versiones, Backup

**Sidebar:**
- Explorar: Todos, Favoritos ⭐, Recientes 🕒, Papelera 🗑️ (con badges)
- Gestión: Carpetas, Plantillas, Historial, Estadísticas, Backup
- Carpetas (scroll): PDF, Excel, Docs, Presentaciones
- Config ⚙️ (fijo al fondo)

**QoL:**
- Ctrl+Z: deshacer (papelera/eliminación)
- Búsqueda reactiva con resaltado en tabla
- Selección múltiple + acciones en lote
- Auto-categorización por tipo y reglas personalizadas
- Checksums SHA256, papelera con vaciado automático
- Vigilancia de carpeta (`notify`), importar carpeta completa
- Backup manual/automático

## Temas (15)

Claro, Oscuro, Alto Contraste, Bosque 🌲, Océano 🌊, Atardecer 🌅, Medianoche 🌙, Lavanda 💜, Coral 🪸, Grafito ⚪, Retro 🕰️, Terminal 💻, Halloween 🎃, Navidad 🎄, El Ari 🟣.

Cada tema implementa `to_visuals()` → `egui::Visuals` con paleta personalizada.

## Dependencias (Cargo.toml)

| Crate | Versión | Propósito |
|-------|---------|-----------|
| `eframe` | 0.27 | Ventana + loop de eventos |
| `egui` | 0.27 | UI inmediate-mode |
| `rusqlite` | 0.31 | SQLite (bundled) |
| `uuid` | 1 | IDs de documentos |
| `chrono` | 0.4 | Fechas y timestamps |
| `rfd` | 0.14 | Diálogos de archivo |
| `opener` | 0.7 | Abrir archivos con app por defecto |
| `serde/serde_json` | 1 | Serialización settings/JSON |
| `pdf-extract` | 0.7 | Extraer texto de PDF |
| `calamine` | 0.26 | Leer Excel (xlsx/xls) |
| `sha2` | 0.10 | Checksums SHA256 |
| `notify` | 7 | Vigilancia de carpeta |

## Makefile

```bash
make build       # cargo build
make release     # cargo build --release
make run         # cargo run
make clean       # cargo clean + rm combined.txt
make combine     # concatena todo el código en combined.txt
make commit msg="x"  # git add -A + git commit
make push        # git push
make github msg="x"  # commit + push (remote: https://github.com/JulianoJul/SGDqwen)
```

## Compilación

```bash
cargo build
cargo run
./target/release/sgd-local
```

**Requisitos:** Rust edition 2021. No requiere dependencias externas del sistema.

## Historial de Cambios

### Fixes de Auditoría (Junio 2026)

| # | Archivo | Cambio | Razón |
|---|---------|--------|-------|
| 1 | `src/app.rs` | `db_path.to_str().unwrap()` → `.ok_or("Invalid path")?` | Posible panic con rutas no UTF-8 |
| 2 | `src/app.rs` | Botón Guardar validado con `add_file_path.is_some()` | Posible panic al hacer unwrap de None |
| 3 | `src/app.rs` | `days_in_month` usa `and_then` en vez de `unwrap()` | Posible panic con fechas inválidas |
| 4 | `src/models.rs` | `DEFAULT_BASE_PATH` ahora es `default_base_path()` que lee de env var `SGD_BASE_PATH` | Configurable vs hardcodeado |
| 5 | `src/app.rs`, `src/storage.rs`, `src/db.rs` | Nuevas constantes `DB_FILENAME`, `SETTINGS_FILENAME`, `FILES_SUBDIR` | DRY + mantenibilidad |
| 6 | `src/models.rs` | `default_categories()`, helpers `is_default_category()`, `file_type_to_category_name()`, `category_icon()` | Eliminar hardcodeo de nombres de categorías en 10+ ubicaciones |
| 7 | `src/app.rs` | Sidebar, add dialog, categories popup usan `default_categories()` e `is_default_category` | DRY + mantenibilidad |
| 8 | `src/app.rs` | `file_type_to_category_name()` reemplaza 3 mapeos duplicados ext→categoría | DRY |
| 9 | `src/app.rs` | Nuevo método `status_msg()` para mensajes bilingües, ~15 llamadas migradas | DRY: elimina patrón `match l` repetido |
| 10 | `src/db.rs` | `batch_permanently_delete` envuelto en transacción | Consistencia: SELECT+DELETE ahora atómico |
| 11 | `src/db.rs` | `batch_soft_delete`/`batch_permanently_delete` validan UUIDs | Seguridad: evitar SQL injection en placeholders dinámicos |
| 12 | `src/app.rs`, `src/models.rs` | `get_extension()` y `get_file_stem()` helpers, reemplazan 11 duplicaciones | DRY |
| 13 | `src/models.rs` | `CATEGORY_ICONS`, `MONTH_NAMES_ES/EN`, `DAY_NAMES_ES/EN`, `FILE_TYPE_STYLES` movidos a models.rs | Constantes en lugar único |
| 14 | `src/app.rs` | `format_size` con array `SIZE_UNITS`, `highlight_text` con const `HIGHLIGHT_MARKER` | Constantes vs literales |
| 15 | `src/app.rs` | `tr()` documentada como placeholder i18n | Claridad |

## Reglas del Proceso

1. **doc.md primero**: antes de cualquier implementación o cambio de código, actualizar esta documentación con lo que se planea hacer.
2. **Makefile siempre**: después de cambios, ejecutar `make build` y `make combine`.
3. **Zero hardcodeo**: cero assumptions de naming o rutas. Todo configurable desde Settings o constantes.
4. **DRY + Reutilización**: toda pieza de lógica debe tener una representación única. No repetir código ni copiar-pegar bloques. Si un patrón aparece en más de un lugar, extraer a función reutilizable. La modularidad no se mide en líneas por archivo, sino en ausencia de redundancia y en que cada función tenga una única responsabilidad (SRP).
5. **Historial de cambios**: cada cambio debe agregarse a `doc.md` con fecha, archivo y razón.

## Estructura del Proyecto

```
sgd-local/
├── Cargo.toml
├── Cargo.lock
├── Makefile
├── doc.md
├── DOCUMENTACION.md
├── prompt
├── src/
│   ├── main.rs
│   ├── app.rs
│   ├── db.rs
│   ├── models.rs
│   └── storage.rs
└── sgd_data/          (generado en tiempo de ejecución)
    ├── documents.db
    ├── settings.json
    └── files/
        ├── PDF/
        ├── Excel/
        ├── Docs/
        ├── Presentaciones/
        └── ...
```
