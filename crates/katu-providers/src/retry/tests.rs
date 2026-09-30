//! Testes da política de retry (E12-T04/T06).

use std::time::Duration;

use super::{RetryPolicy, body_retry_after, delay, is_retryable, retry_after};

fn headers(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
    pairs
        .iter()
        .map(|(key, value)| ((*key).to_string(), (*value).to_string()))
        .collect()
}

#[test]
fn transient_statuses_retry_but_account_limits_do_not() {
    assert!(is_retryable(429, &[], "rate limited"));
    assert!(is_retryable(503, &[], "overloaded"));
    assert!(!is_retryable(
        429,
        &[],
        r#"{"error":{"type":"FreeTierError"}}"#
    ));
    assert!(!is_retryable(400, &[], "bad request"));
    assert!(!is_retryable(
        500,
        &headers(&[("x-should-retry", "false")]),
        ""
    ));
    assert!(is_retryable(
        418,
        &headers(&[("x-should-retry", "true")]),
        ""
    ));
}

#[test]
fn retry_after_prefers_millis_and_caps_at_the_policy_ceiling() {
    assert_eq!(
        retry_after(&headers(&[("Retry-After", "2")])),
        Some(Duration::from_secs(2))
    );
    assert_eq!(
        retry_after(&headers(&[("retry-after-ms", "1500")])),
        Some(Duration::from_millis(1500))
    );
    let policy = RetryPolicy {
        max_delay: Duration::from_secs(5),
        ..RetryPolicy::default()
    };
    assert_eq!(
        delay(&policy, 0, Some(Duration::from_secs(60))),
        Duration::from_secs(5)
    );
}

#[test]
fn retry_after_parses_http_dates_against_the_response_date() {
    let pairs = headers(&[
        ("date", "Sun, 06 Nov 1994 08:49:37 GMT"),
        ("retry-after", "Sun, 06 Nov 1994 08:49:42 GMT"),
    ]);
    assert_eq!(retry_after(&pairs), Some(Duration::from_secs(5)));
    let capped = headers(&[
        ("date", "Sun, 06 Nov 1994 08:49:37 GMT"),
        ("retry-after", "Mon, 07 Nov 1994 08:49:37 GMT"),
    ]);
    assert_eq!(retry_after(&capped), Some(Duration::from_secs(3600)));
    // Sem `Date`, a data `HTTP` é ignorada (o provider não consulta o relógio).
    assert_eq!(
        retry_after(&headers(&[(
            "retry-after",
            "Sun, 06 Nov 1994 08:49:42 GMT"
        )])),
        None
    );
}

#[test]
fn body_retry_after_reads_openrouter_metadata() {
    assert_eq!(
        body_retry_after(r#"{"error":{"metadata":{"retry_after_seconds":22.5}}}"#),
        Some(Duration::from_secs_f64(22.5))
    );
    assert_eq!(
        body_retry_after(r#"{"error":{"metadata":{"retry_after_seconds":-1}}}"#),
        None
    );
    assert_eq!(body_retry_after("not json"), None);
    assert_eq!(body_retry_after(r#"{"error":{}}"#), None);
}

#[test]
fn exponential_backoff_is_bounded() {
    let policy = RetryPolicy {
        max_retries: 5,
        base_delay: Duration::from_millis(100),
        max_delay: Duration::from_millis(250),
    };
    assert_eq!(delay(&policy, 0, None), Duration::from_millis(100));
    assert_eq!(delay(&policy, 1, None), Duration::from_millis(200));
    assert_eq!(delay(&policy, 2, None), Duration::from_millis(250));
}
