use anyhow::{Context, Result};
use hmac::{Hmac, Mac};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

/// Thin reqwest wrapper around the Stripe REST API.
pub struct StripeClient {
    secret_key: String,
    client: reqwest::Client,
}

impl StripeClient {
    pub fn new(secret_key: String) -> Self {
        Self {
            secret_key,
            client: reqwest::Client::new(),
        }
    }

    async fn post_form(&self, path: &str, params: &[(&str, &str)]) -> Result<serde_json::Value> {
        let url = format!("https://api.stripe.com/v1{path}");
        let resp = self
            .client
            .post(&url)
            .basic_auth(&self.secret_key, None::<&str>)
            .form(params)
            .send()
            .await
            .with_context(|| format!("Stripe POST {path} failed"))?;

        let status = resp.status();
        let body: serde_json::Value = resp
            .json()
            .await
            .with_context(|| format!("Stripe {path} response parse error"))?;

        if !status.is_success() {
            let msg = body["error"]["message"]
                .as_str()
                .unwrap_or("unknown stripe error");
            anyhow::bail!("Stripe {path} {status}: {msg}");
        }
        Ok(body)
    }

    /// Create a Stripe Customer and return their `cus_xxx` ID.
    pub async fn create_customer(&self, owner_label: &str) -> Result<String> {
        let params = [("metadata[owner]", owner_label)];
        let body = self.post_form("/customers", &params).await?;
        body["id"]
            .as_str()
            .map(|s| s.to_owned())
            .context("Stripe customer missing id")
    }

    /// Attach a payment method to a customer and set it as their invoice default.
    pub async fn attach_payment_method(
        &self,
        customer_id: &str,
        payment_method_id: &str,
    ) -> Result<()> {
        self.post_form(
            &format!("/payment_methods/{payment_method_id}/attach"),
            &[("customer", customer_id)],
        )
        .await?;
        self.post_form(
            &format!("/customers/{customer_id}"),
            &[("invoice_settings[default_payment_method]", payment_method_id)],
        )
        .await?;
        Ok(())
    }

    /// Create a metered subscription for a customer on an existing Stripe Price.
    pub async fn create_subscription(&self, customer_id: &str, price_id: &str) -> Result<String> {
        let params = [("customer", customer_id), ("items[0][price]", price_id)];
        let body = self.post_form("/subscriptions", &params).await?;
        body["id"]
            .as_str()
            .map(|s| s.to_owned())
            .context("Stripe subscription missing id")
    }

    /// Report a usage event to the Stripe Meters API (idempotent via `identifier`).
    pub async fn report_meter_event(
        &self,
        event_name: &str,
        stripe_customer_id: &str,
        value: u64,
        timestamp: i64,
        identifier: &str,
    ) -> Result<()> {
        let ts = timestamp.to_string();
        let val = value.to_string();
        let params = [
            ("event_name", event_name),
            ("stripe_customer_id", stripe_customer_id),
            ("payload[value]", val.as_str()),
            ("timestamp", ts.as_str()),
            ("identifier", identifier),
        ];
        self.post_form("/billing/meter_events", &params).await?;
        Ok(())
    }

    /// Create a Stripe Connect Express account and return the `acct_xxx` ID.
    pub async fn create_connect_account(&self, email: Option<&str>) -> Result<String> {
        let mut params: Vec<(&str, &str)> = vec![
            ("type", "express"),
            ("capabilities[transfers][requested]", "true"),
        ];
        if let Some(e) = email {
            params.push(("email", e));
        }
        let body = self.post_form("/accounts", &params).await?;
        body["id"]
            .as_str()
            .map(|s| s.to_owned())
            .context("Stripe account missing id")
    }

    /// Create an Account Link URL for Connect onboarding.
    pub async fn create_account_link(
        &self,
        account_id: &str,
        return_url: &str,
        refresh_url: &str,
    ) -> Result<String> {
        let params = [
            ("account", account_id),
            ("type", "account_onboarding"),
            ("return_url", return_url),
            ("refresh_url", refresh_url),
        ];
        let body = self.post_form("/account_links", &params).await?;
        body["url"]
            .as_str()
            .map(|s| s.to_owned())
            .context("Stripe account_link missing url")
    }

    /// Create a Transfer to a Connect account (provider payout).
    pub async fn create_transfer(
        &self,
        amount_cents: u64,
        currency: &str,
        destination: &str,
        description: &str,
    ) -> Result<String> {
        let amt = amount_cents.to_string();
        let params = [
            ("amount", amt.as_str()),
            ("currency", currency),
            ("destination", destination),
            ("description", description),
        ];
        let body = self.post_form("/transfers", &params).await?;
        body["id"]
            .as_str()
            .map(|s| s.to_owned())
            .context("Stripe transfer missing id")
    }
}

/// Validate a Stripe webhook HMAC-SHA256 signature.
///
/// `sig_header` is the raw `Stripe-Signature` header value.
/// Returns `true` if the signature is valid.
pub fn validate_webhook_signature(payload: &[u8], sig_header: &str, secret: &str) -> Result<bool> {
    let mut timestamp_str: Option<&str> = None;
    let mut signatures: Vec<&str> = Vec::new();

    for part in sig_header.split(',') {
        if let Some(t) = part.strip_prefix("t=") {
            timestamp_str = Some(t);
        } else if let Some(v) = part.strip_prefix("v1=") {
            signatures.push(v);
        }
    }

    let ts = timestamp_str.context("missing t= in Stripe-Signature")?;
    if signatures.is_empty() {
        anyhow::bail!("no v1= signatures in Stripe-Signature");
    }

    let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).context("invalid HMAC key")?;
    mac.update(ts.as_bytes());
    mac.update(b".");
    mac.update(payload);
    let expected = hex::encode(mac.finalize().into_bytes());

    Ok(signatures.iter().any(|s| *s == expected))
}
