package gateway

import (
	"context"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"net/url"
	"strings"
	"time"

	gwdb "infer_platform/internal/gateway/db"

	"github.com/google/uuid"
)

type stripeClient struct {
	secretKey string
	client    *http.Client
}

func newStripeClient(secret string) *stripeClient {
	return &stripeClient{secretKey: secret, client: &http.Client{Timeout: 30 * time.Second}}
}

func (s *stripeClient) postForm(path string, params url.Values) (map[string]any, error) {
	req, _ := http.NewRequest(http.MethodPost, "https://api.stripe.com/v1"+path, strings.NewReader(params.Encode()))
	req.SetBasicAuth(s.secretKey, "")
	req.Header.Set("Content-Type", "application/x-www-form-urlencoded")
	resp, err := s.client.Do(req)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()
	var body map[string]any
	if err := json.NewDecoder(resp.Body).Decode(&body); err != nil {
		return nil, err
	}
	if resp.StatusCode/100 != 2 {
		msg := "unknown stripe error"
		if errBody, ok := body["error"].(map[string]any); ok {
			if s, ok := errBody["message"].(string); ok {
				msg = s
			}
		}
		return nil, fmt.Errorf("Stripe %s %s: %s", path, resp.Status, msg)
	}
	return body, nil
}

func (s *stripeClient) createCustomer(owner string) (string, error) {
	body, err := s.postForm("/customers", url.Values{"metadata[owner]": []string{owner}})
	if err != nil {
		return "", err
	}
	return body["id"].(string), nil
}

func (s *stripeClient) attachPaymentMethod(customerID, paymentMethodID string) error {
	if _, err := s.postForm("/payment_methods/"+paymentMethodID+"/attach", url.Values{"customer": []string{customerID}}); err != nil {
		return err
	}
	_, err := s.postForm("/customers/"+customerID, url.Values{"invoice_settings[default_payment_method]": []string{paymentMethodID}})
	return err
}

func (s *stripeClient) createSubscription(customerID, priceID string) (string, error) {
	body, err := s.postForm("/subscriptions", url.Values{"customer": []string{customerID}, "items[0][price]": []string{priceID}})
	if err != nil {
		return "", err
	}
	return body["id"].(string), nil
}

func (s *stripeClient) createConnectAccount(email string) (string, error) {
	params := url.Values{"type": []string{"express"}, "capabilities[transfers][requested]": []string{"true"}}
	if email != "" {
		params.Set("email", email)
	}
	body, err := s.postForm("/accounts", params)
	if err != nil {
		return "", err
	}
	return body["id"].(string), nil
}

func (s *stripeClient) createAccountLink(accountID, returnURL, refreshURL string) (string, error) {
	body, err := s.postForm("/account_links", url.Values{"account": []string{accountID}, "type": []string{"account_onboarding"}, "return_url": []string{returnURL}, "refresh_url": []string{refreshURL}})
	if err != nil {
		return "", err
	}
	return body["url"].(string), nil
}

func (s *stripeClient) reportMeterEvent(eventName, customerID string, value uint64, timestamp int64, identifier string) error {
	_, err := s.postForm("/billing/meter_events", url.Values{"event_name": []string{eventName}, "stripe_customer_id": []string{customerID}, "payload[value]": []string{fmt.Sprintf("%d", value)}, "timestamp": []string{fmt.Sprintf("%d", timestamp)}, "identifier": []string{identifier}})
	return err
}

func (s *stripeClient) createTransfer(amountCents uint64, currency, destination, description string) (string, error) {
	body, err := s.postForm("/transfers", url.Values{"amount": []string{fmt.Sprintf("%d", amountCents)}, "currency": []string{currency}, "destination": []string{destination}, "description": []string{description}})
	if err != nil {
		return "", err
	}
	return body["id"].(string), nil
}

