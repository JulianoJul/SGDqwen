# Plan de Modificaciones - SGD Auditoría de Código

**Fecha:** 2025-01-13
**Archivos auditados:** combined.txt
**Líneas totales:** 3747

---

## Hallazgos Priorizados

### SEVERIDAD ALTA

#### 1. Hardcodeo de ruta base en constante
- **Archivo:** `src/models.rs`
- **Descripción:** `DEFAULT_BASE_PATH` hardcodeado como `"./sgd_data"`. Debería ser configurable vía env var.
- **Fix sugerido:** Leer de `std::env::var("SGD_BASE_PATH")` con fallback a `"./sgd_data"`.
- **Estado:** pendiente

#### 2. unwrap() que puede panic en import_document
- **Archivo:** `src/app.rs`
- **Descripción:** `let src = self.add_file_path.clone().unwrap();` puede panic si `add_file_path` es None.
- **Fix sugerido:** Reemplazar con `if let Some(src) = self.add_file_path.clone()`.
- **Estado:** pendiente

#### 3. Falta de transacción en batch_permanently_delete
- **Archivo:** `src/db.rs`
- **Descripción:** SELECT + DELETE sin transacción. Si falla entre medio, queda inconsistente.
- **Fix sugerido:** Envolver en `conn.transaction()`.
- **Estado:** pendiente

#### 4. Interpolación SQL dinámica (potencial injection)
- **Archivo:** `src/db.rs`
- **Descripción:** `format!` para construir queries con placeholders dinámicos.
- **Fix sugerido:** Validar que los IDs sean UUIDs válidos antes de usarlos.
- **Estado:** pendiente

#### 5. N+1 query potencial en get_document_counts_by_category
- **Archivo:** `src/db.rs`
- **Descripción:** Una query por categoría. Con muchas categorías, N+1 queries.
- **Fix sugerido:** Usar GROUP BY simple en una sola query.
- **Estado:** pendiente

---

### SEVERIDAD MEDIA

#### 6. Duplicación de lógica de extensión de archivo
- **Archivo:** `src/app.rs`
- **Descripción:** `path.extension().unwrap_or_default().to_string_lossy().to_lowercase()` aparece 6 veces.
- **Fix sugerido:** Extraer a función helper `fn get_extension(path: &Path) -> String`.
- **Estado:** pendiente

#### 7. Duplicación de lógica de file_stem
- **Archivo:** `src/app.rs`
- **Descripción:** `path.file_stem().unwrap_or_default().to_string_lossy().to_string()` aparece 5 veces.
- **Fix sugerido:** Extraer a función helper `fn get_file_stem(path: &Path) -> String`.
- **Estado:** pendiente

#### 8. Duplicación de status_msg con match Language
- **Archivo:** `src/app.rs`
- **Descripción:** Patrón `self.status_msg(l, format!(...), format!(...))` en >15 lugares.
- **Fix sugerido:** Crear macro o función helper que acepte closure.
- **Estado:** pendiente

#### 9. Duplicación de refresh_data calls
- **Archivo:** `src/app.rs`
- **Descripción:** `refresh_data()` llama a 8 funciones db distintas con `.unwrap_or_default()`.
- **Fix sugerido:** Considerar caching o carga diferida.
- **Estado:** pendiente

#### 10. Lógica de ordenamiento duplicada
- **Archivo:** `src/app.rs`
- **Descripción:** `sort_documents()` con comparación inline.
- **Fix sugerido:** Extraer comparadores a funciones separadas por campo.
- **Estado:** pendiente

#### 11. DEFAULT_CATEGORIES hardcodeado
- **Archivo:** `src/models.rs`
- **Descripción:** Categorías por defecto hardcodeadas. Deberían ser configurables.
- **Fix sugerido:** Permitir override vía env var `SGD_DEFAULT_CATEGORIES`.
- **Estado:** pendiente

#### 12. CATEGORY_ICONS hardcodeado
- **Archivo:** `src/app.rs`
- **Descripción:** Icons hardcodeados como constante.
- **Fix sugerido:** Mover a models.rs junto con DEFAULT_CATEGORIES.
- **Estado:** pendiente

#### 13. MONTH_NAMES y DAY_NAMES hardcodeados
- **Archivo:** `src/app.rs`
- **Descripción:** Nombres de meses y días en ES/EN hardcodeados.
- **Fix sugerido:** Mover a módulo de constantes de i18n.
- **Estado:** pendiente

#### 14. file_type_style hardcodeado
- **Archivo:** `src/app.rs`
- **Descripción:** Mapping de tipos a iconos/colores hardcodeado.
- **Fix sugerido:** Mover a models.rs como constante estructurada.
- **Estado:** pendiente

---

### SEVERIDAD BAJA

#### 15. tr() función no implementa traducción real
- **Archivo:** `src/app.rs`
- **Descripción:** `tr()` siempre devuelve español ignorando el Language.
- **Fix sugerido:** Documentar que es placeholder para i18n futuro.
- **Estado:** pendiente

#### 16. highlight_text usa marcador Unicode hardcoded
- **Archivo:** `src/app.rs`
- **Descripción:** Usa `"🟢"` como marcador de highlight.
- **Fix sugerido:** Hacer configurable o usar enfoque diferente.
- **Estado:** pendiente

#### 17. format_size con literales de unidades
- **Archivo:** `src/app.rs`
- **Descripción:** Las unidades "B", "KB", "MB", "GB" hardcodeadas.
- **Fix sugerido:** Parametrizar con Language.
- **Estado:** pendiente

#### 18. ensure_category_dirs llamada redundante
- **Archivo:** `src/app.rs`
- **Descripción:** Se llama en `new()` y en cada `refresh_data()`.
- **Fix sugerido:** Solo llamar cuando haya nuevas categorías.
- **Estado:** pendiente

#### 19. auto_select_categories_for_path con lógica duplicada
- **Archivo:** `src/app.rs`
- **Descripción:** Llama a `match_auto_rules` y luego itera.
- **Fix sugerido:** Refactorizar para evitar iteración extra si ya hay match.
- **Estado:** pendiente

#### 20. resolve_storage_subdir con fallback hardcoded
- **Archivo:** `src/app.rs`
- **Descripción:** Fallback a `"pdf"` si no se puede determinar extensión.
- **Fix sugerido:** Usar constante.
- **Estado:** pendiente

---

## Resumen por Categoría

| Categoría | Count | Severidad Predominante |
|-----------|-------|------------------------|
| Hardcodeo | 7 | Alta/Media |
| DRY/Reutilización | 8 | Media |
| Manejo de Errores | 1 | Alta |
| SQL | 4 | Alta/Media |

---

## Reglas del Proceso para OpenCode

1. `plan_modificaciones.md` es la guía — opencode lee este archivo e implementa los fixes uno por uno de mayor a menor prioridad
2. Después de cada fix, opencode ejecuta `make build` para verificar que compila
3. Al terminar todos los fixes, opencode ejecuta `make build && make combine`
4. opencode actualiza `doc.md` con historial de cambios (fecha, archivos, razón de cada fix)
5. opencode sube a GitHub con `make github msg="mensaje"`
