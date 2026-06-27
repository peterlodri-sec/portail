use crate::storage::{DocChunk, PackageDb, SearchResult};
use anyhow::Result;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

const MAX_MEMORY_BYTES: usize = 100_000_000;

pub struct PkgCtxMemory {
    db: Arc<Mutex<PackageDb>>,
    save_path: Option<PathBuf>,
    chunk_count: usize,
}

impl PkgCtxMemory {
    pub fn new() -> Result<Self> {
        let db = PackageDb::create_in_memory()?;
        Ok(Self {
            db: Arc::new(Mutex::new(db)),
            save_path: None,
            chunk_count: 0,
        })
    }

    pub fn load_or_create(pkg_dir: &Path) -> Result<Self> {
        let save_path = pkg_dir.join("pkg-ctx-memory.db");
        let (db, chunk_count) = if save_path.exists() {
            let file_size = std::fs::metadata(&save_path)?.len() as usize;
            if file_size > MAX_MEMORY_BYTES {
                (PackageDb::create_in_memory()?, 0)
            } else {
                let opened = PackageDb::open(save_path.to_str().unwrap())?;
                let count = opened.chunk_count()? as usize;
                (opened, count)
            }
        } else {
            (PackageDb::create_in_memory()?, 0)
        };

        Ok(Self {
            db: Arc::new(Mutex::new(db)),
            save_path: Some(save_path),
            chunk_count,
        })
    }

    pub fn insert_sync(&mut self, chunk: DocChunk) -> Result<()> {
        let memory_used = estimate_memory(&chunk);
        if memory_used > MAX_MEMORY_BYTES {
            anyhow::bail!("pkg-ctx memory full (max 100MB)");
        }
        {
            let db = self.db.lock().unwrap();
            db.insert_chunk(&chunk)?;
        }
        self.chunk_count += 1;
        Ok(())
    }

    pub async fn insert(&mut self, chunk: DocChunk) -> Result<()> {
        let memory_used = estimate_memory(&chunk);
        if memory_used > MAX_MEMORY_BYTES {
            anyhow::bail!("pkg-ctx memory full (max 100MB)");
        }
        let db = Arc::clone(&self.db);
        tokio::task::spawn_blocking(move || -> Result<()> {
            let guard = db.lock().unwrap();
            guard.insert_chunk(&chunk).map_err(|e| anyhow::anyhow!(e))?;
            Ok(())
        })
        .await??;
        self.chunk_count += 1;
        Ok(())
    }

    pub fn search_sync(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>> {
        let db = self.db.lock().unwrap();
        Ok(db.search(query, limit)?)
    }

    pub async fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>> {
        let query = query.to_string();
        let db = Arc::clone(&self.db);
        tokio::task::spawn_blocking(move || -> Result<Vec<SearchResult>> {
            let guard = db.lock().unwrap();
            guard.search(&query, limit).map_err(|e| anyhow::anyhow!(e.to_string()))
        })
        .await?
    }

    pub fn contains(&self, doc_path: &str) -> bool {
        let db = self.db.lock().unwrap();
        let results = db.search(doc_path, 1).unwrap_or_default();
        !results.is_empty()
    }

    pub fn chunk_count(&self) -> usize {
        self.chunk_count
    }

    pub fn size_estimate(&self) -> String {
        format!(
            "pkg-ctx memory: {} chunks, ~{}KB / 100MB max",
            self.chunk_count,
            self.chunk_count.saturating_mul(2),
        )
    }

    pub fn save_sync(&mut self) -> Result<()> {
        if let Some(ref path) = self.save_path.clone() {
            let mem_db = self.db.lock().unwrap();
            Self::backup_to_path(&mem_db, path)?;
        }
        Ok(())
    }

    pub async fn save(&mut self) -> Result<()> {
        if let Some(ref path) = self.save_path.clone() {
            let db = Arc::clone(&self.db);
            let path = path.to_string_lossy().to_string();
            tokio::task::spawn_blocking(move || -> Result<()> {
                let mem_db = db.lock().unwrap();
                Self::backup_to_path(&mem_db, Path::new(&path))
            })
            .await??;
        }
        Ok(())
    }

    pub async fn save_to(&mut self, path: &Path) -> Result<()> {
        let db = Arc::clone(&self.db);
        let path = path.to_string_lossy().to_string();
        tokio::task::spawn_blocking(move || -> Result<()> {
            let mem_db = db.lock().unwrap();
            Self::backup_to_path(&mem_db, Path::new(&path))
        })
        .await??;
        Ok(())
    }

    fn backup_to_path(mem_db: &PackageDb, path: &Path) -> Result<()> {
        let mut file_db = PackageDb::open(path.to_str().unwrap())?;
        let backup = rusqlite::backup::Backup::new(&mem_db.conn, &mut file_db.conn)?;
        backup.run_to_completion(5, std::time::Duration::from_millis(100), None)?;
        Ok(())
    }

    pub fn summarize(&self) -> String {
        format!(
            "pkg-ctx memory: {} chunks loaded, max 100MB\n\
             Use `portail pkg-ctx add <repo>` to add docs\n\
             Use `portail pkg-ctx search <lib> <query>` to query\n\
             Use `portail pkg-ctx serve` for MCP integration",
            self.chunk_count,
        )
    }
}

impl Drop for PkgCtxMemory {
    fn drop(&mut self) {
        if self.save_path.is_some() && self.chunk_count > 0 {
            let _ = self.save_sync();
        }
    }
}

fn estimate_memory(chunk: &DocChunk) -> usize {
    chunk.doc_path.len()
        + chunk.doc_title.len()
        + chunk.section_title.len()
        + chunk.content.len()
        + 64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::DocChunk;

    #[test]
    fn test_memory_create() {
        let mem = PkgCtxMemory::new().unwrap();
        assert_eq!(mem.chunk_count(), 0);
    }

    #[tokio::test]
    async fn test_memory_insert_and_search() {
        let mut mem = PkgCtxMemory::new().unwrap();
        mem.insert(DocChunk {
            id: 0,
            doc_path: "quickstart.md".into(),
            doc_title: "Quickstart".into(),
            section_title: "Installation".into(),
            content: "Install portail with cargo install portail".into(),
            tokens: 8,
            has_code: true,
        })
        .await
        .unwrap();
        assert_eq!(mem.chunk_count(), 1);

        let results = mem.search("install", 10).await.unwrap();
        assert_eq!(results.len(), 1);
    }

    #[tokio::test]
    async fn test_memory_save_and_load() {
        let dir = std::env::temp_dir().join("pkg-ctx-test");
        std::fs::create_dir_all(&dir).unwrap();

        let mut mem = PkgCtxMemory::load_or_create(&dir).unwrap();
        mem.insert(DocChunk {
            id: 0,
            doc_path: "test.md".into(),
            doc_title: "Test".into(),
            section_title: "Section".into(),
            content: "test content here".into(),
            tokens: 4,
            has_code: false,
        })
        .await
        .unwrap();

        mem.save().await.unwrap();

        let loaded = PkgCtxMemory::load_or_create(&dir).unwrap();
        assert_eq!(loaded.chunk_count(), 1);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_summarize_format() {
        let mem = PkgCtxMemory::new().unwrap();
        let s = mem.summarize();
        assert!(s.contains("pkg-ctx memory"));
        assert!(s.contains("100MB"));
    }
}
