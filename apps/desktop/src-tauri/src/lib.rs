use log::{error, info};
use memory_indexer::pipeline::IndexingPipeline;
use memory_indexer::IndexProgress;
use memory_search::{SearchConfig, SearchEngine, SearchQuery, SearchResult};
use rusqlite::params;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::State;
use tauri_plugin_dialog::DialogExt;

/// Get the database directory path.
fn db_dir() -> PathBuf {
    dirs_next::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("memory-os")
}

/// Get the full database file path.
fn db_path() -> PathBuf {
    db_dir().join("memory-os.db")
}

/// Application state shared across commands.
struct AppState {
    db: memory_indexer::db::Database,
    search: SearchEngine,
    /// Shared running flag — new clones passed to background scan threads
    running: Arc<AtomicBool>,
    /// Shared scan progress — updated by background thread, read by frontend
    scan_progress: Arc<Mutex<IndexProgress>>,
}

/// Initialize the application database.
fn init_app() -> memory_indexer::db::Database {
    let path = db_path();

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).ok();
    }

    info!("Opening database at: {}", path.display());
    memory_indexer::db::Database::open(path.to_str().unwrap())
        .expect("Failed to open database")
}

/// Helper to load folders from the config table.
fn load_folders(state: &AppState) -> Vec<String> {
    let folders_json: String = state
        .db
        .connection()
        .query_row(
            "SELECT value FROM config WHERE key = 'scanned_folders'",
            [],
            |row| row.get(0),
        )
        .unwrap_or_else(|_| "[]".to_string());
    serde_json::from_str(&folders_json).unwrap_or_default()
}

/// Helper to save folders to the config table.
fn save_folders(state: &AppState, folders: &[String]) {
    let json = serde_json::to_string(folders).unwrap_or_else(|_| "[]".to_string());
    let _ = state.db.connection().execute(
        "INSERT OR REPLACE INTO config (key, value) VALUES ('scanned_folders', ?1)",
        params![json],
    );
}

/// Open native folder dialog and add the selected folder.
#[tauri::command]
fn add_folder(
    app_handle: tauri::AppHandle,
    state: State<'_, Arc<Mutex<AppState>>>,
) -> Result<Vec<String>, String> {
    let dialog = app_handle.dialog();

    let result = dialog
        .file()
        .blocking_pick_folder();

    let state = state.lock().map_err(|e| e.to_string())?;
    let mut folders = load_folders(&state);

    if let Some(path) = result {
        let path_str = path.to_string();
        if !folders.contains(&path_str) {
            folders.push(path_str);
        }
        save_folders(&state, &folders);
    }

    Ok(folders)
}

/// Get the list of selected folders.
#[tauri::command]
fn get_folders(state: State<'_, Arc<Mutex<AppState>>>) -> Result<Vec<String>, String> {
    let state = state.lock().map_err(|e| e.to_string())?;
    Ok(load_folders(&state))
}

/// Remove a folder from the list.
#[tauri::command]
fn remove_folder(
    folder: String,
    state: State<'_, Arc<Mutex<AppState>>>,
) -> Result<Vec<String>, String> {
    let state = state.lock().map_err(|e| e.to_string())?;
    let mut folders = load_folders(&state);
    folders.retain(|f| f != &folder);
    save_folders(&state, &folders);
    Ok(folders)
}

/// Search command — full-text search across indexed files.
#[tauri::command]
fn search(
    query: SearchQuery,
    state: State<'_, Arc<Mutex<AppState>>>,
) -> Result<Vec<SearchResult>, String> {
    let state = state.lock().map_err(|e| e.to_string())?;
    Ok(state.search.search_fts(state.db.connection(), &query))
}

/// Get file content preview.
#[tauri::command]
fn get_preview(
    file_id: String,
    state: State<'_, Arc<Mutex<AppState>>>,
) -> Result<String, String> {
    let state = state.lock().map_err(|e| e.to_string())?;
    state
        .search
        .get_preview(state.db.connection(), &file_id)
        .ok_or_else(|| "File not found".to_string())
}

/// Get search suggestions.
#[tauri::command]
fn get_suggestions(
    query: String,
    state: State<'_, Arc<Mutex<AppState>>>,
) -> Result<Vec<String>, String> {
    let state = state.lock().map_err(|e| e.to_string())?;
    Ok(state.search.get_suggestions(state.db.connection(), &query))
}

