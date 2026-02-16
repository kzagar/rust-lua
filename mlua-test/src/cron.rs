use crate::types::{AppState, CronJobInfo, EngineRequest};
use mlua::prelude::*;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc::Sender;
use tokio_cron_scheduler::{Job, JobScheduler};

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
) -> Option<JobScheduler> {
    let res = {
        let state = app_state.lock().unwrap();
        !state.cron_jobs.is_empty()
    };

    if res {
        match JobScheduler::new().await {
            Ok(sched) => {
                let jobs: Vec<_> = {
                    let state = app_state.lock().unwrap();
                    state
                        .cron_jobs
                        .iter()
                        .map(|job_info| (job_info.callback_id, job_info.expression.clone()))
                        .collect()
                };

                for (id, expr) in jobs {
                    let tx_clone = tx.clone();
                    let job_res = Job::new_async(expr.as_str(), move |_uuid, _l| {
                        let tx = tx_clone.clone();
                        Box::pin(async move {
                            if let Err(e) = tx.send(EngineRequest::Cron(id)).await {
                                eprintln!("Failed to send cron trigger: {}", e);
                            }
                        })
                    });

                    match job_res {
                        Ok(job) => {
                            if let Err(e) = sched.add(job).await {
                                eprintln!(
                                    "Failed to add cron job for expression '{}': {}",
                                    expr, e
                                );
                            }
                        }
                        Err(e) => {
                            eprintln!("Invalid cron expression '{}': {}", expr, e);
                        }
                    }
                }
                if let Err(e) = sched.start().await {
                    eprintln!("Failed to start cron scheduler: {}", e);
                    None
                } else {
                    println!("Cron scheduler started.");
                    Some(sched)
                }
            }
            Err(e) => {
                eprintln!("Failed to create job scheduler: {}", e);
                None
            }
        }
    } else {
        None
    }
}
