use mlua::prelude::*;
use notify_debouncer_mini::{
    Debouncer, new_debouncer,
    notify::{RecommendedWatcher, RecursiveMode},
};
use std::fs;
use std::path::Path;
use std::time::Duration;
use tokio::sync::mpsc::Sender;

pub fn setup_watcher(path: &Path, tx: Sender<()>) -> LuaResult<Debouncer<RecommendedWatcher>> {
    let abs_path = path.to_path_buf();
    let tx_clone = tx.clone();
    let abs_path_clone = abs_path.clone();
    let mut last_mtime = std::time::SystemTime::UNIX_EPOCH;

    if let Ok(metadata) = fs::metadata(&abs_path)
        && let Ok(mtime) = metadata.modified()
    {
        last_mtime = mtime;
    }

    let mut debouncer = new_debouncer(
        Duration::from_millis(500),
        move |res: std::result::Result<Vec<_>, _>| match res {
            Ok(events) => {
                let mut reload = false;
                for event in events {
                    println!("File event: {:?}", event);
                    if let Ok(metadata) = fs::metadata(&abs_path_clone)
                        && let Ok(mtime) = metadata.modified()
                        && mtime > last_mtime
                    {
                        last_mtime = mtime;
                        reload = true;
                    }
                }
                if reload {
                    let _ = tx_clone.blocking_send(());
                }
            }
            Err(e) => eprintln!("Watch error: {:?}", e),
        },
    )
    .map_err(|e| LuaError::RuntimeError(e.to_string()))?;

    debouncer
        .watcher()
        .watch(&abs_path, RecursiveMode::NonRecursive)
        .map_err(|e| LuaError::RuntimeError(e.to_string()))?;

    Ok(debouncer)
}