func (a *App) handleBillingSetup(w http.ResponseWriter, r *http.Request) {
	if a.cfg.Stripe == nil {
		writeJSON(w, http.StatusServiceUnavailable, map[string]any{"error": map[string]any{"message": "Billing requires STRIPE_SECRET_KEY to be configured", "type": "service_unavailable"}})
		return
	}
	validated, _ := validatedKeyFromContext(r.Context())
	var req struct {
		PaymentMethodID string `json:"payment_method_id"`
	}
	if err := decodeJSON(r, &req); err != nil {
		writeJSON(w, http.StatusBadRequest, map[string]any{"error": map[string]any{"message": "invalid request body", "type": "invalid_request_error"}})
		return
	}

	existing, err := a.billingRepo.FindCustomerByKeyID(r.Context(), validated.KeyID)
	if err != nil {
		writeJSON(w, http.StatusInternalServerError, map[string]any{"error": map[string]any{"message": "database error", "type": "server_error"}})
		return
	}
	if existing != nil {
		writeJSON(w, http.StatusOK, map[string]any{"customer_id": existing.StripeCustomerID, "subscription_id": existing.StripeSubscriptionID})
		return
	}

	owner := validated.KeyID
	key, err := a.keyRepo.FindByID(r.Context(), validated.KeyID)
	if err == nil && key != nil && key.Owner != "" {
		owner = key.Owner
	}
	stripe := newStripeClient(a.cfg.Stripe.SecretKey)
	customerID, err := stripe.createCustomer(owner)
	if err != nil {
		writeJSON(w, http.StatusBadGateway, map[string]any{"error": map[string]any{"message": "failed to create Stripe customer", "type": "stripe_error"}})
		return
	}
	if err := stripe.attachPaymentMethod(customerID, req.PaymentMethodID); err != nil {
		writeJSON(w, http.StatusBadGateway, map[string]any{"error": map[string]any{"message": "failed to attach payment method", "type": "stripe_error"}})
		return
	}
	var subscriptionID *string
	if a.cfg.Stripe.PriceID != "" {
		id, err := stripe.createSubscription(customerID, a.cfg.Stripe.PriceID)
		if err != nil {
			writeJSON(w, http.StatusBadGateway, map[string]any{"error": map[string]any{"message": "failed to create subscription", "type": "stripe_error"}})
			return
		}
		subscriptionID = &id
	}
	now := time.Now().UTC()
	paymentMethodID := req.PaymentMethodID
	if err := a.billingRepo.CreateCustomer(r.Context(), gwdb.BillingCustomer{ID: uuid.NewString(), KeyID: validated.KeyID, StripeCustomerID: customerID, StripePaymentMethodID: &paymentMethodID, StripeSubscriptionID: subscriptionID, CreatedAt: now, UpdatedAt: now}); err != nil {
		writeJSON(w, http.StatusInternalServerError, map[string]any{"error": map[string]any{"message": "database error", "type": "server_error"}})
		return
	}
	writeJSON(w, http.StatusCreated, map[string]any{"customer_id": customerID, "subscription_id": subscriptionID})
}

