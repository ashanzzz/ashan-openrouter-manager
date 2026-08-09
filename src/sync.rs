use chrono::Utc;
use uuid::Uuid;

use crate::{AppState, error::AppError, models::{ManagedChannel, ScanResult, SyncRun}, newapi::NewApiClient, openrouter::OpenRouterClient, ranking, tester};

async fn secret(state:&AppState,name:&str)->Result<String,AppError>{
    let encrypted=state.db.get_secret_ciphertext(name).await?.ok_or_else(||AppError::bad(format!("secret {name} is not configured")))?;
    state.crypto.decrypt(&encrypted)
}

pub async fn scan(state:&AppState)->Result<ScanResult,AppError>{
    let settings=state.db.get_settings().await?;
    let key=secret(state,"openrouter_api_key").await?;
    let client=OpenRouterClient::new(state.http.clone(),&settings.openrouter_api_base);
    let models=client.list_models(&key).await?;
    let total=models.len();
    let benchmarks=client.benchmarks(&key).await?;
    let (free_count,candidates)=ranking::rank(models,benchmarks,&settings);
    let tested=tester::preflight(client,key,candidates,settings.preflight_concurrency).await;
    let selected:Vec<_>=tested.iter().filter(|m|m.usable==Some(true)).take(3).cloned().collect();
    let warning=if selected.len()<3{Some(format!("only {} usable models found; production will not be changed",selected.len()))}else{None};
    let result=ScanResult{scanned_at:Utc::now().to_rfc3339(),total_models:total,free_models:free_count,ranked_candidates:tested,selected,warning};
    state.db.save_last_scan(&result).await?;
    Ok(result)
}

pub async fn run(state:&AppState,trigger:&str,force:bool)->Result<SyncRun,AppError>{
    let _guard=state.sync_guard.try_lock().map_err(|_|AppError::conflict("a sync is already running"))?;
    let started=Utc::now().to_rfc3339(); let id=Uuid::new_v4().to_string();
    let result=run_inner(state,force).await;
    let ended=Utc::now().to_rfc3339();
    let run=match result {
        Ok((changed,models,status))=>SyncRun{id,started_at:started,ended_at:ended,trigger:trigger.into(),status,changed,selected_models:models,error:None},
        Err(e)=>SyncRun{id,started_at:started,ended_at:ended,trigger:trigger.into(),status:"failed".into(),changed:false,selected_models:vec![],error:Some(e.to_string())},
    };
    state.db.record_run(&run).await?;
    if run.status=="failed" { return Err(AppError::bad(run.error.clone().unwrap_or_else(||"sync failed".into()))); }
    Ok(run)
}

async fn run_inner(state:&AppState,force:bool)->Result<(bool,Vec<String>,String),AppError>{
    let settings=state.db.get_settings().await?;
    let scan=scan(state).await?;
    if scan.selected.len()!=3{return Err(AppError::bad(scan.warning.unwrap_or_else(||"three usable models are required".into())));}
    let selected_ids:Vec<String>=scan.selected.iter().map(|m|m.id.clone()).collect();
    let openrouter_key=secret(state,"openrouter_api_key").await?;
    let admin_token=secret(state,"newapi_admin_token").await?;
    let client=NewApiClient::new(state.http.clone(),&settings,admin_token)?;
    client.test_connection().await?;
    let mut managed=state.db.list_managed_channels().await?;
    client.assert_no_foreign_conflicts(&settings,&managed).await?;

    if managed.is_empty(){
        let mut created=Vec::new();
        for (idx,model) in scan.selected.iter().enumerate(){
            match client.create_channel(&settings,&openrouter_key,model,(idx+1) as i64).await {
                Ok(c)=>created.push(c),
                Err(e)=>{let _=client.delete_exact(&settings,&created).await;return Err(e);}
            }
        }
        for c in &created { if let Err(e)=client.test_channel(c.channel_id,&settings.alias_model).await{let _=client.delete_exact(&settings,&created).await;return Err(e);} }
        for c in &created { if let Err(e)=client.set_status(c.channel_id,settings.enabled_status).await{let _=client.delete_exact(&settings,&created).await;return Err(e);} }
        if let Err(e)=maybe_e2e(state,&client,&settings).await{let _=client.delete_exact(&settings,&created).await;return Err(e);}
        for c in &created { state.db.upsert_managed_channel(c).await?; }
        return Ok((true,selected_ids,"initialized".into()));
    }
    if managed.len()!=3{return Err(AppError::conflict(format!("expected 3 managed channels in database, found {}",managed.len())));}
    managed.sort_by_key(|c|c.rank);
    for c in &managed { client.assert_owned(&settings,c).await?; }
    let current:Vec<String>=managed.iter().map(|c|c.model_id.clone()).collect();
    if current==selected_ids && !force{return Ok((false,selected_ids,"no_change".into()));}

    let previous=managed.clone();
    for (c,new_model) in managed.iter().zip(scan.selected.iter()) {
        if let Err(error)=client.update_model(&settings,c,&openrouter_key,&new_model.id).await {
            let _=rollback(&client,&settings,&openrouter_key,&previous).await;
            return Err(error);
        }
    }
    for c in &managed {
        if let Err(error)=client.test_channel(c.channel_id,&settings.alias_model).await {
            let _=rollback(&client,&settings,&openrouter_key,&previous).await;
            return Err(error);
        }
    }
    if let Err(error)=maybe_e2e(state,&client,&settings).await{let _=rollback(&client,&settings,&openrouter_key,&previous).await;return Err(error);}
    for (c,new_model) in managed.iter().zip(scan.selected.iter()) {
        let updated=ManagedChannel{model_id:new_model.id.clone(),updated_at:Utc::now().to_rfc3339(),..c.clone()};
        state.db.upsert_managed_channel(&updated).await?;
    }
    Ok((true,selected_ids,"success".into()))
}

async fn rollback(client:&NewApiClient,settings:&crate::models::AppSettings,key:&str,previous:&[ManagedChannel])->Result<(),AppError>{
    for c in previous { let _=client.update_model(settings,c,key,&c.model_id).await; }
    Ok(())
}

async fn maybe_e2e(state:&AppState,client:&NewApiClient,settings:&crate::models::AppSettings)->Result<(),AppError>{
    if !settings.e2e_test_enabled{return Ok(());}
    let token=secret(state,"newapi_test_token").await?;
    client.e2e_test(&token,&settings.alias_model).await
}
