use chrono::{DateTime, Duration as ChronoDuration, LocalResult, NaiveDateTime, NaiveTime, TimeZone, Utc};
use chrono_tz::Tz;
use tokio::time::{sleep, Duration};
use tracing::{error, info, warn};

use crate::{models::{AppSettings, ScheduleMode, SchedulerStatus}, AppState};

pub fn preview_status(settings: &AppSettings, now: DateTime<Utc>) -> Result<SchedulerStatus, String> {
    if !settings.auto_sync {
        return Ok(SchedulerStatus {
            next_run_at: None,
            next_run_local: None,
            enabled: false,
            mode: schedule_mode_name(&settings.schedule_mode).into(),
        });
    }
    let next = next_run(settings, now)?;
    Ok(SchedulerStatus {
        next_run_at: Some(next.to_rfc3339()),
        next_run_local: format_next_local(settings, next),
        enabled: true,
        mode: schedule_mode_name(&settings.schedule_mode).into(),
    })
}

pub async fn run(state: AppState) {
    loop {
        let settings = match state.db.get_settings().await {
            Ok(settings) => settings,
            Err(error) => {
                error!(error = %error, "scheduler could not load settings");
                sleep(Duration::from_secs(60)).await;
                continue;
            }
        };

        if !settings.auto_sync {
            {
                let mut status = state.scheduler_status.write().await;
                status.enabled = false;
                status.next_run_at = None;
                status.next_run_local = None;
                status.mode = schedule_mode_name(&settings.schedule_mode).into();
            }
            state.scheduler_notify.notified().await;
            continue;
        }

        let now = Utc::now();
        let next = match next_run(&settings, now) {
            Ok(next) => next,
            Err(error) => {
                error!(error = %error, "scheduler configuration is invalid");
                {
                    let mut status = state.scheduler_status.write().await;
                    status.enabled = false;
                    status.next_run_at = None;
                    status.next_run_local = None;
                    status.mode = schedule_mode_name(&settings.schedule_mode).into();
                }
                state.scheduler_notify.notified().await;
                continue;
            }
        };

        let next_local = format_next_local(&settings, next);
        {
            let mut status = state.scheduler_status.write().await;
            status.enabled = true;
            status.next_run_at = Some(next.to_rfc3339());
            status.next_run_local = next_local.clone();
            status.mode = schedule_mode_name(&settings.schedule_mode).into();
        }

        let wait_seconds = (next - now).num_seconds().max(1) as u64;
        info!(
            mode = schedule_mode_name(&settings.schedule_mode),
            next_run = %next,
            next_run_local = next_local.as_deref().unwrap_or("browser-local"),
            "scheduler armed"
        );

        tokio::select! {
            _ = sleep(Duration::from_secs(wait_seconds)) => {
                info!("scheduled sync starting");
                if let Err(error) = crate::sync::run(&state, "schedule", false).await {
                    error!(error = %error, "scheduled sync failed");
                }
            }
            _ = state.scheduler_notify.notified() => continue,
        }
    }
}

fn schedule_mode_name(mode: &ScheduleMode) -> &'static str {
    match mode {
        ScheduleMode::Interval => "interval",
        ScheduleMode::Daily => "daily",
    }
}

fn next_run(settings: &AppSettings, now: DateTime<Utc>) -> Result<DateTime<Utc>, String> {
    match settings.schedule_mode {
        ScheduleMode::Interval => {
            let interval = settings.sync_interval_minutes.max(60);
            Ok(now + ChronoDuration::minutes(interval as i64))
        }
        ScheduleMode::Daily => next_daily_run(settings, now),
    }
}

fn next_daily_run(settings: &AppSettings, now: DateTime<Utc>) -> Result<DateTime<Utc>, String> {
    let timezone: Tz = settings.schedule_timezone.trim().parse().map_err(|_| format!("invalid timezone: {}", settings.schedule_timezone))?;
    let time = NaiveTime::parse_from_str(settings.daily_sync_time.trim(), "%H:%M")
        .map_err(|_| format!("invalid daily sync time: {}", settings.daily_sync_time))?;
    let local_now = now.with_timezone(&timezone);
    let today = local_now.date_naive();
    let mut candidate = resolve_local_time(timezone, today.and_time(time))?;
    if candidate <= local_now {
        let tomorrow = today.succ_opt().ok_or_else(|| "could not calculate tomorrow for scheduler".to_string())?;
        candidate = resolve_local_time(timezone, tomorrow.and_time(time))?;
    }
    Ok(candidate.with_timezone(&Utc))
}

fn resolve_local_time(timezone: Tz, naive: NaiveDateTime) -> Result<DateTime<Tz>, String> {
    match timezone.from_local_datetime(&naive) {
        LocalResult::Single(value) => Ok(value),
        LocalResult::Ambiguous(first, second) => Ok(first.min(second)),
        LocalResult::None => {
            for minutes in 1..=180 {
                let shifted = naive + ChronoDuration::minutes(minutes);
                match timezone.from_local_datetime(&shifted) {
                    LocalResult::Single(value) => {
                        warn!(requested = %naive, resolved = %shifted, "daily sync time fell in a DST gap; shifted forward");
                        return Ok(value);
                    }
                    LocalResult::Ambiguous(first, second) => return Ok(first.min(second)),
                    LocalResult::None => {}
                }
            }
            Err(format!("could not resolve local scheduled time {naive} in {timezone}"))
        }
    }
}

fn format_next_local(settings: &AppSettings, next: DateTime<Utc>) -> Option<String> {
    if settings.schedule_mode == ScheduleMode::Daily {
        if let Ok(timezone) = settings.schedule_timezone.trim().parse::<Tz>() {
            return Some(format!(
                "{} ({})",
                next.with_timezone(&timezone).format("%Y-%m-%d %H:%M"),
                timezone
            ));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn daily_midnight_uses_configured_timezone() {
        let mut settings = AppSettings::default();
        settings.schedule_mode = ScheduleMode::Daily;
        settings.daily_sync_time = "00:00".into();
        settings.schedule_timezone = "Asia/Shanghai".into();
        let now = DateTime::parse_from_rfc3339("2026-08-09T10:00:00Z").unwrap().with_timezone(&Utc);
        let next = next_run(&settings, now).unwrap();
        assert_eq!(next.to_rfc3339(), "2026-08-09T16:00:00+00:00");
    }

    #[test]
    fn interval_mode_keeps_existing_behavior() {
        let mut settings = AppSettings::default();
        settings.schedule_mode = ScheduleMode::Interval;
        settings.sync_interval_minutes = 360;
        let now = DateTime::parse_from_rfc3339("2026-08-09T10:00:00Z").unwrap().with_timezone(&Utc);
        let next = next_run(&settings, now).unwrap();
        assert_eq!(next.to_rfc3339(), "2026-08-09T16:00:00+00:00");
    }
}