func (a *App) handleBillingConnect(w http.ResponseWriter, r *http.Request) {
	if a.cfg.Stripe == nil {
		writeJSON(w, http.StatusServiceUnavailable, map[string]any{"error": map[string]any{"message": "Billing requires STRIPE_SECRET_KEY to be configured", "type": "service_unavailable"}})
		return
	}
	var req struct {
		NodeID, ReturnURL, RefreshURL string
		Email                         *string
	}
	if err := decodeJSON(r, &req); err != nil {
		writeJSON(w, http.StatusBadRequest, map[string]any{"error": map[string]any{"message": "invalid request body", "type": "invalid_request_error"}})
		return
	}
	stripe := newStripeClient(a.cfg.Stripe.SecretKey)
	existing, err := a.billingRepo.FindProviderAccountByNodeID(r.Context(), req.NodeID)
	if err != nil {
		writeJSON(w, http.StatusInternalServerError, map[string]any{"error": map[string]any{"message": "database error", "type": "server_error"}})
		return
	}
	if existing != nil {
		if existing.OnboardingComplete {
			writeJSON(w, http.StatusConflict, map[string]any{"error": map[string]any{"message": "Stripe Connect onboarding already complete for this node", "type": "invalid_request_error"}})
			return
		}
		url, err := stripe.createAccountLink(existing.StripeAccountID, req.ReturnURL, req.RefreshURL)
		if err != nil {
			writeJSON(w, http.StatusBadGateway, map[string]any{"error": map[string]any{"message": "failed to create account link", "type": "stripe_error"}})
			return
		}
		writeJSON(w, http.StatusOK, map[string]any{"account_id": existing.StripeAccountID, "onboarding_url": url})
		return
	}
	accountID, err := stripe.createConnectAccount(derefString(req.Email))
	if err != nil {
		writeJSON(w, http.StatusBadGateway, map[string]any{"error": map[string]any{"message": "failed to create Connect account", "type": "stripe_error"}})
		return
	}
	link, err := stripe.createAccountLink(accountID, req.ReturnURL, req.RefreshURL)
	if err != nil {
		writeJSON(w, http.StatusBadGateway, map[string]any{"error": map[string]any{"message": "failed to create account link", "type": "stripe_error"}})
		return
	}
	now := time.Now().UTC()
	if err := a.billingRepo.CreateProviderAccount(r.Context(), gwdb.ProviderStripeAccount{ID: uuid.NewString(), NodeID: req.NodeID, StripeAccountID: accountID, OnboardingComplete: false, CreatedAt: now, UpdatedAt: now}); err != nil {
		writeJSON(w, http.StatusInternalServerError, map[string]any{"error": map[string]any{"message": "database error", "type": "server_error"}})
		return
	}
	writeJSON(w, http.StatusCreated, map[string]any{"account_id": accountID, "onboarding_url": link})
}

func (a *App) handleStripeWebhook(w http.ResponseWriter, r *http.Request) {
	if a.cfg.Stripe == nil {
		writeJSON(w, http.StatusServiceUnavailable, map[string]any{"error": map[string]any{"message": "Billing requires STRIPE_SECRET_KEY to be configured", "type": "service_unavailable"}})
		return
	}
	body, err := io.ReadAll(r.Body)
	if err != nil {
		writeJSON(w, http.StatusBadRequest, map[string]any{"error": map[string]any{"message": "Invalid JSON payload", "type": "invalid_request_error"}})
		return
	}
	if a.cfg.Stripe.WebhookSecret != "" {
		sig := r.Header.Get("stripe-signature")
		if sig == "" {
			writeJSON(w, http.StatusBadRequest, map[string]any{"error": map[string]any{"message": "Missing Stripe-Signature header", "type": "invalid_request_error"}})
			return
		}
		if !validateWebhookSignature(body, sig, a.cfg.Stripe.WebhookSecret) {
			writeJSON(w, http.StatusUnauthorized, map[string]any{"error": map[string]any{"message": "Webhook signature verification failed", "type": "authentication_error"}})
			return
		}
	}
	var event map[string]any
	if err := json.Unmarshal(body, &event); err != nil {
		writeJSON(w, http.StatusBadRequest, map[string]any{"error": map[string]any{"message": "Invalid JSON payload", "type": "invalid_request_error"}})
		return
	}
	eventType, _ := event["type"].(string)
	if eventType == "account.updated" {
		obj := nestedMap(event, "data", "object")
		accountID, _ := obj["id"].(string)
		chargesEnabled, _ := obj["charges_enabled"].(bool)
		payoutsEnabled, _ := obj["payouts_enabled"].(bool)
		_ = a.billingRepo.UpdateProviderOnboarding(r.Context(), accountID, chargesEnabled && payoutsEnabled)
	}
	if eventType == "invoice.payment_failed" {
		obj := nestedMap(event, "data", "object")
		customerID, _ := obj["customer"].(string)
		_ = a.billingRepo.TouchCustomerByStripeID(r.Context(), customerID)
	}
	w.WriteHeader(http.StatusOK)
}

