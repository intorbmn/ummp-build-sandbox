//! Knowledge Base & Compatibility Engine.
//!
//! Repository over SQLite + JSON/YAML knowledge files for mappings, version/loader
//! compatibility, breaking changes, migration recipes, and search.
//!
//! Tasks: 5.1 SQLite schema + migrations, 5.2 Mapping import/export,
//! 5.3 Version/loader compatibility matrix, 5.4 Breaking change DB,
//! 5.5 Migration recipes (CRUD + versioning), 5.6 Knowledge search,
//! 5.7 Schema versioning + seed data.

use crate::cache::CacheKey;
use crate::persistence::DbError;
use rusqlite::{params, Connection};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::Path;

/// API mapping between versions/loaders.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Mapping {
    pub version: String,
    pub loader: String,
    pub from_api: String,
    pub to_api: String,
    pub kind: String,
    pub source: String,
    pub confidence: f64,
}

/// Version/loader compatibility matrix entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Compatibility {
    pub minecraft_version: String,
    pub loader: String,
    pub loader_version: String,
    pub status: String,
    pub notes: Option<String>,
}

/// Known breaking change entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BreakingChange {
    pub version: String,
    pub change: String,
    pub severity: String,
    pub workaround: Option<String>,
    pub source: String,
}

/// Reusable migration recipe.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Recipe {
    pub recipe_id: String,
    pub version: String,
    pub loader: Option<String>,
    pub description: String,
    pub steps: Vec<Value>,
    pub source: String,
    pub tested: bool,
    pub recipe_version: u32,
    pub parent_recipe_id: Option<String>,
}

/// Mod dependency metadata.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModDependency {
    pub mod_id: String,
    pub name: Option<String>,
    pub version: String,
    pub loader: Option<String>,
    pub url: Option<String>,
    pub checksum: Option<String>,
    pub source: String,
}

/// SQLite knowledge-base repository.
pub struct KnowledgeBase {
    conn: Connection,
}