/// Get index statistics.
#[tauri::command]
fn get_stats(
    state: State<'_, Arc<Mutex<AppState>>>,
) -> Result<memory_indexer::IndexStats, String> {
    let state = state.lock().map_err(|e| e.to_string())?;
    let conn = state.db.connection();

    let total_files: i64 = conn
        .query_row("SELECT COUNT(*) FROM files", [], |row| row.get(0))
        .unwrap_or(0);

    let total_size: i64 = conn
        .query_row("SELECT COALESCE(SUM(size), 0) FROM files", [], |row| row.get(0))
        .unwrap_or(0);

    let indexed_files: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM files WHERE status = 'indexed'",
            [],
            |row| row.get(0),
        )
        .unwrap_or(0);

    let mut stmt = conn
        .prepare(
            "SELECT extension, COUNT(*), COALESCE(SUM(size), 0) FROM files GROUP BY extension",
        )
        .map_err(|e| e.to_string())?;

    let file_types = stmt
        .query_map([], |row| {
            Ok(memory_indexer::FileTypeCount {
                extension: row.get(0)?,
                count: row.get(1)?,
                total_size: row.get(2)?,
            })
        })
        .map_err(|e| e.to_string())?
        .filter_map(|r| r.ok())
        .collect();

    let folders: String = conn
        .query_row(
            "SELECT value FROM config WHERE key = 'scanned_folders'",
            [],
            |row| row.get(0),
        )
        .unwrap_or_default();

    let last_scan: Option<String> = conn
        .query_row(
            "SELECT scanned_at FROM scan_log ORDER BY scanned_at DESC LIMIT 1",
            [],
            |row| row.get(0),
        )
        .ok();

    Ok(memory_indexer::IndexStats {
        total_files: total_files as u64,
        total_size: total_size as u64,
        indexed_files: indexed_files as u64,
        failed_files: (total_files - indexed_files) as u64,
        file_types,
        last_scan,
        folders: serde_json::from_str(&folders).unwrap_or_default(),
    })
}

/// Start scanning selected folders on a background thread.
#[tauri::command]
fn start_scan(
    state: State<'_, Arc<Mutex<AppState>>>,
) -> Result<(), String> {
    let (folders, running, progress) = {
        let s = state.lock().map_err(|e| e.to_string())?;
        let folders = load_folders(&s);
        if folders.is_empty() {
            return Err("Chưa có thư mục nào được chọn. Vui lòng chọn thư mục trước.".to_string());
        }

        // Reset progress
        *s.scan_progress.lock().map_err(|e| e.to_string())? = IndexProgress {
            total_files: 0,
            indexed_files: 0,
            failed_files: 0,
            current_file: Some("Đang khởi tạo...".to_string()),
            percentage: 0.0,
            is_running: true,
        };

        s.running.store(true, Ordering::SeqCst);

        (folders, s.running.clone(), s.scan_progress.clone())
    };

    let db_path_clone = db_path().to_str().unwrap_or("memory-os.db").to_string();

    // Spawn a background thread that opens its own DB connection
    std::thread::spawn(move || {
        info!("Scanning folders: {:?}", folders);

        // Open a separate DB connection for this thread
        let thread_db = match memory_indexer::db::Database::open(&db_path_clone) {
            Ok(db) => Arc::new(db),
            Err(e) => {
                error!("Failed to open database in scan thread: {}", e);
                if let Ok(mut p) = progress.lock() {
                    *p = IndexProgress {
                        total_files: 0,
                        indexed_files: 0,
                        failed_files: 0,
                        current_file: Some(format!("Lỗi DB: {}", e)),
                        percentage: 0.0,
                        is_running: false,
                    };
                }
                return;
            }
        };

        // Create pipeline with shared running flag
        let search_config = SearchConfig::default();
        let search_engine = SearchEngine::new(search_config);
        let pipeline =
            IndexingPipeline::new_with_running(thread_db, search_engine, running);

        match pipeline.start_indexing(&folders) {
            Ok(prog) => {
                if let Ok(mut p) = progress.lock() {
                    *p = prog;
                }
            }
            Err(e) => {
                error!("Scan failed: {}", e);
                if let Ok(mut p) = progress.lock() {
                    *p = IndexProgress {
                        total_files: 0,
                        indexed_files: 0,
                        failed_files: 0,
                        current_file: Some(format!("Lỗi: {}", e)),
                        percentage: 0.0,
                        is_running: false,
                    };
                }
            }
        }
    });

    Ok(())
}

/// Stop the scanning process.
#[tauri::command]
fn stop_scan(state: State<'_, Arc<Mutex<AppState>>>) -> Result<(), String> {
    let state = state.lock().map_err(|e| e.to_string())?;
    state.running.store(false, Ordering::SeqCst);

    if let Ok(mut p) = state.scan_progress.lock() {
        p.is_running = false;
        p.current_file = Some("Đã dừng.".to_string());
    }

    Ok(())
}

/// Get current scan progress.
#[tauri::command]
fn get_scan_progress(
    state: State<'_, Arc<Mutex<AppState>>>,
) -> Result<IndexProgress, String> {
    let state = state.lock().map_err(|e| e.to_string())?;
    let progress = state.scan_progress.lock().map_err(|e| e.to_string())?;
    Ok(progress.clone())
}

/// Run the Tauri application.
pub fn run() {
    env_logger::init();
    info!("Starting MemoryOS Desktop v{}", env!("CARGO_PKG_VERSION"));

    let db = init_app();
    let search_config = SearchConfig::default();
    let search_engine = SearchEngine::new(search_config);

    let running = Arc::new(AtomicBool::new(false));
    let scan_progress = Arc::new(Mutex::new(IndexProgress {
        total_files: 0,
        indexed_files: 0,
        failed_files: 0,
        current_file: None,
        percentage: 0.0,
        is_running: false,
    }));

    let state = Arc::new(Mutex::new(AppState {
        db,
        search: search_engine,
        running,
        scan_progress,
    }));

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            search,
            get_preview,
            get_suggestions,
            get_stats,
            add_folder,
            get_folders,
            remove_folder,
            start_scan,
            stop_scan,
            get_scan_progress,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
