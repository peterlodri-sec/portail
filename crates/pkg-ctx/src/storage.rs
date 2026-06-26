use rusqlite::{params, Connection, Result as SqlResult};

#[derive(Debug, Clone)]
pub struct DocChunk {
    pub id: i64,
    pub doc_path: String,
    pub doc_title: String,
    pub section_title: String,
    pub content: String,
    pub tokens: i64,
    pub has_code: bool,
}

#[derive(Debug, Clone)]
pub struct SearchResult {
    pub rank: f64,
    pub doc_path: String,
    pub doc_title: String,
    pub section_title: String,
    pub content: String,
    pub tokens: i64,
}

pub struct PackageDb {
    pub conn: Connection,
}

impl PackageDb {
    pub fn open(path: &str) -> SqlResult<Self> {
        let conn = Connection::open(path)?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;")?;
        let db = Self { conn };
        db.init_tables()?;
        Ok(db)
    }

    pub fn open_with_conn(conn: Connection) -> SqlResult<Self> {
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;")?;
        let db = Self { conn };
        db.init_tables()?;
        Ok(db)
    }

    pub fn create_in_memory() -> SqlResult<Self> {
        let conn = Connection::open_in_memory()?;
        let db = Self { conn };
        db.init_tables()?;
        Ok(db)
    }

    fn init_tables(&self) -> SqlResult<()> {
        self.conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS chunks (
                id INTEGER PRIMARY KEY,
                doc_path TEXT NOT NULL,
                doc_title TEXT NOT NULL,
                section_title TEXT NOT NULL,
                content TEXT NOT NULL,
                tokens INTEGER NOT NULL,
                has_code INTEGER DEFAULT 0
            );

            CREATE TABLE IF NOT EXISTS pkg_meta (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );

            CREATE VIRTUAL TABLE IF NOT EXISTS chunks_fts USING fts5(
                doc_title, section_title, content,
                tokenize='porter unicode61'
            );",
        )?;
        Ok(())
    }

    pub fn insert_chunk(&self, chunk: &DocChunk) -> SqlResult<i64> {
        self.conn.execute(
            "INSERT INTO chunks (doc_path, doc_title, section_title, content, tokens, has_code)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                chunk.doc_path,
                chunk.doc_title,
                chunk.section_title,
                chunk.content,
                chunk.tokens,
                chunk.has_code as i64,
            ],
        )?;
        let chunk_id = self.conn.last_insert_rowid();
        self.conn.execute(
            "INSERT INTO chunks_fts (rowid, doc_title, section_title, content)
             VALUES (?1, ?2, ?3, ?4)",
            params![chunk_id, chunk.doc_title, chunk.section_title, chunk.content],
        )?;
        Ok(chunk_id)
    }

    pub fn search(&self, query: &str, limit: usize) -> SqlResult<Vec<SearchResult>> {
        let limit = limit.min(20) as i64;
        let terms: Vec<&str> = query.split_whitespace().filter(|w| w.len() > 1).collect();
        if terms.is_empty() {
            return Ok(Vec::new());
        }
        let fts_query = terms.join(" ");

        let mut stmt = self.conn.prepare(
            "SELECT c.doc_path, c.doc_title, c.section_title, c.content, c.tokens
             FROM chunks_fts
             JOIN chunks c ON chunks_fts.rowid = c.id
             WHERE chunks_fts MATCH ?1
             LIMIT ?2",
        )?;

        let results = stmt
            .query_map(params![fts_query, limit], |row| {
                Ok(SearchResult {
                    rank: 0.0,
                    doc_path: row.get(0)?,
                    doc_title: row.get(1)?,
                    section_title: row.get(2)?,
                    content: row.get(3)?,
                    tokens: row.get(4)?,
                })
            })?
            .collect::<SqlResult<Vec<_>>>()?;

        Ok(results)
    }

    pub fn set_meta(&self, key: &str, value: &str) -> SqlResult<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO pkg_meta (key, value) VALUES (?1, ?2)",
            params![key, value],
        )?;
        Ok(())
    }

    pub fn get_meta(&self, key: &str) -> SqlResult<Option<String>> {
        let mut stmt = self.conn.prepare("SELECT value FROM pkg_meta WHERE key = ?1")?;
        let mut rows = stmt.query(params![key])?;
        match rows.next()? {
            Some(row) => Ok(Some(row.get(0)?)),
            None => Ok(None),
        }
    }

    pub fn chunk_count(&self) -> SqlResult<i64> {
        self.conn
            .query_row("SELECT COUNT(*) FROM chunks", [], |row| row.get(0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_in_memory() {
        let db = PackageDb::create_in_memory().unwrap();
        assert!(db.chunk_count().unwrap() == 0);
    }

    #[test]
    fn test_insert_and_search() {
        let db = PackageDb::create_in_memory().unwrap();
        db.insert_chunk(&DocChunk {
            id: 0,
            doc_path: "middleware.md".into(),
            doc_title: "Middleware".into(),
            section_title: "Auth Middleware".into(),
            content: "Authentication middleware verifies JWT tokens and sets request context.".into(),
            tokens: 15,
            has_code: false,
        })
        .unwrap();
        db.insert_chunk(&DocChunk {
            id: 0,
            doc_path: "routes.md".into(),
            doc_title: "Routes".into(),
            section_title: "API Routes".into(),
            content: "API route handlers for the v1 endpoints.".into(),
            tokens: 10,
            has_code: false,
        })
        .unwrap();

        let results = db.search("authentication", 10).unwrap();
        assert_eq!(results.len(), 1);
        assert!(results[0].content.contains("Authentication"));
    }

    #[test]
    fn test_meta_roundtrip() {
        let db = PackageDb::create_in_memory().unwrap();
        db.set_meta("name", "test-pkg").unwrap();
        assert_eq!(db.get_meta("name").unwrap(), Some("test-pkg".into()));
    }
}
