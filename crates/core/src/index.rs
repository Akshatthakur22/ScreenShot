use std::path::{Path, PathBuf};

use anyhow::{Result, bail};
use rusqlite::{Connection, OptionalExtension, params, params_from_iter, types::Value};

use crate::{Line, Rect, Shot};

// Bump this with a preserving migration. Unknown versions are rejected rather
// than reset so an application upgrade cannot silently erase indexed data.
const SCHEMA: i32 = 1;

// The trigram tokenizer indexes every 3 character window, which is what makes
// "nutsmp" find "donutsmp.net". The flip side is that it cannot match anything
// shorter than 3 characters, see `search`.
const TABLES: &str = "
    CREATE TABLE shots (
        id     INTEGER PRIMARY KEY,
        path   TEXT NOT NULL UNIQUE,
        mtime  INTEGER NOT NULL,
        width  INTEGER NOT NULL,
        height INTEGER NOT NULL
    );
    CREATE TABLE lines (
        id      INTEGER PRIMARY KEY,
        shot_id INTEGER NOT NULL REFERENCES shots(id) ON DELETE CASCADE,
        text    TEXT NOT NULL,
        x REAL NOT NULL, y REAL NOT NULL, w REAL NOT NULL, h REAL NOT NULL,
        score   REAL NOT NULL
    );
    CREATE INDEX lines_by_shot ON lines(shot_id);
    CREATE VIRTUAL TABLE shots_fts USING fts5(text, tokenize = 'trigram');
";

pub struct Index {
    db: Connection,
}

#[derive(Debug, Clone)]
pub struct Hit {
    pub id: i64,
    pub path: PathBuf,
    pub mtime: i64,
    pub width: u32,
    pub height: u32,
    /// Only the lines that contain a query term, so the UI can draw a box
    /// around each one. Empty when browsing without a query.
    pub lines: Vec<Line>,
}