func nestedMap(root map[string]any, keys ...string) map[string]any {
	cur := root
	for _, key := range keys {
		next, _ := cur[key].(map[string]any)
		cur = next
		if cur == nil {
			return map[string]any{}
		}
	}
	return cur
}

func (a *App) spawnBillingJobs(ctx context.Context) {
	if a.cfg.Stripe == nil {
		return
	}
	go func() {
		ticker := time.NewTicker(time.Hour)
		defer ticker.Stop()
		for {
			select {
			case <-ctx.Done():
				return
			case <-ticker.C:
				_ = a.reportMeterUsage(context.Background())
			}
		}
	}()
	go func() {
		ticker := time.NewTicker(5 * time.Minute)
		defer ticker.Stop()
		var lastDay int
		for {
			select {
			case <-ctx.Done():
				return
			case <-ticker.C:
				now := time.Now().UTC()
				if now.Hour() == 2 && now.YearDay() != lastDay {
					_ = a.processPayouts(context.Background())
					lastDay = now.YearDay()
				}
			}
		}
	}()
}

func (a *App) reportMeterUsage(ctx context.Context) error {
	stripe := newStripeClient(a.cfg.Stripe.SecretKey)
	now := time.Now().UTC().Truncate(time.Hour)
	periodEnd := now
	periodStart := periodEnd.Add(-time.Hour)
	customers, err := a.billingRepo.ListCustomers(ctx)
	if err != nil {
		return err
	}
	for _, customer := range customers {
		exists, err := a.billingRepo.MeterReportExists(ctx, customer.KeyID, periodStart)
		if err != nil || exists {
			continue
		}
		totals, err := a.usageRepo.TotalsForKeyBetween(ctx, customer.KeyID, periodStart, periodEnd)
		if err != nil {
			continue
		}
		weighted := derefInt64(totals.TokensIn) + derefInt64(totals.TokensOut)*2
		if weighted <= 0 {
			continue
		}
		identifier := fmt.Sprintf("meter-%s-%d", customer.KeyID, periodStart.Unix())
		if err := stripe.reportMeterEvent(a.cfg.Stripe.MeterEventName, customer.StripeCustomerID, uint64(weighted), periodStart.Unix(), identifier); err != nil {
			continue
		}
		_ = a.billingRepo.InsertMeterReport(ctx, gwdb.BillingMeterReport{KeyID: customer.KeyID, PeriodStart: periodStart, PeriodEnd: periodEnd, TokensIn: derefInt64(totals.TokensIn), TokensOut: derefInt64(totals.TokensOut), StripeEventID: identifier, ReportedAt: time.Now().UTC()})
	}
	return nil
}

func (a *App) processPayouts(ctx context.Context) error {
	stripe := newStripeClient(a.cfg.Stripe.SecretKey)
	now := time.Now().UTC()
	periodStart := now.Add(-24 * time.Hour)
	providers, err := a.billingRepo.ListPayoutProviders(ctx)
	if err != nil {
		return err
	}
	if len(providers) == 0 {
		return nil
	}
	totals, err := a.usageRepo.TotalsBetween(ctx, periodStart, now)
	if err != nil {
		return err
	}
	weighted := derefInt64(totals.TokensIn) + derefInt64(totals.TokensOut)*2
	if weighted == 0 {
		return nil
	}
	grossUSD := (float64(weighted) * a.cfg.Stripe.TokenRateUSDPer1K) / 1000.0
	providerPoolUSD := grossUSD * (1.0 - a.cfg.Stripe.CommissionRate)
	perProviderCents := uint64((providerPoolUSD / float64(len(providers))) * 100.0)
	if perProviderCents == 0 {
		return nil
	}
	description := fmt.Sprintf("Infer provider earnings %s UTC", periodStart.Format("2006-01-02"))
	for _, provider := range providers {
		_, _ = stripe.createTransfer(perProviderCents, "usd", provider.StripeAccountID, description)
	}
	return nil
}

func derefInt64(v *int64) int64 {
	if v == nil {
		return 0
	}
	return *v
}
