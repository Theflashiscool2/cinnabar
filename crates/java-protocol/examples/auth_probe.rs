//! Interactive, token-redacted acceptance using the production authentication exchange.
//! Takes a public application ID; keeps credentials in memory only.
use java_protocol::auth::{Client, Error, HttpService, Service};
use serde_json::Value;
use std::io::Write;

struct Observed(HttpService);
impl Service for Observed {
    async fn request(
        &self,
        stage: &'static str,
        url: &str,
        bearer: Option<&str>,
        body: Option<Value>,
        form: bool,
    ) -> Result<Value, Error> {
        let _ = writeln!(std::io::stderr(), "Starting: {stage}");
        let result = self.0.request(stage, url, bearer, body, form).await;
        match &result {
            Ok(_) => {
                let _ = writeln!(std::io::stderr(), "Response received: {stage}");
            }
            Err(error) => {
                let _ = writeln!(std::io::stderr(), "{error}");
            }
        }
        result
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let id = std::env::args()
        .nth(1)
        .ok_or("usage: auth_probe PUBLIC_APPLICATION_ID")?;
    let client = Client::with_service(id, Observed(HttpService::default()))?;
    let account = client
        .sign_in(&Default::default(), |prompt| {
            let _ = writeln!(
                std::io::stdout(),
                "Open {} and enter {} (expires in {}s). Do not share this code.",
                prompt.verification_uri,
                prompt.user_code,
                prompt.expires_in
            );
        })
        .await
        .map_err(|error| std::io::Error::other(error.to_string()))?;
    // Credentials are dropped and zeroized; never print or persist them in this probe.
    let _ = writeln!(
        std::io::stdout(),
        "PASS: Java login, entitlement and profile verified; no credentials saved."
    );
    drop(account);
    Ok(())
}