impl Index {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        Self::init(Connection::open(path)?)
    }

    pub fn open_default() -> Result<Self> {
        Self::open(&crate::data_dir()?.join("index.db"))
    }

    pub fn open_in_memory() -> Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(mut db: Connection) -> Result<Self> {
        // WAL so the watcher can write while the app is reading.
        db.pragma_update(None, "journal_mode", "WAL")?;
        db.pragma_update(None, "synchronous", "NORMAL")?;
        db.pragma_update(None, "foreign_keys", true)?;

        let version: i32 = db.pragma_query_value(None, "user_version", |r| r.get(0))?;
        if version == 0 {
            let objects: i64 = db.query_row(
                "SELECT count(*) FROM sqlite_master WHERE type IN ('table', 'view') AND name NOT LIKE 'sqlite_%'",
                [],
                |row| row.get(0),
            )?;
            if objects != 0 {
                bail!("unversioned database contains data; refusing to replace it");
            }
            let tx = db.transaction()?;
            tx.execute_batch(TABLES)?;
            tx.execute_batch("PRAGMA user_version = 1;")?;
            tx.commit()?;
        } else if version != SCHEMA {
            bail!("unsupported index schema version {version}; refusing destructive reset");
        }
        Ok(Self { db })
    }

    /// True if this exact file (same path, same mtime) is already indexed.
    pub fn is_current(&self, path: &Path, mtime: i64) -> Result<bool> {
        let found = self
            .db
            .query_row(
                "SELECT 1 FROM shots WHERE path = ?1 AND mtime = ?2",
                params![path_str(path), mtime],
                |_| Ok(()),
            )
            .optional()?;
        Ok(found.is_some())
    }

    /// Whether this path is indexed as a real, showable screenshot.
    pub fn is_visible(&self, path: &Path) -> Result<bool> {
        let found = self
            .db
            .query_row(
                "SELECT 1 FROM shots WHERE path = ?1 AND width > 0",
                [path_str(path)],
                |_| Ok(()),
            )
            .optional()?;
        Ok(found.is_some())
    }

    /// Stores a shot and its lines, replacing whatever was there for the same path.
    pub fn insert(&mut self, shot: &Shot, lines: &[Line]) -> Result<i64> {
        let tx = self.db.transaction()?;
        let path = path_str(&shot.path);
        delete(&tx, &path)?;

        tx.execute(
            "INSERT INTO shots (path, mtime, width, height) VALUES (?1, ?2, ?3, ?4)",
            params![path, shot.mtime, shot.width, shot.height],
        )?;
        let id = tx.last_insert_rowid();

        {
            let mut add = tx.prepare(
                "INSERT INTO lines (shot_id, text, x, y, w, h, score)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            )?;
            for l in lines {
                let r = l.rect;
                add.execute(params![id, l.text, r.x, r.y, r.w, r.h, l.score])?;
            }
        }

        // One fts row per screenshot, not per line, so a query like
        // "invoice march" matches even when the two words sit on different lines.
        let text = lines
            .iter()
            .map(|l| l.text.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        tx.execute(
            "INSERT INTO shots_fts (rowid, text) VALUES (?1, ?2)",
            params![id, text],
        )?;

        tx.commit()?;
        Ok(id)
    }

    /// Moves a shot to a new path without touching its text, since a rename
    /// doesn't change a single pixel. Returns false if `from` wasn't indexed.
    pub fn rename(&mut self, from: &Path, to: &Path) -> Result<bool> {
        let tx = self.db.transaction()?;
        delete(&tx, &path_str(to))?;
        let moved = tx.execute(
            "UPDATE shots SET path = ?2 WHERE path = ?1",
            params![path_str(from), path_str(to)],
        )?;
        tx.commit()?;
        Ok(moved > 0)
    }

    /// Returns whether there was anything to remove.
    pub fn remove(&mut self, path: &Path) -> Result<bool> {
        let tx = self.db.transaction()?;
        let removed = delete(&tx, &path_str(path))?;
        tx.commit()?;
        Ok(removed)
    }

    /// Drops every shot whose file is gone, returns how many.
    pub fn prune(&mut self) -> Result<usize> {
        let gone: Vec<PathBuf> = self.paths()?.into_iter().filter(|p| !p.exists()).collect();
        for p in &gone {
            self.remove(p)?;
        }
        Ok(gone.len())
    }

    pub fn paths(&self) -> Result<Vec<PathBuf>> {
        let mut stmt = self.db.prepare("SELECT path FROM shots")?;
        let paths = stmt
            .query_map([], |r| r.get::<_, String>(0))?
            .map(|p| p.map(PathBuf::from))
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(paths)
    }

    pub fn len(&self) -> Result<usize> {
        let n: i64 = self
            .db
            .query_row("SELECT count(*) FROM shots", [], |r| r.get(0))?;
        Ok(n as usize)
    }

    /// Everything in `shots` has been looked at, but only these can be shown.
    pub fn visible_len(&self) -> Result<usize> {
        let n: i64 = self
            .db
            .query_row("SELECT count(*) FROM shots WHERE width > 0", [], |r| {
                r.get(0)
            })?;
        Ok(n as usize)
    }

    /// Counts OCR lines stored for all indexed screenshots.
    pub fn line_len(&self) -> Result<usize> {
        let n: i64 = self
            .db
            .query_row("SELECT count(*) FROM lines", [], |r| r.get(0))?;
        Ok(n as usize)
    }

    /// Returns one indexed screenshot by its stable database ID.
    pub fn get(&self, id: i64) -> Result<Option<Hit>> {
        let hit = self
            .db
            .query_row(
                "SELECT id, path, mtime, width, height FROM shots WHERE id = ?1 AND width > 0",
                [id],
                |r| {
                    Ok(Hit {
                        id: r.get(0)?,
                        path: PathBuf::from(r.get::<_, String>(1)?),
                        mtime: r.get(2)?,
                        width: r.get(3)?,
                        height: r.get(4)?,
                        lines: Vec::new(),
                    })
                },
            )
            .optional()?;
        Ok(hit)
    }

    pub fn is_empty(&self) -> Result<bool> {
        Ok(self.len()? == 0)
    }

    /// Every whitespace separated term has to appear somewhere in the shot.
    /// An empty query returns the newest shots, which is what the app shows
    /// before you type anything. Each hit comes with the lines that matched.
    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<Hit>> {
        let mut hits = self.find(query, limit)?;
        for hit in &mut hits {
            hit.lines = self.matching_lines(hit.id, query)?;
        }
        Ok(hits)
    }

    /// Same as `search` but without the lines. Fetching those is most of the
    /// cost of a broad query (two letters can match 2000 shots), and a grid
    /// only ever shows a few dozen at a time, so the app asks per tile.
    pub fn find(&self, query: &str, limit: usize) -> Result<Vec<Hit>> {
        let terms: Vec<&str> = query.split_whitespace().collect();
        let (long, short): (Vec<&str>, Vec<&str>) =
            terms.iter().partition(|t| t.chars().count() >= 3);

        let mut sql = String::from(
            "SELECT s.id, s.path, s.mtime, s.width, s.height
             FROM shots_fts f JOIN shots s ON s.id = f.rowid WHERE s.width > 0",
        );
        let mut args: Vec<Value> = Vec::new();

        if !long.is_empty() {
            sql.push_str(" AND shots_fts MATCH ?");
            args.push(Value::Text(match_expr(&long)));
        }
        // Terms under 3 characters are invisible to the trigram index, so they
        // fall back to LIKE. That is a scan, but only over rows the MATCH above
        // already narrowed down, or over everything for a query like "ip".
        for t in &short {
            sql.push_str(" AND f.text LIKE ? ESCAPE '\\'");
            args.push(Value::Text(format!("%{}%", escape_like(t))));
        }

        sql.push_str(if long.is_empty() {
            " ORDER BY s.mtime DESC"
        } else {
            " ORDER BY f.rank, s.mtime DESC"
        });
        sql.push_str(" LIMIT ?");
        args.push(Value::Integer(limit as i64));

        let mut stmt = self.db.prepare_cached(&sql)?;
        let hits = stmt
            .query_map(params_from_iter(args), |r| {
                Ok(Hit {
                    id: r.get(0)?,
                    path: PathBuf::from(r.get::<_, String>(1)?),
                    mtime: r.get(2)?,
                    width: r.get(3)?,
                    height: r.get(4)?,
                    lines: Vec::new(),
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(hits)
    }

    /// The lines of one shot that contain any of the query's terms.
    pub fn matching_lines(&self, shot_id: i64, query: &str) -> Result<Vec<Line>> {
        let needles: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
        if needles.is_empty() {
            return Ok(Vec::new());
        }
        let mut lines = self.lines(shot_id)?;
        lines.retain(|l| {
            let text = l.text.to_lowercase();
            needles.iter().any(|n| text.contains(n.as_str()))
        });
        Ok(lines)
    }

    pub fn lines(&self, shot_id: i64) -> Result<Vec<Line>> {
        let mut stmt = self.db.prepare_cached(
            "SELECT text, x, y, w, h, score FROM lines WHERE shot_id = ?1 ORDER BY id",
        )?;
        let lines = stmt
            .query_map([shot_id], |r| {
                Ok(Line {
                    text: r.get(0)?,
                    rect: Rect {
                        x: r.get(1)?,
                        y: r.get(2)?,
                        w: r.get(3)?,
                        h: r.get(4)?,
                    },
                    score: r.get(5)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(lines)
    }
}

fn delete(tx: &rusqlite::Transaction, path: &str) -> Result<bool> {
    let Some(id) = tx
        .query_row("SELECT id FROM shots WHERE path = ?1", [path], |r| {
            r.get::<_, i64>(0)
        })
        .optional()?
    else {
        return Ok(false);
    };
    // The fts table has no foreign key, so it has to be cleaned up by hand.
    tx.execute("DELETE FROM shots_fts WHERE rowid = ?1", [id])?;
    tx.execute("DELETE FROM shots WHERE id = ?1", [id])?;
    Ok(true)
}

// Each term becomes a quoted fts5 string, which turns off the query syntax
// inside it. Without this, typing `c++` or `"` or `NOT` would be a parse error
// or silently mean something else.
fn match_expr(terms: &[&str]) -> String {
    terms
        .iter()
        .map(|t| format!("\"{}\"", t.replace('"', "\"\"")))
        .collect::<Vec<_>>()
        .join(" ")
}

fn escape_like(term: &str) -> String {
    term.replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

fn path_str(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(text: &str, y: f32) -> Line {
        Line {
            text: text.into(),
            rect: Rect {
                x: 0.1,
                y,
                w: 0.5,
                h: 0.03,
            },
            score: 0.9,
        }
    }

    fn shot(path: &str, mtime: i64) -> Shot {
        Shot {
            path: path.into(),
            mtime,
            width: 2568,
            height: 1428,
        }
    }

    fn sample() -> Index {
        let mut idx = Index::open_in_memory().unwrap();
        idx.insert(
            &shot("/shots/youtube.png", 100),
            &[
                line("IP: donutsmp.net", 0.2),
                line("Later in this video...", 0.5),
                line("Subscribe", 0.8),
            ],
        )
        .unwrap();
        idx.insert(
            &shot("/shots/pr.png", 200),
            &[
                line("add extension: clean-keyboard #288", 0.1),
                line("Uses EVIOCGRAB to grab", 0.6),
            ],
        )
        .unwrap();
        idx
    }

    #[test]
    fn unsupported_schema_is_rejected_without_dropping_existing_tables() {
        let path = std::env::temp_dir().join(format!(
            "akshat-schema-guard-{}-{}.db",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        {
            let db = Connection::open(&path).unwrap();
            db.execute_batch("CREATE TABLE preserved(value TEXT); INSERT INTO preserved VALUES ('kept'); PRAGMA user_version = 42;")
                .unwrap();
        }

        assert!(Index::open(&path).is_err());
        let db = Connection::open(&path).unwrap();
        let value: String = db
            .query_row("SELECT value FROM preserved", [], |row| row.get(0))
            .unwrap();
        assert_eq!(value, "kept");
        drop(db);
        let _ = std::fs::remove_file(path);
    }

    fn paths(hits: &[Hit]) -> Vec<&str> {
        hits.iter().map(|h| h.path.to_str().unwrap()).collect()
    }

    #[test]
    fn finds_the_middle_of_a_word() {
        let hits = sample().search("nutsmp", 10).unwrap();
        assert_eq!(paths(&hits), ["/shots/youtube.png"]);
        assert_eq!(hits[0].lines, [line("IP: donutsmp.net", 0.2)]);
    }

    #[test]
    fn is_case_insensitive() {
        assert_eq!(
            paths(&sample().search("eviocgrab", 10).unwrap()),
            ["/shots/pr.png"]
        );
    }

    #[test]
    fn terms_can_be_on_different_lines() {
        let hits = sample().search("subscribe later", 10).unwrap();
        assert_eq!(paths(&hits), ["/shots/youtube.png"]);
        assert_eq!(hits[0].lines.len(), 2);
    }

    #[test]
    fn every_term_must_match() {
        assert!(
            sample()
                .search("subscribe eviocgrab", 10)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn short_terms_fall_back_to_like() {
        assert_eq!(
            paths(&sample().search("ip", 10).unwrap()),
            ["/shots/youtube.png"]
        );
        assert_eq!(
            paths(&sample().search("#2", 10).unwrap()),
            ["/shots/pr.png"]
        );
        assert_eq!(
            paths(&sample().search("ip donut", 10).unwrap()),
            ["/shots/youtube.png"]
        );
    }

    #[test]
    fn query_syntax_is_not_interpreted() {
        let idx = sample();
        for q in ["\"", "NOT", "c++", "a*", "100%", "_", "(", "clean-keyboard"] {
            idx.search(q, 10).unwrap();
        }
        assert_eq!(
            paths(&idx.search("clean-keyboard", 10).unwrap()),
            ["/shots/pr.png"]
        );
        assert!(idx.search("100%", 10).unwrap().is_empty());
    }

    #[test]
    fn empty_query_is_newest_first() {
        let hits = sample().search("   ", 10).unwrap();
        assert_eq!(paths(&hits), ["/shots/pr.png", "/shots/youtube.png"]);
        assert!(hits.iter().all(|h| h.lines.is_empty()));
    }

    #[test]
    fn reinserting_a_path_replaces_it() {
        let mut idx = sample();
        idx.insert(
            &shot("/shots/youtube.png", 300),
            &[line("something else", 0.1)],
        )
        .unwrap();
        assert_eq!(idx.len().unwrap(), 2);
        assert!(idx.search("donutsmp", 10).unwrap().is_empty());
        assert_eq!(
            paths(&idx.search("something", 10).unwrap()),
            ["/shots/youtube.png"]
        );
        assert!(
            idx.is_current(Path::new("/shots/youtube.png"), 300)
                .unwrap()
        );
        assert!(
            !idx.is_current(Path::new("/shots/youtube.png"), 100)
                .unwrap()
        );
    }

    #[test]
    fn a_shot_with_no_text_is_still_indexed() {
        let mut idx = Index::open_in_memory().unwrap();
        idx.insert(&shot("/shots/blank.png", 1), &[]).unwrap();
        assert_eq!(idx.len().unwrap(), 1);
        assert_eq!(idx.search("", 10).unwrap().len(), 1);
        assert!(idx.search("anything", 10).unwrap().is_empty());
    }

    #[test]
    fn removed_shots_stop_matching() {
        let mut idx = sample();
        assert!(idx.remove(Path::new("/shots/youtube.png")).unwrap());
        assert!(!idx.remove(Path::new("/shots/youtube.png")).unwrap());
        assert!(idx.search("donutsmp", 10).unwrap().is_empty());
        assert_eq!(idx.len().unwrap(), 1);
    }

    #[test]
    fn rename_keeps_the_text() {
        let mut idx = sample();
        assert!(
            idx.rename(
                Path::new("/shots/youtube.png"),
                Path::new("/shots/moved.png")
            )
            .unwrap()
        );
        assert_eq!(
            paths(&idx.search("donutsmp", 10).unwrap()),
            ["/shots/moved.png"]
        );
        assert!(idx.is_current(Path::new("/shots/moved.png"), 100).unwrap());
        assert!(
            !idx.rename(Path::new("/shots/youtube.png"), Path::new("/x.png"))
                .unwrap()
        );
    }

    #[test]
    fn rename_onto_an_existing_shot_replaces_it() {
        let mut idx = sample();
        idx.rename(Path::new("/shots/youtube.png"), Path::new("/shots/pr.png"))
            .unwrap();
        assert_eq!(idx.len().unwrap(), 1);
        assert_eq!(
            paths(&idx.search("donutsmp", 10).unwrap()),
            ["/shots/pr.png"]
        );
        assert!(idx.search("eviocgrab", 10).unwrap().is_empty());
    }

    #[test]
    fn prune_drops_files_that_no_longer_exist() {
        let dir = std::env::temp_dir().join(format!("akshat-prune-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let kept = dir.join("kept.png");
        std::fs::write(&kept, b"").unwrap();

        let mut idx = Index::open_in_memory().unwrap();
        idx.insert(
            &Shot {
                path: kept.clone(),
                mtime: 1,
                width: 10,
                height: 10,
            },
            &[],
        )
        .unwrap();
        idx.insert(&shot("/definitely/not/here.png", 1), &[])
            .unwrap();
        assert_eq!(idx.prune().unwrap(), 1);
        assert_eq!(idx.paths().unwrap(), [kept]);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn zero_sized_shots_are_indexed_but_hidden() {
        let mut idx = sample();
        let broken = Shot {
            path: "/shots/broken.png".into(),
            mtime: 999,
            width: 0,
            height: 0,
        };
        idx.insert(&broken, &[line("donutsmp", 0.1)]).unwrap();
        assert!(idx.is_current(Path::new("/shots/broken.png"), 999).unwrap());
        assert_eq!(idx.len().unwrap(), 3);
        assert_eq!(idx.visible_len().unwrap(), 2);
        assert_eq!(
            paths(&idx.search("donutsmp", 10).unwrap()),
            ["/shots/youtube.png"]
        );
        assert_eq!(idx.search("", 10).unwrap().len(), 2);
    }

    #[test]
    fn find_is_search_without_the_lines() {
        let idx = sample();
        let found = idx.find("subscribe later", 10).unwrap();
        assert_eq!(paths(&found), ["/shots/youtube.png"]);
        assert!(found[0].lines.is_empty());
        let lines = idx.matching_lines(found[0].id, "subscribe later").unwrap();
        assert_eq!(lines, idx.search("subscribe later", 10).unwrap()[0].lines);
        assert!(idx.matching_lines(found[0].id, "  ").unwrap().is_empty());
    }

    #[test]
    fn limit_zero_returns_nothing() {
        assert!(sample().search("", 0).unwrap().is_empty());
    }
}