// Module-level schema const (not inside impl block)
const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS schema_version (
    version INTEGER PRIMARY KEY,
    applied_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%f', 'now')),
    description TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS mappings (
    version TEXT NOT NULL,
    loader TEXT NOT NULL,
    from_api TEXT NOT NULL,
    to_api TEXT NOT NULL,
    kind TEXT NOT NULL,
    source TEXT NOT NULL,
    confidence REAL NOT NULL,
    PRIMARY KEY (version, loader, from_api, to_api, kind, source)
);

CREATE TABLE IF NOT EXISTS compatibility (
    minecraft_version TEXT NOT NULL,
    loader TEXT NOT NULL,
    loader_version TEXT NOT NULL,
    status TEXT NOT NULL,
    notes TEXT,
    PRIMARY KEY (minecraft_version, loader, loader_version)
);

CREATE TABLE IF NOT EXISTS breaking_changes (
    version TEXT NOT NULL,
    change TEXT NOT NULL,
    severity TEXT NOT NULL,
    workaround TEXT,
    source TEXT NOT NULL,
    PRIMARY KEY (version, change, severity, source)
);

CREATE TABLE IF NOT EXISTS recipes (
    recipe_id TEXT PRIMARY KEY,
    version TEXT NOT NULL,
    loader TEXT,
    description TEXT NOT NULL,
    steps_json TEXT NOT NULL,
    source TEXT NOT NULL,
    tested INTEGER NOT NULL DEFAULT 0,
    recipe_version INTEGER NOT NULL DEFAULT 1,
    parent_recipe_id TEXT
);

CREATE TABLE IF NOT EXISTS pack_formats (
    minecraft_version TEXT NOT NULL,
    kind TEXT NOT NULL,
    format INTEGER NOT NULL,
    PRIMARY KEY (minecraft_version, kind)
);

CREATE TABLE IF NOT EXISTS dependencies (
    mod_id TEXT NOT NULL,
    name TEXT,
    version TEXT NOT NULL,
    loader TEXT,
    url TEXT,
    checksum TEXT,
    source TEXT NOT NULL,
    PRIMARY KEY (mod_id, name, version, loader, source)
);

CREATE INDEX IF NOT EXISTS idx_mappings_loader ON mappings(loader);
CREATE INDEX IF NOT EXISTS idx_compatibility_loader ON compatibility(loader);
CREATE INDEX IF NOT EXISTS idx_breakingchanges_version ON breaking_changes(version);
CREATE INDEX IF NOT EXISTS idx_recipes_loader ON recipes(loader);
CREATE INDEX IF NOT EXISTS idx_recipes_parent ON recipes(parent_recipe_id);
CREATE INDEX IF NOT EXISTS idx_dependencies_mod ON dependencies(mod_id);
";

impl KnowledgeBase {
    pub fn open(path: &Path) -> Result<Self, DbError> {
        let conn = Connection::open(path)?;
        conn.execute_batch(SCHEMA)?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS analysis_cache (
                cache_kind TEXT NOT NULL,
                content_sha256 TEXT NOT NULL,
                ummp_version TEXT NOT NULL,
                rules_sha256 TEXT NOT NULL,
                payload TEXT NOT NULL,
                PRIMARY KEY (cache_kind, content_sha256, ummp_version, rules_sha256)
            );",
        )?;
        Ok(Self { conn })
    }

    /// Record schema version as applied. Use after each migration.
    pub fn record_version(&self, version: i32, description: &str) -> Result<(), DbError> {
        self.conn.execute(
            "INSERT OR REPLACE INTO schema_version (version, description) VALUES (?1, ?2)",
            params![version, description],
        )?;
        Ok(())
    }

    /// Check whether migration to `target_version` has been applied.
    pub fn is_version_applied(&self, target_version: i32) -> Result<bool, DbError> {
        let mut stmt = self
            .conn
            .prepare("SELECT version FROM schema_version WHERE version = ?1")?;
        let row = stmt.query_row(params![target_version], |row| row.get::<_, i32>(0));
        Ok(row.is_ok())
    }

    /// Apply pending schema migrations sequentially from current version to target.
    /// Returns number of migrations applied.
    pub fn apply_migrations(&self, migrations: &[(i32, &str, &str)]) -> Result<usize, DbError> {
        let mut applied = 0;
        for (version, description, sql) in migrations {
            if !self.is_version_applied(*version)? {
                self.conn.execute_batch(sql)?;
                self.record_version(*version, description)?;
                applied += 1;
            }
        }
        Ok(applied)
    }

    /// Maximum serialized cache payload accepted from SQLite (64 MiB).
    const MAX_CACHE_PAYLOAD_BYTES: i64 = 64 * 1024 * 1024;
    const MAX_CACHE_PAYLOAD_BYTES_USIZE: usize = 64 * 1024 * 1024;

    /// Read and deserialize a cache entry. Corrupt rows are deleted and treated as misses.
    pub fn cache_get<T: DeserializeOwned>(&self, key: &CacheKey) -> Result<Option<T>, DbError> {
        let payload = self.conn.query_row(
            "SELECT payload FROM analysis_cache
             WHERE cache_kind = ?1 AND content_sha256 = ?2 AND ummp_version = ?3
               AND rules_sha256 = ?4 AND length(CAST(payload AS BLOB)) <= ?5",
            params![
                key.kind.as_str(),
                key.content_sha256,
                key.ummp_version,
                key.rules_sha256,
                Self::MAX_CACHE_PAYLOAD_BYTES,
            ],
            |row| row.get::<_, String>(0),
        );
        match payload {
            Ok(json) => match serde_json::from_str(&json) {
                Ok(value) => Ok(Some(value)),
                Err(_) => {
                    self.cache_delete(key)?;
                    Ok(None)
                }
            },
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    /// Store a complete result atomically. Oversized payloads are deliberately not cached.
    pub fn cache_put<T: Serialize>(&self, key: &CacheKey, value: &T) -> Result<bool, DbError> {
        let payload = serde_json::to_string(value)?;
        if payload.len() > Self::MAX_CACHE_PAYLOAD_BYTES_USIZE {
            return Ok(false);
        }
        self.conn.execute(
            "INSERT OR REPLACE INTO analysis_cache
             (cache_kind, content_sha256, ummp_version, rules_sha256, payload)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                key.kind.as_str(),
                key.content_sha256,
                key.ummp_version,
                key.rules_sha256,
                payload,
            ],
        )?;
        Ok(true)
    }

    /// Remove a cache row.
    pub fn cache_delete(&self, key: &CacheKey) -> Result<(), DbError> {
        self.conn.execute(
            "DELETE FROM analysis_cache
             WHERE cache_kind = ?1 AND content_sha256 = ?2 AND ummp_version = ?3
               AND rules_sha256 = ?4",
            params![
                key.kind.as_str(),
                key.content_sha256,
                key.ummp_version,
                key.rules_sha256,
            ],
        )?;
        Ok(())
    }

    // --- MAPPINGS ---

    pub fn insert_mapping(&self, m: &Mapping) -> Result<(), DbError> {
        self.conn.execute(
            "INSERT OR REPLACE INTO mappings (version, loader, from_api, to_api, kind, source, confidence)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                m.version,
                m.loader,
                m.from_api,
                m.to_api,
                m.kind,
                m.source,
                m.confidence,
            ],
        )?;
        Ok(())
    }

    pub fn mappings_by_loader(&self, loader: &str) -> Result<Vec<Mapping>, DbError> {
        let mut stmt = self.conn.prepare("SELECT version, loader, from_api, to_api, kind, source, confidence FROM mappings WHERE loader = ?1")?;
        let rows: Vec<Mapping> = stmt
            .query_map(params![loader], |row| {
                Ok(Mapping {
                    version: row.get(0)?,
                    loader: row.get(1)?,
                    from_api: row.get(2)?,
                    to_api: row.get(3)?,
                    kind: row.get(4)?,
                    source: row.get(5)?,
                    confidence: row.get(6)?,
                })
            })?
            .filter_map(Result::ok)
            .collect();
        Ok(rows)
    }

    // --- COMPATIBILITY ---

    pub fn insert_compatibility(&self, c: &Compatibility) -> Result<(), DbError> {
        self.conn.execute(
            "INSERT OR REPLACE INTO compatibility (minecraft_version, loader, loader_version, status, notes)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                c.minecraft_version,
                c.loader,
                c.loader_version,
                c.status,
                c.notes,
            ],
        )?;
        Ok(())
    }

    pub fn compatibilities_by_loader(&self, loader: &str) -> Result<Vec<Compatibility>, DbError> {
        let mut stmt = self.conn.prepare(
            "SELECT minecraft_version, loader, loader_version, status, notes FROM compatibility WHERE loader = ?1",
        )?;
        let rows: Vec<Compatibility> = stmt
            .query_map(params![loader], |row| {
                Ok(Compatibility {
                    minecraft_version: row.get(0)?,
                    loader: row.get(1)?,
                    loader_version: row.get(2)?,
                    status: row.get(3)?,
                    notes: row.get(4)?,
                })
            })?
            .filter_map(Result::ok)
            .collect();
        Ok(rows)
    }

    // --- BREAKING CHANGES ---

    pub fn insert_breaking_change(&self, b: &BreakingChange) -> Result<(), DbError> {
        self.conn.execute(
            "INSERT OR REPLACE INTO breaking_changes (version, change, severity, workaround, source)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![b.version, b.change, b.severity, b.workaround, b.source],
        )?;
        Ok(())
    }

    pub fn breaking_changes_by_version(
        &self,
        version: &str,
    ) -> Result<Vec<BreakingChange>, DbError> {
        let mut stmt = self.conn.prepare(
            "SELECT version, change, severity, workaround, source FROM breaking_changes WHERE version = ?1",
        )?;
        let rows: Vec<BreakingChange> = stmt
            .query_map(params![version], |row| {
                Ok(BreakingChange {
                    version: row.get(0)?,
                    change: row.get(1)?,
                    severity: row.get(2)?,
                    workaround: row.get(3)?,
                    source: row.get(4)?,
                })
            })?
            .filter_map(Result::ok)
            .collect();
        Ok(rows)
    }

    // --- RECIPES (CRUD + versioning) ---

    pub fn insert_recipe(&self, r: &Recipe) -> Result<(), DbError> {
        self.conn.execute(
            "INSERT OR REPLACE INTO recipes (recipe_id, version, loader, description, steps_json, source, tested, recipe_version, parent_recipe_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                r.recipe_id,
                r.version,
                r.loader,
                r.description,
                serde_json::to_string(&r.steps).unwrap_or_else(|_| "[]".to_string()),
                r.source,
                i32::from(r.tested),
                r.recipe_version,
                r.parent_recipe_id,
            ],
        )?;
        Ok(())
    }

    pub fn recipe(&self, recipe_id: &str) -> Result<Option<Recipe>, DbError> {
        let mut stmt = self.conn.prepare(
            "SELECT recipe_id, version, loader, description, steps_json, source, tested, recipe_version, parent_recipe_id FROM recipes WHERE recipe_id = ?1",
        )?;
        let opt = match stmt.query_row(params![recipe_id], |row| {
            let tested: i64 = row.get(6)?;
            let rver: u32 = row.get(7)?;
            Ok(Recipe {
                recipe_id: row.get(0)?,
                version: row.get(1)?,
                loader: row.get(2).ok(),
                description: row.get(3)?,
                steps: serde_json::from_str(&row.get::<_, String>(4)?).unwrap_or_default(),
                source: row.get(5)?,
                tested: tested != 0,
                recipe_version: rver,
                parent_recipe_id: row.get(8).ok(),
            })
        }) {
            Ok(r) => Some(r),
            Err(rusqlite::Error::QueryReturnedNoRows) => None,
            Err(e) => return Err(e.into()),
        };
        Ok(opt)
    }

    pub fn recipes_by_loader(&self, loader: &str) -> Result<Vec<Recipe>, DbError> {
        let mut stmt = self.conn.prepare(
            "SELECT recipe_id, version, loader, description, steps_json, source, tested, recipe_version, parent_recipe_id FROM recipes WHERE loader = ?1",
        )?;
        let rows: Vec<Recipe> = stmt
            .query_map(params![loader], |row| {
                let tested: i64 = row.get(6)?;
                let rver: u32 = row.get(7)?;
                Ok(Recipe {
                    recipe_id: row.get(0)?,
                    version: row.get(1)?,
                    loader: row.get(2).ok(),
                    description: row.get(3)?,
                    steps: serde_json::from_str(&row.get::<_, String>(4)?).unwrap_or_default(),
                    source: row.get(5)?,
                    tested: tested != 0,
                    recipe_version: rver,
                    parent_recipe_id: row.get(8).ok(),
                })
            })?
            .filter_map(Result::ok)
            .collect();
        Ok(rows)
    }

    /// Record the pack format of `kind` (`"resource"` or `"data"`) for a version.
    pub fn insert_pack_format(
        &self,
        minecraft_version: &str,
        kind: &str,
        format: u32,
    ) -> Result<(), DbError> {
        self.conn.execute(
            "INSERT OR REPLACE INTO pack_formats (minecraft_version, kind, format) VALUES (?1, ?2, ?3)",
            params![minecraft_version, kind, format],
        )?;
        Ok(())
    }

    /// All `(minecraft_version, format)` rows of one kind.
    pub fn pack_formats(&self, kind: &str) -> Result<Vec<(String, u32)>, DbError> {
        let mut stmt = self
            .conn
            .prepare("SELECT minecraft_version, format FROM pack_formats WHERE kind = ?1")?;
        let rows = stmt
            .query_map(params![kind], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn delete_recipe(&self, recipe_id: &str) -> Result<bool, DbError> {
        let n = self.conn.execute(
            "DELETE FROM recipes WHERE recipe_id = ?1",
            params![recipe_id],
        )?;
        Ok(n > 0)
    }

    /// Create a new version of an existing recipe (task 5.5 versioning).
    /// Copies steps from parent, increments recipe_version, links parent_recipe_id.
    pub fn version_recipe(
        &self,
        parent_id: &str,
        new_id: &str,
        new_description: &str,
    ) -> Result<Recipe, DbError> {
        let parent = self
            .recipe(parent_id)?
            .ok_or_else(|| DbError::Deserialize("parent recipe not found".into()))?;
        let mut new_recipe = parent.clone();
        new_recipe.recipe_id = new_id.to_string();
        new_recipe.description = new_description.to_string();
        new_recipe.recipe_version = parent.recipe_version + 1;
        new_recipe.parent_recipe_id = Some(parent_id.to_string());
        new_recipe.tested = false;
        self.insert_recipe(&new_recipe)?;
        Ok(new_recipe)
    }

    // --- MOD DEPENDENCIES ---

    pub fn insert_dependency(&self, d: &ModDependency) -> Result<(), DbError> {
        self.conn.execute(
            "INSERT OR REPLACE INTO dependencies (mod_id, name, version, loader, url, checksum, source)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                d.mod_id,
                d.name,
                d.version,
                d.loader,
                d.url,
                d.checksum,
                d.source,
            ],
        )?;
        Ok(())
    }

    pub fn dependencies_by_mod_id(&self, mod_id: &str) -> Result<Vec<ModDependency>, DbError> {
        let mut stmt = self.conn.prepare(
            "SELECT mod_id, name, version, loader, url, checksum, source FROM dependencies WHERE mod_id = ?1",
        )?;
        let rows: Vec<ModDependency> = stmt
            .query_map(params![mod_id], |row| {
                Ok(ModDependency {
                    mod_id: row.get(0)?,
                    name: row.get(1)?,
                    version: row.get(2)?,
                    loader: row.get(3).ok(),
                    url: row.get(4).ok(),
                    checksum: row.get(5).ok(),
                    source: row.get(6)?,
                })
            })?
            .filter_map(Result::ok)
            .collect();
        Ok(rows)
    }

    /// Dependency compatibility: check if a mod's declared dependencies are satisfied by available mods.
    pub fn check_dependency_compatibility(
        &self,
        mod_id: &str,
        available_mods: &[String],
    ) -> Result<Vec<String>, DbError> {
        let deps = self.dependencies_by_mod_id(mod_id)?;
        let mut missing = Vec::new();
        for dep in deps {
            let satisfied = available_mods.iter().any(|a| a == &dep.mod_id);
            if !satisfied {
                missing.push(dep.mod_id);
            }
        }
        Ok(missing)
    }

    // --- IMPORT/EXPORT JSON ---

    pub fn import_mappings_from_file(&self, path: &Path) -> Result<(), DbError> {
        let data = std::fs::read_to_string(path)?;
        let mappings: Vec<Mapping> = serde_json::from_str(&data)?;
        for m in &mappings {
            self.insert_mapping(m)?;
        }
        Ok(())
    }

    pub fn export_mappings_to_file(&self, path: &Path) -> Result<(), DbError> {
        let mut stmt = self.conn.prepare(
            "SELECT version, loader, from_api, to_api, kind, source, confidence FROM mappings",
        )?;
        let mappings: Vec<Mapping> = stmt
            .query_map(params![], |row| {
                Ok(Mapping {
                    version: row.get(0)?,
                    loader: row.get(1)?,
                    from_api: row.get(2)?,
                    to_api: row.get(3)?,
                    kind: row.get(4)?,
                    source: row.get(5)?,
                    confidence: row.get(6)?,
                })
            })?
            .filter_map(Result::ok)
            .collect();
        let data = serde_json::to_string_pretty(&mappings)?;
        std::fs::write(path, data)?;
        Ok(())
    }

    pub fn import_compatibilities_from_file(&self, path: &Path) -> Result<(), DbError> {
        let data = std::fs::read_to_string(path)?;
        let compatibilities: Vec<Compatibility> = serde_json::from_str(&data)?;
        for c in &compatibilities {
            self.insert_compatibility(c)?;
        }
        Ok(())
    }

    pub fn export_compatibilities_to_file(&self, path: &Path) -> Result<(), DbError> {
        let compatibilities = self.compatibilities_by_loader("")?;
        let data = serde_json::to_string_pretty(&compatibilities)?;
        std::fs::write(path, data)?;
        Ok(())
    }

    pub fn import_breaking_changes_from_file(&self, path: &Path) -> Result<(), DbError> {
        let data = std::fs::read_to_string(path)?;
        let breaking_changes: Vec<BreakingChange> = serde_json::from_str(&data)?;
        for b in &breaking_changes {
            self.insert_breaking_change(b)?;
        }
        Ok(())
    }

    pub fn export_breaking_changes_to_file(&self, path: &Path) -> Result<(), DbError> {
        let mut stmt = self.conn.prepare(
            "SELECT version, change, severity, workaround, source FROM breaking_changes",
        )?;
        let rows: Vec<BreakingChange> = stmt
            .query_map(params![], |row| {
                Ok(BreakingChange {
                    version: row.get(0)?,
                    change: row.get(1)?,
                    severity: row.get(2)?,
                    workaround: row.get(3)?,
                    source: row.get(4)?,
                })
            })?
            .filter_map(Result::ok)
            .collect();
        let data = serde_json::to_string_pretty(&rows)?;
        std::fs::write(path, data)?;
        Ok(())
    }

    pub fn import_recipes_from_file(&self, path: &Path) -> Result<(), DbError> {
        let data = std::fs::read_to_string(path)?;
        let recipes: Vec<Recipe> = serde_json::from_str(&data)?;
        for r in &recipes {
            self.insert_recipe(r)?;
        }
        Ok(())
    }

    pub fn export_recipes_to_file(&self, path: &Path) -> Result<(), DbError> {
        let recipes = self.recipes_by_loader("")?;
        let data = serde_json::to_string_pretty(&recipes)?;
        std::fs::write(path, data)?;
        Ok(())
    }

    pub fn import_dependencies_from_file(&self, path: &Path) -> Result<(), DbError> {
        let data = std::fs::read_to_string(path)?;
        let deps: Vec<ModDependency> = serde_json::from_str(&data)?;
        for d in &deps {
            self.insert_dependency(d)?;
        }
        Ok(())
    }

    pub fn export_dependencies_to_file(&self, path: &Path) -> Result<(), DbError> {
        let deps = self.dependencies_by_mod_id("")?;
        let data = serde_json::to_string_pretty(&deps)?;
        std::fs::write(path, data)?;
        Ok(())
    }

    // --- KNOWLEDGE SEARCH (5.6) ---

    pub fn search_mappings(&self, pattern: &str) -> Result<Vec<Mapping>, DbError> {
        let like = format!("%{pattern}%");
        let mut stmt = self.conn.prepare(
            "SELECT version, loader, from_api, to_api, kind, source, confidence FROM mappings
             WHERE from_api LIKE ?1 OR to_api LIKE ?1 OR kind LIKE ?1",
        )?;
        let rows: Vec<Mapping> = stmt
            .query_map(params![like], |row| {
                Ok(Mapping {
                    version: row.get(0)?,
                    loader: row.get(1)?,
                    from_api: row.get(2)?,
                    to_api: row.get(3)?,
                    kind: row.get(4)?,
                    source: row.get(5)?,
                    confidence: row.get(6)?,
                })
            })?
            .filter_map(Result::ok)
            .collect();
        Ok(rows)
    }

    pub fn search_recipes(&self, pattern: &str) -> Result<Vec<Recipe>, DbError> {
        let like = format!("%{pattern}%");
        let mut stmt = self.conn.prepare(
            "SELECT recipe_id, version, loader, description, steps_json, source, tested, recipe_version, parent_recipe_id FROM recipes
             WHERE description LIKE ?1 OR source LIKE ?1",
        )?;
        let rows: Vec<Recipe> = stmt
            .query_map(params![like], |row| {
                let tested: i64 = row.get(6)?;
                let rver: u32 = row.get(7)?;
                Ok(Recipe {
                    recipe_id: row.get(0)?,
                    version: row.get(1)?,
                    loader: row.get(2).ok(),
                    description: row.get(3)?,
                    steps: serde_json::from_str(&row.get::<_, String>(4)?).unwrap_or_default(),
                    source: row.get(5)?,
                    tested: tested != 0,
                    recipe_version: rver,
                    parent_recipe_id: row.get(8).ok(),
                })
            })?
            .filter_map(Result::ok)
            .collect();
        Ok(rows)
    }

    pub fn search_breaking_changes(&self, pattern: &str) -> Result<Vec<BreakingChange>, DbError> {
        let like = format!("%{pattern}%");
        let mut stmt = self.conn.prepare(
            "SELECT version, change, severity, workaround, source FROM breaking_changes
             WHERE change LIKE ?1",
        )?;
        let rows: Vec<BreakingChange> = stmt
            .query_map(params![like], |row| {
                Ok(BreakingChange {
                    version: row.get(0)?,
                    change: row.get(1)?,
                    severity: row.get(2)?,
                    workaround: row.get(3)?,
                    source: row.get(4)?,
                })
            })?
            .filter_map(Result::ok)
            .collect();
        Ok(rows)
    }

    pub fn search_dependencies(&self, pattern: &str) -> Result<Vec<ModDependency>, DbError> {
        let like = format!("%{pattern}%");
        let mut stmt = self.conn.prepare(
            "SELECT mod_id, name, version, loader, url, checksum, source FROM dependencies
             WHERE mod_id LIKE ?1 OR name LIKE ?1 OR source LIKE ?1",
        )?;
        let rows: Vec<ModDependency> = stmt
            .query_map(params![like], |row| {
                Ok(ModDependency {
                    mod_id: row.get(0)?,
                    name: row.get(1)?,
                    version: row.get(2)?,
                    loader: row.get(3).ok(),
                    url: row.get(4).ok(),
                    checksum: row.get(5).ok(),
                    source: row.get(6)?,
                })
            })?
            .filter_map(Result::ok)
            .collect();
        Ok(rows)
    }
}

