use chrono::{Duration as ChronoDuration, Utc};
use tokio::time::{sleep, Duration};
use tracing::{error, info};

use crate::AppState;

pub async fn run(state:AppState){
    loop {
        let settings=match state.db.get_settings().await {Ok(s)=>s,Err(e)=>{error!(error=%e,"scheduler could not load settings");sleep(Duration::from_secs(60)).await;continue;}};
        if !settings.auto_sync {
            { let mut status=state.scheduler_status.write().await; status.enabled=false; status.next_run_at=None; }
            state.scheduler_notify.notified().await;
            continue;
        }
        let interval=settings.sync_interval_minutes.max(60);
        let next=Utc::now()+ChronoDuration::minutes(interval as i64);
        { let mut status=state.scheduler_status.write().await; status.enabled=true; status.next_run_at=Some(next.to_rfc3339()); }
        tokio::select! {
            _=sleep(Duration::from_secs(interval*60))=>{
                info!("scheduled sync starting");
                if let Err(e)=crate::sync::run(&state,"schedule",false).await { error!(error=%e,"scheduled sync failed"); }
            }
            _=state.scheduler_notify.notified()=>{continue;}
        }
    }
}
