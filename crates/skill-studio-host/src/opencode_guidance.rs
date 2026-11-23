//! Read `OpenCode` 2.3.2's current session instruction blob, never its original baseline.
use crate::opencode_db::{
    default_opencode_databases, open_opencode_database, table_exists, OPENCODE_DATA_ROOT,
};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use skill_studio_core::skill_sessions::{
    temporary_session_directory, ParsedSession, RecordedEntry,
};
use std::path::{Path, PathBuf};

#[derive(Deserialize)]
struct SkillGuidance {
    id: String,
    name: String,
    description: String,
}

pub(crate) fn read_opencode_guidance(
    home: &Path,
    databases: Option<&[PathBuf]>,
    project: Option<&Path>,
    since: DateTime<Utc>,
    limit: usize,
) -> Vec<ParsedSession> {
    let mut sessions = Vec::new();
    let paths = databases.map_or_else(
        || default_opencode_databases(&home.join(OPENCODE_DATA_ROOT)),
        <[_]>::to_vec,
    );
    for path in paths {
        let Some(db) = open_opencode_database(&path) else {
            continue;
        };
        let source = if table_exists(&db, "session_v2").unwrap_or(false) {
            if table_exists(&db, "instruction_state").unwrap_or(false)
                && table_exists(&db, "instruction_blob").unwrap_or(false)
            {
                r#"s.agent, b.value FROM session_v2 s
                   LEFT JOIN instruction_state i ON i.session_id = s.id
                   LEFT JOIN instruction_blob b ON b.hash = CASE WHEN json_valid(i.current_values)
                       THEN json_extract(i.current_values, '$."core/skill-guidance"') END"#
            } else {
                "NULL, NULL FROM session_v2 s"
            }
        } else {
            "NULL, NULL FROM session s"
        };
        let sql = format!(
            "SELECT s.id, s.directory, s.time_updated, {source}
             WHERE s.time_updated >= ?1 AND (?2 IS NULL OR s.directory = ?2)
             ORDER BY s.time_updated DESC, s.id"
        );
        let Ok(mut query) = db.prepare(&sql) else {
            continue;
        };
        let Ok(rows) = query.query_map(
            rusqlite::params![since.timestamp_millis(), project.and_then(Path::to_str)],
            |row| {
                let blob: Option<String> = row.get(4)?;
                let listing = blob
                    .as_deref()
                    .and_then(|json| serde_json::from_str::<Vec<SkillGuidance>>(json).ok())
                    .map(|entries| {
                        entries
                            .into_iter()
                            .map(|e| {
                                (
                                    e.name,
                                    RecordedEntry::Saved {
                                        text: e.description,
                                        locator: e.id,
                                    },
                                )
                            })
                            .collect()
                    });
                Ok(ParsedSession {
                    id: row.get(0)?,
                    transcript: path.clone(),
                    project_path: row.get(1)?,
                    agent_name: row.get(3)?,
                    at: DateTime::from_timestamp_millis(row.get(2)?).map(|at| at.to_rfc3339()),
                    listing,
                    ..ParsedSession::default()
                })
            },
        ) else {
            continue;
        };
        sessions.extend(
            rows.flatten()
                .filter(|s| {
                    s.project_path
                        .as_deref()
                        .is_none_or(|p| !temporary_session_directory(home, Path::new(p)))
                })
                .take(limit),
        );
    }
    sessions.sort_by(|a, b| b.at.cmp(&a.at).then_with(|| a.id.cmp(&b.id)));
    sessions.truncate(limit);
    sessions
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reads_each_agents_current_blob_and_leaves_missing_guidance_unknown() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join(OPENCODE_DATA_ROOT);
        std::fs::create_dir_all(&root).unwrap();
        let db = rusqlite::Connection::open(root.join("opencode.db")).unwrap();
        db.execute_batch(r#"
            CREATE TABLE session_v2(id TEXT, directory TEXT, agent TEXT, time_updated INTEGER);
            CREATE TABLE instruction_state(session_id TEXT, initial_values TEXT, current_values TEXT);
            CREATE TABLE instruction_blob(hash TEXT, value TEXT);
            INSERT INTO session_v2 VALUES ('a','/work','build',2),('b','/work','plan',1),('c','/other','build',3);
            INSERT INTO instruction_blob VALUES ('before','[{"id":"old","name":"Old","description":"Gone"}]'),('after','[{"id":"release","name":"Release","description":"Check tags"}]');
            INSERT INTO instruction_state VALUES ('a','{"core/skill-guidance":"before"}','{"core/skill-guidance":"after"}'),('b','{}','{"core/skill-guidance":"missing"}');
        "#).unwrap();
        drop(db);
        let sessions = read_opencode_guidance(
            dir.path(),
            None,
            Some(Path::new("/work")),
            DateTime::UNIX_EPOCH,
            10,
        );
        assert_eq!(sessions.len(), 2);
        assert_eq!(sessions[0].agent_name.as_deref(), Some("build"));
        assert_eq!(
            sessions[0].listing.as_deref().unwrap(),
            [(
                "Release".into(),
                RecordedEntry::Saved {
                    text: "Check tags".into(),
                    locator: "release".into()
                }
            )]
        );
        assert!(sessions[1].listing.is_none());
        let db = rusqlite::Connection::open(root.join("opencode.db")).unwrap();
        db.execute_batch("DROP TABLE instruction_blob; DROP TABLE instruction_state;")
            .unwrap();
        drop(db);
        let legacy = read_opencode_guidance(
            dir.path(),
            None,
            Some(Path::new("/work")),
            DateTime::UNIX_EPOCH,
            10,
        );
        assert_eq!(legacy.len(), 2);
        assert!(legacy.iter().all(|s| s.listing.is_none()));
    }
}
