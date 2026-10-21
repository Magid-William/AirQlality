//! The shared blocking HTTP client for the real sources (EXP19).
//!
//! `ureq`'s `native-tls` feature only provides the `TlsConnector` impl; it never
//! wires a backend by default, so every source must build a connector and hand
//! it to the agent. On this GNU/MinGW build `native-tls` uses the Windows
//! schannel stack (no C toolchain, unlike rustls/ring).
//!
//! Every error string is **scrubbed of secrets** before it leaves this module:
//! `ureq` puts the request URL in its transport errors, and OpenWeatherMap takes
//! its API key as a query parameter, so a raw error would otherwise print the
//! user's key into the widget's "offline" note.

use std::sync::Arc;
use std::time::Duration;

use crate::source::SourceError;

/// A `ureq` agent with native TLS and a fail-fast timeout, so a source never
/// hangs the background fetch.
pub(crate) fn agent(timeout: Duration) -> Result<ureq::Agent, SourceError> {
    let tls = native_tls::TlsConnector::new()
        .map_err(|e| SourceError::Network(format!("native TLS init failed: {e}")))?;
    Ok(ureq::AgentBuilder::new()
        .timeout(timeout)
        .tls_connector(Arc::new(tls))
        .build())
}

/// `GET` a URL and return the body as a string, with any occurrence of
/// `secrets` replaced by `***` in an error message, so a key can never reach the
/// UI, a log or a screenshot.
pub(crate) fn get_redacted(
    agent: &ureq::Agent,
    url: &str,
    headers: &[(&str, &str)],
    secrets: &[&str],
) -> Result<String, SourceError> {
    let mut req = agent.get(url);
    for (name, value) in headers {
        req = req.set(name, value);
    }
    req.call()
        .map_err(|e| match &e {
            // 401/403 is the provider refusing the key, not the network.
            ureq::Error::Status(code, _) if *code == 401 || *code == 403 => {
                SourceError::Rejected(format!("HTTP {code}"))
            }
            _ => SourceError::Network(describe(&e, secrets)),
        })?
        .into_string()
        .map_err(|e| SourceError::Network(scrub(&e.to_string(), secrets)))
}

/// A short, user-facing description of a transport failure.
///
/// A bare `ureq` error embeds the **whole request URL**, which is both noisy in
/// the widget's stale note and the single largest place a key could leak from.
/// A status code is reported as a status code; only genuine transport failures
/// keep their text (scrubbed).
pub(crate) fn describe(error: &ureq::Error, secrets: &[&str]) -> String {
    match error {
        ureq::Error::Status(code, _) => format!("HTTP {code}"),
        ureq::Error::Transport(t) => scrub(&t.to_string(), secrets),
    }
}

/// Replace every non-empty secret in `text` with `***`.
pub(crate) fn scrub(text: &str, secrets: &[&str]) -> String {
    let mut out = text.to_string();
    for secret in secrets {
        if !secret.is_empty() {
            out = out.replace(secret, "***");
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scrub_removes_every_secret() {
        let url = "https://api.openweathermap.org/data/2.5/air_pollution?lat=1&appid=KEY123";
        assert_eq!(
            scrub(url, &["KEY123"]),
            "https://api.openweathermap.org/data/2.5/air_pollution?lat=1&appid=***"
        );
        // Empty secrets are ignored (never turn the message into swiss cheese).
        assert_eq!(scrub("abc", &[""]), "abc");
    }

    #[test]
    fn scrub_handles_a_key_that_appears_twice() {
        assert_eq!(scrub("k-k", &["k"]), "***-***");
    }
}
