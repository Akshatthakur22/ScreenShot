use std::sync::{Arc, Mutex};

use akshat_application::{
    Application, ApplicationError, IpcError, ShotDetailDto, ShotDto, ShotImageDto, StatsDto,
};
use serde::Serialize;
use tauri::State;

#[derive(Clone)]
struct AppState(Arc<Mutex<Option<Application>>>);

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CoreConnection {
    application_name: &'static str,
    core_connected: bool,
}

fn lock_error() -> IpcError {
    IpcError { code: "state_unavailable", message: "The local application state is unavailable." }
}

async fn with_application<T, F>(state: State<'_, AppState>, operation: F) -> Result<T, IpcError>
where
    T: Send + 'static,
    F: FnOnce(&mut Application) -> Result<T, ApplicationError> + Send + 'static,
{
    let shared = Arc::clone(&state.0);
    tauri::async_runtime::spawn_blocking(move || {
        let mut guard = shared.lock().map_err(|_| lock_error())?;
        if guard.is_none() {
            *guard = Some(Application::open_default().map_err(|error| IpcError::from(&error))?);
        }
        let application = guard.as_mut().ok_or_else(lock_error)?;
        operation(application).map_err(|error| IpcError::from(&error))
    })
    .await
    .map_err(|_| IpcError { code: "worker_failed", message: "The local index operation could not finish." })?
}

#[tauri::command]
async fn check_core_connection(state: State<'_, AppState>) -> Result<CoreConnection, IpcError> {
    with_application(state, |_| Ok(())).await?;
    #[cfg(debug_assertions)]
    eprintln!("check_core_connection invoked through Tauri IPC");
    Ok(CoreConnection {
        application_name: "Akshat",
        core_connected: true,
    })
}

#[tauri::command]
async fn get_stats(state: State<'_, AppState>) -> Result<StatsDto, IpcError> {
    let stats = with_application(state, |app| app.stats()).await?;
    #[cfg(debug_assertions)]
    eprintln!("get_stats invoked through Tauri IPC");
    Ok(stats)
}

#[tauri::command]
async fn get_recent_shots(state: State<'_, AppState>, limit: usize) -> Result<Vec<ShotDto>, IpcError> {
    with_application(state, move |app| app.recent_shots(limit)).await
}

#[tauri::command]
async fn search_shots(
    state: State<'_, AppState>,
    query: String,
    limit: usize,
) -> Result<Vec<ShotDto>, IpcError> {
    let results = with_application(state, move |app| app.search_shots(&query, limit)).await?;
    #[cfg(debug_assertions)]
    eprintln!("search_shots invoked through Tauri IPC");
    Ok(results)
}

#[tauri::command]
async fn get_shot(state: State<'_, AppState>, id: i64) -> Result<ShotDto, IpcError> {
    with_application(state, move |app| app.shot(id)).await
}

#[tauri::command]
async fn get_shot_detail(state: State<'_, AppState>, id: i64) -> Result<ShotDetailDto, IpcError> {
    with_application(state, move |app| app.shot_detail(id)).await
}

#[tauri::command]
async fn get_shot_image(state: State<'_, AppState>, id: i64) -> Result<ShotImageDto, IpcError> {
    with_application(state, move |app| app.shot_image(id)).await
}

#[tauri::command]
async fn get_shot_thumbnail(state: State<'_, AppState>, id: i64) -> Result<ShotImageDto, IpcError> {
    with_application(state, move |app| app.shot_thumbnail(id)).await
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(AppState(Arc::new(Mutex::new(None))))
        .invoke_handler(tauri::generate_handler![
            check_core_connection,
            get_stats,
            get_recent_shots,
            search_shots,
            get_shot,
            get_shot_image,
            get_shot_detail,
            get_shot_thumbnail
        ])
        .run(tauri::generate_context!())
        .expect("failed to run Akshat desktop shell");
}

#[cfg(test)]
mod tests {
    use akshat_application::Application;
    use akshat_core::{Index, Line, Rect, Shot};

    fn application() -> Application {
        let mut index = Index::open_in_memory().unwrap();
        index
            .insert(
                &Shot {
                    path: "/tmp/phase-two.png".into(),
                    mtime: 1_700_000_000,
                    width: 800,
                    height: 600,
                },
                &[Line {
                    text: "adapter uses existing trigram search".into(),
                    rect: Rect {
                        x: 0.1,
                        y: 0.2,
                        w: 0.6,
                        h: 0.1,
                    },
                    score: 0.95,
                }],
            )
            .unwrap();
        Application::from_index(index)
    }

    #[test]
    fn adapter_returns_stats_recent_and_fts_results() {
        let app = application();
        let stats = app.stats().unwrap();
        assert_eq!((stats.screenshot_count, stats.line_count), (1, 1));
        assert_eq!(app.recent_shots(10).unwrap().len(), 1);
        assert_eq!(app.search_shots("trigram search", 10).unwrap().len(), 1);
        assert!(app.search_shots("absent", 10).unwrap().is_empty());
    }

    #[test]
    fn invalid_shot_id_is_a_typed_error() {
        assert!(matches!(
            application().shot(0),
            Err(akshat_application::ApplicationError::InvalidShotId)
        ));
    }

    #[test]
    fn unsupported_schema_refuses_to_delete_existing_data() {
        let path = std::env::temp_dir().join(format!(
            "akshat-index-guard-{}-{}.db",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        {
            let db = rusqlite::Connection::open(&path).unwrap();
            db.execute_batch(
                "CREATE TABLE preserved(value TEXT); INSERT INTO preserved VALUES ('kept'); PRAGMA user_version = 42;",
            )
            .unwrap();
        }

        assert!(Index::open(&path).is_err());
        let db = rusqlite::Connection::open(&path).unwrap();
        let value: String = db
            .query_row("SELECT value FROM preserved", [], |row| row.get(0))
            .unwrap();
        assert_eq!(value, "kept");
        drop(db);
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(path.with_extension("db-wal"));
        let _ = std::fs::remove_file(path.with_extension("db-shm"));
    }
}
