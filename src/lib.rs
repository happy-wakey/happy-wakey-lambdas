//! Provider-neutral Happy Wakey ingestion pipeline.
//!
//! Cloud-specific entry points adapt an authenticated invocation into these
//! types. The core never scrapes providers and never emits raw private content.

use happy_wakey_interfaces::{
    AccountKind, BriefingCard, ChatAudience, ConnectorConsent, ConsentState, SourceItemCandidate,
    UsefulnessDecision, UsefulnessDisposition, UsefulnessReason,
};
use happy_wakey_pub_lib_core::authorize_deep_link;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use syncer_rs::{MergeOptions, merge_json};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

const REQUIRED_MIDDLEWARE_CAPABILITIES: &[&str] = &[
    "request-context",
    "trace-context",
    "payload-limit",
    "rate-limit",
    "auth",
    "sync-observer",
    "tls-policy",
    "idempotency",
];

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SecretBearer(String);

impl SecretBearer {
    /// Accept a bounded bearer for immediate verification.
    ///
    /// # Errors
    ///
    /// Returns an error when the value is empty, oversized, or contains
    /// whitespace/control characters.
    pub fn new(value: String) -> Result<Self, PipelineError> {
        if value.is_empty()
            || value.len() > 16 * 1024
            || value.chars().any(char::is_whitespace)
            || value.chars().any(char::is_control)
        {
            return Err(PipelineError::InvalidCredential);
        }
        Ok(Self(value))
    }

