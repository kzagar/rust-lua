use crate::types::{AppState, CronJobInfo, EngineRequest};
use chrono::{DateTime, Local};
use croner::Cron;
use mlua::prelude::*;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc::Sender;

pub struct CronScheduler {
    state: Arc<Mutex<AppState>>,
}

impl LuaUserData for CronScheduler {
    fn add_methods<M: LuaUserDataMethods<Self>>(methods: &mut M) {
        methods.add_method(
            "register",
            |lua, scheduler, (expression, func): (String, LuaFunction)| {
                let mut state = scheduler.state.lock().unwrap();
                let callback_id = state.cron_jobs.len();
                let callback_key = lua.create_registry_value(func)?;
                state.cron_jobs.push(CronJobInfo {
                    expression,
                    callback_id,
                    callback_key,
                });
                Ok(())
            },
        );
    }
}

pub fn register(lua: &Lua, app_state: Arc<Mutex<AppState>>) -> LuaResult<()> {
    let cron = lua.create_table()?;
    let state_clone = app_state.clone();
    cron.set(
        "new",
        lua.create_function(move |_, ()| {
            Ok(CronScheduler {
                state: state_clone.clone(),
            })
        })?,
    )?;
    lua.globals().set("cron", cron)?;
    Ok(())
}

pub async fn start(
    app_state: Arc<Mutex<AppState>>,
    tx: Sender<EngineRequest>,
) -> Option<tokio::task::JoinHandle<()>> {
    let has_jobs = {
        let state = app_state.lock().unwrap();
        !state.cron_jobs.is_empty()
    };

    if has_jobs {
        let jobs: Vec<_> = {
            let state = app_state.lock().unwrap();
            state
                .cron_jobs
                .iter()
                .map(|job_info| (job_info.callback_id, job_info.expression.clone()))
                .collect()
        };

        let handle = tokio::spawn(async move {
            let mut cron_jobs: Vec<(usize, Cron)> = Vec::new();
            for (id, expr) in jobs {
                // Try parsing using the Parse trait which Cron implements
                match expr.parse::<Cron>() {
                    Ok(cron) => cron_jobs.push((id, cron)),
                    Err(e) => eprintln!("Invalid cron expression '{}': {}", expr, e),
                }
            }

            if cron_jobs.is_empty() {
                return;
            }

            println!("Cron scheduler started with {} jobs.", cron_jobs.len());

            loop {
                let now = Local::now();
                let mut next_run: Option<(DateTime<Local>, Vec<usize>)> = None;

                for (id, cron) in &cron_jobs {
                    if let Ok(next) = cron.find_next_occurrence(&now, false) {
                        match next_run {
                            None => next_run = Some((next, vec![*id])),
                            Some((curr_next, ref mut ids)) => {
                                if next < curr_next {
                                    next_run = Some((next, vec![*id]));
                                } else if next == curr_next {
                                    ids.push(*id);
                                }
                            }
                        }
                    }
                }

                if let Some((next, ids)) = next_run {
                    let sleep_duration = next
                        .signed_duration_since(now)
                        .to_std()
                        .unwrap_or(std::time::Duration::from_secs(0));
                    tokio::time::sleep(sleep_duration).await;

                    for id in ids {
                        if let Err(e) = tx.send(EngineRequest::Cron(id)).await {
                            eprintln!("Failed to send cron trigger: {}", e);
                        }
                    }
                    // Avoid tight loop
                    tokio::time::sleep(std::time::Duration::from_millis(1000)).await;
                } else {
                    break;
                }
            }
        });

        Some(handle)
    } else {
        None
    }
}