/// Seed data for initial population.
///
/// Contains representative mappings, compatibility entries, breaking changes,
/// and recipes for Forge/Fabric/NeoForge across Minecraft versions 1.12.2 – 1.21.
pub fn seed_data() -> Vec<(i32, String)> {
    vec![
        (
            1,
            "Initial schema with mappings, compatibility, breaking_changes, recipes, dependencies, schema_version tables".to_string(),
        ),
    ]
}

/// Populate the database with seed data (task 5.7).
pub fn seed_knowledge(kb: &KnowledgeBase) -> Result<(), DbError> {
    // Mappings
    for m in [
        Mapping {
            version: "1.20.1".into(),
            loader: "fabric".into(),
            from_api: "DemoWidget".into(),
            to_api: "DemoGadget".into(),
            kind: "rename".into(),
            source: "fabric-api".into(),
            confidence: 0.95,
        },
        Mapping {
            version: "1.21".into(),
            loader: "fabric".into(),
            from_api: "DemoWidget".into(),
            to_api: "DemoGadget".into(),
            kind: "rename".into(),
            source: "fabric-api".into(),
            confidence: 0.95,
        },
        Mapping {
            version: "1.20.1".into(),
            loader: "forge".into(),
            from_api: "net.minecraft.block.Block".into(),
            to_api: "net.minecraft.world.level.block.Block".into(),
            kind: "move".into(),
            source: "forge".into(),
            confidence: 0.9,
        },
        Mapping {
            version: "1.21".into(),
            loader: "neoforge".into(),
            from_api: "DemoWidget".into(),
            to_api: "DemoGadget".into(),
            kind: "rename".into(),
            source: "neoforge".into(),
            confidence: 0.9,
        },
    ] {
        kb.insert_mapping(&m)?;
    }

    // Compatibility matrix
    for c in [
        Compatibility {
            minecraft_version: "1.12.2".into(),
            loader: "forge".into(),
            loader_version: "14.23.5".into(),
            status: "stable".into(),
            notes: Some("Legacy Forge".into()),
        },
        Compatibility {
            minecraft_version: "1.16.5".into(),
            loader: "forge".into(),
            loader_version: "36.2".into(),
            status: "stable".into(),
            notes: None,
        },
        Compatibility {
            minecraft_version: "1.18.2".into(),
            loader: "forge".into(),
            loader_version: "40.2".into(),
            status: "stable".into(),
            notes: None,
        },
        Compatibility {
            minecraft_version: "1.20.1".into(),
            loader: "forge".into(),
            loader_version: "47.2".into(),
            status: "stable".into(),
            notes: None,
        },
        Compatibility {
            minecraft_version: "1.21".into(),
            loader: "neoforge".into(),
            loader_version: "21.0".into(),
            status: "stable".into(),
            notes: None,
        },
        Compatibility {
            minecraft_version: "1.12.2".into(),
            loader: "fabric".into(),
            loader_version: "0.14".into(),
            status: "stable".into(),
            notes: Some("Legacy Fabric".into()),
        },
        Compatibility {
            minecraft_version: "1.16.5".into(),
            loader: "fabric".into(),
            loader_version: "0.33".into(),
            status: "stable".into(),
            notes: None,
        },
        Compatibility {
            minecraft_version: "1.18.2".into(),
            loader: "fabric".into(),
            loader_version: "0.56".into(),
            status: "stable".into(),
            notes: None,
        },
        Compatibility {
            minecraft_version: "1.20.1".into(),
            loader: "fabric".into(),
            loader_version: "0.88".into(),
            status: "stable".into(),
            notes: None,
        },
        Compatibility {
            minecraft_version: "1.21".into(),
            loader: "fabric".into(),
            loader_version: "0.100".into(),
            status: "stable".into(),
            notes: None,
        },
    ] {
        kb.insert_compatibility(&c)?;
    }

    // Breaking changes
    for b in [
        BreakingChange {
            version: "1.17".into(),
            change: "Flattening: block/item IDs changed".into(),
            severity: "high".into(),
            workaround: Some("Use DataFixerUpper".into()),
            source: "mojang".into(),
        },
        BreakingChange {
            version: "1.18".into(),
            change: "World height/build limit changed".into(),
            severity: "high".into(),
            workaround: Some("Update chunk handling".into()),
            source: "mojang".into(),
        },
        BreakingChange {
            version: "1.20.1".into(),
            change: "DemoWidget -> DemoGadget rename (illustrative sample)".into(),
            severity: "medium".into(),
            workaround: Some("Rename DemoWidget to DemoGadget".into()),
            source: "fabric-api".into(),
        },
        BreakingChange {
            version: "1.20.5".into(),
            change: "Data components replace NBT for items".into(),
            severity: "high".into(),
            workaround: Some("Migrate to DataComponent API".into()),
            source: "mojang".into(),
        },
    ] {
        kb.insert_breaking_change(&b)?;
    }

    // Recipes
    for r in [
        Recipe {
            recipe_id: "fabric-block-rename".into(),
            version: "1.20.1+".into(),
            loader: Some("fabric".into()),
            description: "Rename DemoWidget to DemoGadget (illustrative sample)".into(),
            steps: vec![
                Value::String("search: Block".into()),
                Value::String("replace: DemoGadget".into()),
            ],
            source: "fabric-api".into(),
            tested: true,
            recipe_version: 1,
            parent_recipe_id: None,
        },
        Recipe {
            recipe_id: "forge-block-move".into(),
            version: "1.17+".into(),
            loader: Some("forge".into()),
            description: "Update Block import path for Forge 1.17+".into(),
            steps: vec![
                Value::String("import: net.minecraft.block.Block".into()),
                Value::String("import: net.minecraft.world.level.block.Block".into()),
            ],
            source: "forge".into(),
            tested: true,
            recipe_version: 1,
            parent_recipe_id: None,
        },
        Recipe {
            recipe_id: "data-components-migration".into(),
            version: "1.20.5+".into(),
            loader: None,
            description: "Migrate item NBT to data components".into(),
            steps: vec![
                Value::String("parse NBT".into()),
                Value::String("convert to DataComponent".into()),
                Value::String("write component".into()),
            ],
            source: "mojang".into(),
            tested: false,
            recipe_version: 1,
            parent_recipe_id: None,
        },
    ] {
        kb.insert_recipe(&r)?;
    }

    // Dependencies
    for d in [
        ModDependency {
            mod_id: "fabric-api".into(),
            name: Some("Fabric API".into()),
            version: "0.88.0".into(),
            loader: Some("fabric".into()),
            url: Some("https://modrinth.com/mod/fabric-api".into()),
            checksum: None,
            source: "modrinth".into(),
        },
        ModDependency {
            mod_id: "cloth-config".into(),
            name: Some("Cloth Config".into()),
            version: "10.0".into(),
            loader: Some("fabric".into()),
            url: Some("https://modrinth.com/mod/cloth-config".into()),
            checksum: None,
            source: "modrinth".into(),
        },
    ] {
        kb.insert_dependency(&d)?;
    }

    kb.record_version(1, "Seed data loaded")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    /// Returns the `TempDir` guard together with the KB: dropping the guard
    /// deletes the directory, which makes SQLite fail with READONLY_DBMOVED.
    fn setup_kb() -> (tempfile::TempDir, KnowledgeBase) {
        let dir = tempdir().unwrap();
        let kb = KnowledgeBase::open(&dir.path().join("test.db")).unwrap();
        (dir, kb)
    }

    #[test]
    fn rule_change_invalidates_cache() {
        use crate::cache::{CacheKey, CacheKind};
        let (_dir, kb) = setup_kb();
        let key = CacheKey::new(CacheKind::Analysis, b"jar bytes", "rules-a".into());
        assert!(kb.cache_put(&key, &vec!["cached".to_string()]).unwrap());
        let value: Option<Vec<String>> = kb.cache_get(&key).unwrap();
        assert_eq!(value, Some(vec!["cached".to_string()]));
        let changed_rules = CacheKey {
            rules_sha256: "rules-b".into(),
            ..key
        };
        let changed_content = CacheKey::new(CacheKind::Analysis, b"other bytes", "rules-a".into());
        assert!(kb
            .cache_get::<Vec<String>>(&changed_rules)
            .unwrap()
            .is_none());
        assert!(kb
            .cache_get::<Vec<String>>(&changed_content)
            .unwrap()
            .is_none());
    }

    #[test]
    fn ummp_version_change_invalidates_cache() {
        use crate::cache::{CacheKey, CacheKind};
        let (_dir, kb) = setup_kb();
        let key = CacheKey::new(CacheKind::Analysis, b"jar bytes", "rules".into());
        kb.cache_put(&key, &serde_json::json!({"value": 1}))
            .unwrap();
        let old_version = CacheKey {
            ummp_version: "0.0.0".into(),
            ..key
        };
        assert!(kb
            .cache_get::<serde_json::Value>(&old_version)
            .unwrap()
            .is_none());
    }

    #[test]
    fn corrupt_cache_row_is_deleted_and_rebuilt() {
        use crate::cache::{CacheKey, CacheKind};
        let (_dir, kb) = setup_kb();
        let key = CacheKey::new(CacheKind::Scan, b"jar bytes", "rules".into());
        kb.conn.execute(
            "INSERT INTO analysis_cache (cache_kind, content_sha256, ummp_version, rules_sha256, payload)
             VALUES (?1, ?2, ?3, ?4, 'not-json')",
            params![key.kind.as_str(), key.content_sha256, key.ummp_version, key.rules_sha256],
        ).unwrap();
        assert!(kb.cache_get::<serde_json::Value>(&key).unwrap().is_none());
        assert!(kb
            .cache_put(&key, &serde_json::json!({"rebuilt": true}))
            .unwrap());
        let value: Option<serde_json::Value> = kb.cache_get(&key).unwrap();
        assert_eq!(value, Some(serde_json::json!({"rebuilt": true})));
    }

    #[test]
    fn cache_table_is_added_to_existing_knowledge_database() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("existing.db");
        {
            let conn = Connection::open(&path).unwrap();
            conn.execute_batch(SCHEMA).unwrap();
        }
        let kb = KnowledgeBase::open(&path).unwrap();
        let count: i64 = kb
            .conn
            .query_row(
                "SELECT count(*) FROM sqlite_master
                 WHERE type = 'table' AND name = 'analysis_cache'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn test_schema_and_open() {
        let (_kb_dir, kb) = setup_kb();
        // tables exist
        let count: i64 = kb
            .conn
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE type='table'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(count >= 6); // schema_version + 5 data tables
    }

    #[test]
    fn test_mapping_crud() {
        let (_kb_dir, kb) = setup_kb();
        let m = Mapping {
            version: "1.20.1".into(),
            loader: "fabric".into(),
            from_api: "Test".into(),
            to_api: "Test2".into(),
            kind: "rename".into(),
            source: "test".into(),
            confidence: 0.8,
        };
        kb.insert_mapping(&m).unwrap();
        let res = kb.mappings_by_loader("fabric").unwrap();
        assert_eq!(res.len(), 1);
        assert_eq!(res[0].from_api, "Test");
    }

    #[test]
    fn test_compatibility_crud() {
        let (_kb_dir, kb) = setup_kb();
        let c = Compatibility {
            minecraft_version: "1.20.1".into(),
            loader: "fabric".into(),
            loader_version: "0.88".into(),
            status: "stable".into(),
            notes: None,
        };
        kb.insert_compatibility(&c).unwrap();
        let res = kb.compatibilities_by_loader("fabric").unwrap();
        assert_eq!(res.len(), 1);
    }

    #[test]
    fn test_breaking_change_crud() {
        let (_kb_dir, kb) = setup_kb();
        let b = BreakingChange {
            version: "1.21".into(),
            change: "Test change".into(),
            severity: "low".into(),
            workaround: Some("none".into()),
            source: "test".into(),
        };
        kb.insert_breaking_change(&b).unwrap();
        let res = kb.breaking_changes_by_version("1.21").unwrap();
        assert_eq!(res.len(), 1);
    }

    #[test]
    fn test_recipe_crud_and_versioning() {
        let (_kb_dir, kb) = setup_kb();
        let r = Recipe {
            recipe_id: "test-recipe".into(),
            version: "1.20.1".into(),
            loader: Some("fabric".into()),
            description: "Test".into(),
            steps: vec![Value::String("step1".into())],
            source: "test".into(),
            tested: false,
            recipe_version: 1,
            parent_recipe_id: None,
        };
        kb.insert_recipe(&r).unwrap();
        let fetched = kb.recipe("test-recipe").unwrap().unwrap();
        assert_eq!(fetched.recipe_id, "test-recipe");
        assert_eq!(fetched.recipe_version, 1);

        // versioning
        let v2 = kb
            .version_recipe("test-recipe", "test-recipe-v2", "Test v2")
            .unwrap();
        assert_eq!(v2.recipe_version, 2);
        assert_eq!(v2.parent_recipe_id, Some("test-recipe".into()));

        // delete
        assert!(kb.delete_recipe("test-recipe").unwrap());
        assert!(kb.recipe("test-recipe").unwrap().is_none());
    }

    #[test]
    fn test_dependency_crud_and_compat() {
        let (_kb_dir, kb) = setup_kb();
        let d = ModDependency {
            mod_id: "test-mod".into(),
            name: Some("Test Mod".into()),
            version: "1.0".into(),
            loader: Some("fabric".into()),
            url: None,
            checksum: None,
            source: "test".into(),
        };
        kb.insert_dependency(&d).unwrap();
        let res = kb.dependencies_by_mod_id("test-mod").unwrap();
        assert_eq!(res.len(), 1);

        let missing = kb
            .check_dependency_compatibility("test-mod", &["other-mod".into()])
            .unwrap();
        assert_eq!(missing.len(), 1);
        assert_eq!(missing[0], "test-mod");
    }

    #[test]
    fn test_import_export() {
        let (_kb_dir, kb) = setup_kb();
        let dir = tempdir().unwrap();
        let path = dir.path().join("mappings.json");

        let m = Mapping {
            version: "1.20.1".into(),
            loader: "fabric".into(),
            from_api: "A".into(),
            to_api: "B".into(),
            kind: "rename".into(),
            source: "test".into(),
            confidence: 0.9,
        };
        kb.insert_mapping(&m).unwrap();

        kb.export_mappings_to_file(&path).unwrap();
        let (_kb2_dir, kb2) = setup_kb();
        kb2.import_mappings_from_file(&path).unwrap();
        let imported = kb2.mappings_by_loader("fabric").unwrap();
        assert_eq!(imported.len(), 1);
        assert_eq!(imported[0].from_api, "A");
    }

    #[test]
    fn test_search() {
        let (_kb_dir, kb) = setup_kb();
        let m = Mapping {
            version: "1.20.1".into(),
            loader: "fabric".into(),
            from_api: "DemoWidget".into(),
            to_api: "DemoGadget".into(),
            kind: "rename".into(),
            source: "test".into(),
            confidence: 0.9,
        };
        kb.insert_mapping(&m).unwrap();
        let res = kb.search_mappings("DemoWidget").unwrap();
        assert_eq!(res.len(), 1);
    }

    #[test]
    fn test_seed_knowledge() {
        let (_kb_dir, kb) = setup_kb();
        seed_knowledge(&kb).unwrap();
        let mappings = kb.mappings_by_loader("fabric").unwrap();
        assert_ne!(mappings.len(), 0, "seed produced no mappings");
        let compat = kb.compatibilities_by_loader("fabric").unwrap();
        assert_ne!(compat.len(), 0, "seed produced no compat");
        let breaks = kb.breaking_changes_by_version("1.17").unwrap();
        assert_ne!(breaks.len(), 0, "seed produced no breaks");
        let recipes = kb.recipes_by_loader("fabric").unwrap();
        assert_ne!(recipes.len(), 0, "seed produced no recipes");
        let deps = kb.dependencies_by_mod_id("fabric-api").unwrap();
        assert_ne!(deps.len(), 0, "seed produced no deps");
    }

    #[test]
    fn test_apply_migrations() {
        let (_kb_dir, kb) = setup_kb();
        let migrations = vec![
            (
                2,
                "Add index",
                "CREATE INDEX IF NOT EXISTS idx_test ON mappings(version);",
            ),
            (3, "Add column", "ALTER TABLE recipes ADD COLUMN tags TEXT;"),
        ];
        let applied = kb.apply_migrations(&migrations).unwrap();
        assert_eq!(applied, 2);
        // second call should apply 0
        let applied2 = kb.apply_migrations(&migrations).unwrap();
        assert_eq!(applied2, 0);
    }
}