    fn expose_for_verification(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuthenticatedInvocation {
    pub request_id: String,
    pub trace_id: String,
    pub tenant_id: String,
    pub subject_id: String,
    pub account_kind: AccountKind,
    pub issuer: String,
    pub audience: String,
    pub expires_at_unix: i64,
}

#[derive(Clone, Debug)]
pub struct IngestRequest {
    pub bearer: SecretBearer,
    pub consent: ConnectorConsent,
    pub candidate: SourceItemCandidate,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ClassifiedCandidate {
    pub decision: UsefulnessDecision,
    pub card: Option<BriefingCard>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IngestDisposition {
    Published,
    Suppressed,
    NeedsReview,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatHandoff {
    pub tenant_id: String,
    pub audience: ChatAudience,
    pub card_id: String,
    pub reason: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IngestOutcome {
    pub disposition: IngestDisposition,
    pub decision_id: String,
    pub publish_subject: Option<String>,
    pub card: Option<BriefingCard>,
    pub chat_handoff: Option<ChatHandoff>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RateLimitGrant {
    pub allowed: bool,
    pub policy_version: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TelemetryEvent {
    pub event_name: &'static str,
    pub request_id: String,
    pub trace_id: String,
    pub account_kind: AccountKind,
    pub disposition: IngestDisposition,
}

pub trait SharedAuthPort {
    type Error;

    /// Verify a transient bearer and return server-derived identity.
    ///
    /// # Errors
    ///
    /// Returns the adapter error when the authority cannot verify the bearer.
    fn verify(&self, bearer: &str) -> Result<AuthenticatedInvocation, Self::Error>;
}

pub trait OresRateLimitPort {
    type Error;

    /// Ask the Ores Rate Limit authority for an explicit grant.
    ///
    /// # Errors
    ///
    /// Returns the adapter error when the authority cannot decide.
    fn check(&self, invocation: &AuthenticatedInvocation) -> Result<RateLimitGrant, Self::Error>;
}

pub trait UsefulnessClassifierPort {
    type Error;

    /// Classify one opaque, consented source candidate.
    ///
    /// # Errors
    ///
    /// Returns the adapter error when classification is unavailable.
    fn classify(
        &self,
        invocation: &AuthenticatedInvocation,
        candidate: &SourceItemCandidate,
    ) -> Result<ClassifiedCandidate, Self::Error>;
}

pub trait OresTelemetryPort {
    type Error;

    /// Record an allowlisted event that contains no source message or token.
    ///
    /// # Errors
    ///
    /// Returns the adapter error when the event is not durably accepted.
    fn record(&self, event: TelemetryEvent) -> Result<(), Self::Error>;
}

#[derive(Clone, Copy, Debug)]
pub struct PipelinePolicy<'a> {
    pub expected_issuer: &'a str,
    pub expected_audience: &'a str,
    pub now: OffsetDateTime,
}

pub struct PipelinePorts<'a, A, R, C, T> {
    pub auth: &'a A,
    pub rate_limit: &'a R,
    pub classifier: &'a C,
    pub telemetry: &'a T,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum PipelineError {
    #[error("credential is invalid")]
    InvalidCredential,
    #[error("shared auth could not verify the invocation")]
    AuthenticationUnavailable,
    #[error("shared auth claims do not satisfy the lambda policy")]
    Unauthorized,
    #[error("connector consent is absent, expired, revoked, or inconsistent")]
    ConsentDenied,
    #[error("rate-limit authority could not grant the operation")]
    RateLimited,
    #[error("usefulness classification failed closed")]
    ClassificationUnavailable,
    #[error("classifier output does not match the source candidate")]
    InvalidClassification,
    #[error("the proposed social action is not a safe item-specific deep link")]
    UnsafeDeepLink,
    #[error("middleware contract is incomplete")]
    MiddlewareContract,
    #[error("telemetry sink could not record the bounded outcome")]
    TelemetryUnavailable,
    #[error("preference merge failed")]
    SyncMerge,
}

/// Validate the linked Ores Middleware contract before accepting traffic.
///
/// # Errors
///
/// Fails when a required production boundary is missing.
pub fn validate_middleware_contract() -> Result<(), PipelineError> {
    let capabilities = ores_middleware::capabilities();
    if REQUIRED_MIDDLEWARE_CAPABILITIES
        .iter()
        .all(|required| capabilities.contains(required))
    {
        Ok(())
    } else {
        Err(PipelineError::MiddlewareContract)
    }
}

/// Process one consented source item into a card, suppression, or review item.
///
/// # Errors
///
/// Authentication, consent, rate limiting, classification, deep-link policy,
/// middleware, and telemetry all fail closed.
pub fn process_candidate<A, R, C, T>(
    request: &IngestRequest,
    policy: PipelinePolicy<'_>,
    ports: &PipelinePorts<'_, A, R, C, T>,
) -> Result<IngestOutcome, PipelineError>
where
    A: SharedAuthPort,
    R: OresRateLimitPort,
    C: UsefulnessClassifierPort,
    T: OresTelemetryPort,
{
    validate_middleware_contract()?;
    let invocation = ports
        .auth
        .verify(request.bearer.expose_for_verification())
        .map_err(|_| PipelineError::AuthenticationUnavailable)?;
    validate_invocation(
        &invocation,
        policy.expected_issuer,
        policy.expected_audience,
        policy.now,
    )?;
    validate_consent(&request.consent, &request.candidate, policy.now)?;
    let grant = ports
        .rate_limit
        .check(&invocation)
        .map_err(|_| PipelineError::RateLimited)?;
    if !grant.allowed || grant.policy_version.is_empty() {
        return Err(PipelineError::RateLimited);
    }
    let classification = ports
        .classifier
        .classify(&invocation, &request.candidate)
        .map_err(|_| PipelineError::ClassificationUnavailable)?;
    validate_classification(&classification, &request.candidate, policy.now)?;

    let disposition = match classification.decision.disposition {
        UsefulnessDisposition::Useful => IngestDisposition::Published,
        UsefulnessDisposition::NotUseful => IngestDisposition::Suppressed,
        UsefulnessDisposition::NeedsReview => IngestDisposition::NeedsReview,
    };
    let card = (disposition == IngestDisposition::Published)
        .then_some(classification.card)
        .flatten();
    let publish_subject = card
        .as_ref()
        .map(|_| format!("happywakey.briefing.{}.candidate", invocation.tenant_id));
    let chat_handoff = card.as_ref().and_then(|card| {
        support_reason(&classification.decision).map(|reason| ChatHandoff {
            tenant_id: invocation.tenant_id.clone(),
            audience: ChatAudience::CustomerSupport,
            card_id: card.card_id.clone(),
            reason: reason.to_owned(),
        })
    });
    let outcome = IngestOutcome {
        disposition,
        decision_id: classification.decision.decision_id,
        publish_subject,
        card,
        chat_handoff,
    };
    ports
        .telemetry
        .record(TelemetryEvent {
            event_name: "happy_wakey_candidate_processed",
            request_id: invocation.request_id,
            trace_id: invocation.trace_id,
            account_kind: invocation.account_kind,
            disposition,
        })
        .map_err(|_| PipelineError::TelemetryUnavailable)?;
    Ok(outcome)
}

fn validate_invocation(
    invocation: &AuthenticatedInvocation,
    expected_issuer: &str,
    expected_audience: &str,
    now: OffsetDateTime,
) -> Result<(), PipelineError> {
    if invocation.request_id.is_empty()
        || invocation.trace_id.len() != 32
        || !safe_scope(&invocation.tenant_id)
        || !safe_scope(&invocation.subject_id)
        || invocation.issuer != expected_issuer
        || invocation.audience != expected_audience
        || invocation.expires_at_unix <= now.unix_timestamp()
    {
        return Err(PipelineError::Unauthorized);
    }
    Ok(())
}

fn validate_consent(
    consent: &ConnectorConsent,
    candidate: &SourceItemCandidate,
    now: OffsetDateTime,
) -> Result<(), PipelineError> {
    let valid_expiry = consent.expires_at.as_deref().is_none_or(|expiry| {
        OffsetDateTime::parse(expiry, &Rfc3339).is_ok_and(|expires| expires > now)
    });
    if consent.state != ConsentState::Granted
        || consent.connector != candidate.connector
        || consent.scopes.is_empty()
        || !valid_expiry
    {
        return Err(PipelineError::ConsentDenied);
    }
    Ok(())
}

fn validate_classification(
    classified: &ClassifiedCandidate,
    candidate: &SourceItemCandidate,
    now: OffsetDateTime,
) -> Result<(), PipelineError> {
    if classified.decision.source_item_ref != candidate.source_item_ref
        || classified.decision.content_sha256 != candidate.content_sha256
    {
        return Err(PipelineError::InvalidClassification);
    }
    match classified.decision.disposition {
        UsefulnessDisposition::Useful => {
            let card = classified
                .card
                .as_ref()
                .ok_or(PipelineError::InvalidClassification)?;
            if card.deep_link.is_some() && authorize_deep_link(card, now).is_err() {
                return Err(PipelineError::UnsafeDeepLink);
            }
        }
        UsefulnessDisposition::NotUseful | UsefulnessDisposition::NeedsReview => {
            if classified.card.is_some() {
                return Err(PipelineError::InvalidClassification);
            }
        }
    }
    Ok(())
}

fn support_reason(decision: &UsefulnessDecision) -> Option<&'static str> {
    if decision.reasons.contains(&UsefulnessReason::SecurityRisk) {
        Some("security_risk")
    } else if decision
        .reasons
        .contains(&UsefulnessReason::CustomerEscalation)
    {
        Some("customer_escalation")
    } else {
        None
    }
}

fn safe_scope(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

/// Reconcile user briefing preferences through Opto Sync's deterministic core.
///
/// # Errors
///
/// Returns an error when either document or the merge result is invalid.
pub fn merge_preferences(base: &str, incoming: &str) -> Result<Value, PipelineError> {
    let merged = merge_json(base, incoming, &MergeOptions::default())
        .map_err(|_| PipelineError::SyncMerge)?;
    serde_json::from_str(&merged).map_err(|_| PipelineError::SyncMerge)
}

#[cfg(test)]
mod tests {
    use super::*;
    use happy_wakey_interfaces::{
        BriefingCardKind, BriefingCardPriority, ConnectorKind, SafeDeepLink, SenderClass,
    };
    use std::cell::RefCell;

    fn now() -> OffsetDateTime {
        OffsetDateTime::parse("2026-09-05T12:00:00Z", &Rfc3339).expect("time")
    }

    fn request() -> IngestRequest {
        IngestRequest {
            bearer: SecretBearer::new("opaque-token".into()).expect("bearer"),
            consent: ConnectorConsent {
                consent_id: "consent-1".into(),
                connector: ConnectorKind::Linkedin,
                state: ConsentState::Granted,
                scopes: vec!["messages.read".into()],
                granted_at: Some("2026-09-01T00:00:00Z".into()),
                expires_at: Some("2026-09-06T00:00:00Z".into()),
                source_account_ref: "account-1".into(),
            },
            candidate: SourceItemCandidate {
                source_item_ref: "source-1".into(),
                connector: ConnectorKind::Linkedin,
                sender_class: SenderClass::Vip,
                received_at: "2026-09-05T11:00:00Z".into(),
                thread_ref: "thread-1".into(),
                encrypted_content_ref: "vault/item-1".into(),
                content_sha256: "a".repeat(64),
                has_direct_reply_request: true,
                due_at: Some("2026-09-05T15:00:00Z".into()),
            },
        }
    }

    struct Auth;
    impl SharedAuthPort for Auth {
        type Error = ();
        fn verify(&self, bearer: &str) -> Result<AuthenticatedInvocation, Self::Error> {
            assert_eq!(bearer, "opaque-token");
            Ok(AuthenticatedInvocation {
                request_id: "request-1".into(),
                trace_id: "0123456789abcdef0123456789abcdef".into(),
                tenant_id: "tenant-1".into(),
                subject_id: "subject-1".into(),
                account_kind: AccountKind::OrganizationMember,
                issuer: "https://auth.hawky.pro".into(),
                audience: "hawky-api".into(),
                expires_at_unix: now().unix_timestamp() + 60,
            })
        }
    }

    struct Limiter(bool);
    impl OresRateLimitPort for Limiter {
        type Error = ();
        fn check(&self, _: &AuthenticatedInvocation) -> Result<RateLimitGrant, Self::Error> {
            Ok(RateLimitGrant {
                allowed: self.0,
                policy_version: "rate-v1".into(),
            })
        }
    }

    struct Classifier;
    impl UsefulnessClassifierPort for Classifier {
        type Error = ();
        fn classify(
            &self,
            _: &AuthenticatedInvocation,
            candidate: &SourceItemCandidate,
        ) -> Result<ClassifiedCandidate, Self::Error> {
            let decision = UsefulnessDecision {
                decision_id: "decision-1".into(),
                source_item_ref: candidate.source_item_ref.clone(),
                disposition: UsefulnessDisposition::Useful,
                score: 0.96,
                reasons: vec![UsefulnessReason::CustomerEscalation],
                model_ref: "classifier-1".into(),
                policy_version: "usefulness-v1".into(),
                evaluated_at: "2026-09-05T11:01:00Z".into(),
                content_sha256: candidate.content_sha256.clone(),
            };
            Ok(ClassifiedCandidate {
                card: Some(BriefingCard {
                    card_id: "card-1".into(),
                    kind: BriefingCardKind::UsefulMessage,
                    priority: BriefingCardPriority::High,
                    title: "Customer response needed".into(),
                    summary: "A redacted, actionable summary".into(),
                    source_label: "LinkedIn message".into(),
                    observed_at: "2026-09-05T11:00:00Z".into(),
                    action_by: candidate.due_at.clone(),
                    deep_link: Some(SafeDeepLink {
                        link_id: "link-1".into(),
                        connector: ConnectorKind::Linkedin,
                        target_url: "https://www.linkedin.com/messaging/thread/example".into(),
                        decision_id: decision.decision_id.clone(),
                        source_item_ref: candidate.source_item_ref.clone(),
                        expires_at: "2026-09-05T14:00:00Z".into(),
                        requires_reauthentication: true,
                        feed_fallback_allowed: false,
                    }),
                    usefulness: Some(decision.clone()),
                    uncertainty_notice: None,
                }),
                decision,
            })
        }
    }

    #[derive(Default)]
    struct Telemetry(RefCell<Vec<TelemetryEvent>>);
    impl OresTelemetryPort for Telemetry {
        type Error = ();
        fn record(&self, event: TelemetryEvent) -> Result<(), Self::Error> {
            self.0.borrow_mut().push(event);
            Ok(())
        }
    }

    #[test]
    fn publishes_useful_item_and_prepares_bounded_chat_handoff() {
        let telemetry = Telemetry::default();
        let outcome = process_candidate(
            &request(),
            PipelinePolicy {
                expected_issuer: "https://auth.hawky.pro",
                expected_audience: "hawky-api",
                now: now(),
            },
            &PipelinePorts {
                auth: &Auth,
                rate_limit: &Limiter(true),
                classifier: &Classifier,
                telemetry: &telemetry,
            },
        )
        .expect("published");
        assert_eq!(outcome.disposition, IngestDisposition::Published);
        assert_eq!(
            outcome.publish_subject.as_deref(),
            Some("happywakey.briefing.tenant-1.candidate")
        );
        assert_eq!(
            outcome.chat_handoff.expect("handoff").reason,
            "customer_escalation"
        );
        assert_eq!(telemetry.0.borrow().len(), 1);
    }

    #[test]
    fn rate_limit_and_revoked_consent_fail_closed() {
        assert_eq!(
            process_candidate(
                &request(),
                PipelinePolicy {
                    expected_issuer: "https://auth.hawky.pro",
                    expected_audience: "hawky-api",
                    now: now(),
                },
                &PipelinePorts {
                    auth: &Auth,
                    rate_limit: &Limiter(false),
                    classifier: &Classifier,
                    telemetry: &Telemetry::default(),
                },
            ),
            Err(PipelineError::RateLimited)
        );
        let mut revoked = request();
        revoked.consent.state = ConsentState::Revoked;
        assert_eq!(
            process_candidate(
                &revoked,
                PipelinePolicy {
                    expected_issuer: "https://auth.hawky.pro",
                    expected_audience: "hawky-api",
                    now: now(),
                },
                &PipelinePorts {
                    auth: &Auth,
                    rate_limit: &Limiter(true),
                    classifier: &Classifier,
                    telemetry: &Telemetry::default(),
                },
            ),
            Err(PipelineError::ConsentDenied)
        );
    }

    #[test]
    fn opto_sync_owns_preference_reconciliation() {
        assert_eq!(
            merge_preferences(
                r#"{"theme":"dark","lanes":{"weather":true}}"#,
                r#"{"lanes":{"markets":true}}"#
            )
            .expect("merged"),
            serde_json::json!({"theme":"dark","lanes":{"weather":true,"markets":true}})
        );
    }

    #[test]
    fn middleware_contract_contains_required_production_boundaries() {
        validate_middleware_contract().expect("contract");
    }
}
