use std::sync::Arc;
use std::time::Duration;

use chrono::{Datelike, Timelike, Utc};

use crate::state::AppState;

pub fn spawn(state: Arc<AppState>) {
    let s_meter = state.clone();
    let s_payout = state;

    // Hourly meter reporter — runs at the top of every hour.
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(3600));
        loop {
            interval.tick().await;
            if let Err(e) = report_meter_usage(&s_meter).await {
                tracing::error!("Meter reporter error: {e}");
            }
        }
    });

    // Nightly payout at ~02:00 UTC — checked every 5 minutes.
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(300));
        let mut last_payout_day: Option<u32> = None;
        loop {
            interval.tick().await;
            let now = Utc::now();
            if now.hour() == 2 {
                let today = now.ordinal();
                if last_payout_day != Some(today) {
                    if let Err(e) = process_payouts(&s_payout).await {
                        tracing::error!("Payout processor error: {e}");
                    } else {
                        last_payout_day = Some(today);
                    }
                }
            }
        }
    });
}

async fn report_meter_usage(state: &AppState) -> anyhow::Result<()> {
    let (pool, stripe_cfg) = match (&state.db, &state.stripe) {
        (Some(p), Some(s)) => (p, s),
        _ => return Ok(()),
    };

    let now = Utc::now();
    // Align to the most recent whole hour boundary.
    let period_end = now
        .with_minute(0)
        .and_then(|t| t.with_second(0))
        .and_then(|t| t.with_nanosecond(0))
        .unwrap_or(now);
    let period_start = period_end - chrono::Duration::hours(1);

    #[derive(sqlx::FromRow)]
    struct BillingCustomer {
        key_id: String,
        stripe_customer_id: String,
    }

    let customers = sqlx::query_as::<_, BillingCustomer>(
        "SELECT key_id, stripe_customer_id FROM billing_customers",
    )
    .fetch_all(pool)
    .await?;

    for cust in customers {
        // Skip if this period is already reported (idempotency).
        let already: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM billing_meter_reports \
             WHERE key_id = $1 AND period_start = $2",
        )
        .bind(&cust.key_id)
        .bind(period_start)
        .fetch_one(pool)
        .await?;

        if already > 0 {
            continue;
        }

        #[derive(sqlx::FromRow)]
        struct Usage {
            tokens_in: Option<i64>,
            tokens_out: Option<i64>,
        }

        let usage = sqlx::query_as::<_, Usage>(
            "SELECT SUM(tokens_in) as tokens_in, SUM(tokens_out) as tokens_out \
             FROM usage_logs \
             WHERE key_id = $1 AND timestamp >= $2 AND timestamp < $3",
        )
        .bind(&cust.key_id)
        .bind(period_start)
        .bind(period_end)
        .fetch_one(pool)
        .await?;

        let tin = usage.tokens_in.unwrap_or(0);
        let tout = usage.tokens_out.unwrap_or(0);
        // Weight output tokens 2× (output is more expensive to generate).
        let weighted_total = tin + tout * 2;

        if weighted_total <= 0 {
            continue;
        }

        let identifier = format!("meter-{}-{}", cust.key_id, period_start.timestamp());

        match stripe_cfg
            .client
            .report_meter_event(
                &stripe_cfg.meter_event_name,
                &cust.stripe_customer_id,
                weighted_total as u64,
                period_start.timestamp(),
                &identifier,
            )
            .await
        {
            Ok(()) => {
                sqlx::query(
                    "INSERT INTO billing_meter_reports \
                     (key_id, period_start, period_end, tokens_in, tokens_out, stripe_event_id) \
                     VALUES ($1, $2, $3, $4, $5, $6) \
                     ON CONFLICT (key_id, period_start) DO NOTHING",
                )
                .bind(&cust.key_id)
                .bind(period_start)
                .bind(period_end)
                .bind(tin)
                .bind(tout)
                .bind(&identifier)
                .execute(pool)
                .await?;

                tracing::info!(
                    key_id = %cust.key_id,
                    tokens_weighted = weighted_total,
                    "Stripe meter event reported"
                );
            }
            Err(e) => {
                tracing::warn!(
                    key_id = %cust.key_id,
                    error = %e,
                    "Failed to report meter event — will retry next hour"
                );
            }
        }
    }

    Ok(())
}

async fn process_payouts(state: &AppState) -> anyhow::Result<()> {
    let (pool, stripe_cfg) = match (&state.db, &state.stripe) {
        (Some(p), Some(s)) => (p, s),
        _ => return Ok(()),
    };

    let now = Utc::now();
    let period_start = now - chrono::Duration::days(1);

    #[derive(sqlx::FromRow)]
    struct Provider {
        node_id: String,
        stripe_account_id: String,
    }

    let providers = sqlx::query_as::<_, Provider>(
        "SELECT n.id as node_id, psa.stripe_account_id \
         FROM provider_stripe_accounts psa \
         JOIN nodes n ON n.id = psa.node_id \
         WHERE psa.onboarding_complete = true",
    )
    .fetch_all(pool)
    .await?;

    if providers.is_empty() {
        tracing::debug!("No onboarded providers — skipping payouts");
        return Ok(());
    }

    #[derive(sqlx::FromRow)]
    struct TotalUsage {
        tokens_in: Option<i64>,
        tokens_out: Option<i64>,
    }

    let totals = sqlx::query_as::<_, TotalUsage>(
        "SELECT SUM(tokens_in) as tokens_in, SUM(tokens_out) as tokens_out \
         FROM usage_logs WHERE timestamp >= $1 AND timestamp < $2",
    )
    .bind(period_start)
    .bind(now)
    .fetch_one(pool)
    .await?;

    let total_weighted =
        totals.tokens_in.unwrap_or(0) + totals.tokens_out.unwrap_or(0) * 2;

    if total_weighted == 0 {
        tracing::info!("No token usage in payout window — skipping");
        return Ok(());
    }

    let gross_usd = (total_weighted as f64) * stripe_cfg.token_rate_usd_per_1k / 1000.0;
    let provider_pool_usd = gross_usd * (1.0 - stripe_cfg.commission_rate);
    let per_provider_usd = provider_pool_usd / providers.len() as f64;
    let per_provider_cents = (per_provider_usd * 100.0).floor() as u64;

    if per_provider_cents == 0 {
        tracing::info!(
            gross_usd,
            provider_count = providers.len(),
            "Provider payout rounds to $0 — skipping"
        );
        return Ok(());
    }

    let description = format!(
        "Infer provider earnings {} UTC",
        period_start.format("%Y-%m-%d")
    );

    for provider in &providers {
        match stripe_cfg
            .client
            .create_transfer(
                per_provider_cents,
                "usd",
                &provider.stripe_account_id,
                &description,
            )
            .await
        {
            Ok(transfer_id) => {
                tracing::info!(
                    node_id = %provider.node_id,
                    amount_cents = per_provider_cents,
                    transfer_id = %transfer_id,
                    "Provider payout transfer created"
                );
            }
            Err(e) => {
                tracing::warn!(
                    node_id = %provider.node_id,
                    error = %e,
                    "Payout transfer failed — will retry next run"
                );
            }
        }
    }

    tracing::info!(
        providers = providers.len(),
        per_provider_cents,
        gross_usd,
        "Nightly payout run complete"
    );
    Ok(())
}
