//! Java OAuth/Xbox/Minecraft authentication. No borrowed application ID or Bedrock tokens.
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio_util::sync::CancellationToken;
use zeroize::Zeroize;

const AUTHORITY: &str = "https://login.microsoftonline.com/consumers/oauth2/v2.0";
const SCOPE: &str = "XboxLive.signin offline_access";
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(
        "Configure this fork's registered Microsoft public-client application ID before Java sign-in"
    )]
    MissingClient,
    #[error("Java sign-in was cancelled")]
    Cancelled,
    #[error(
        "Java authentication request failed at {0}; check network access and application approval"
    )]
    Service(&'static str),
    #[error(
        "{0} rejected Java authentication (HTTP {1}); check account eligibility and this application's service access"
    )]
    Rejected(&'static str, u16),
    #[error(
        "Xbox denied Java authorization (account restriction code {0}); check the account's Xbox profile, family and regional requirements"
    )]
    AccountRestricted(u64),
    #[error("Java sign-in expired; start again")]
    Expired,
    #[error("Microsoft sign-in was declined")]
    Declined,
    #[error("This account has no Minecraft Java entitlement/profile")]
    Entitlement,
    #[error("Java credential cache is invalid or expired")]
    Credentials,
}
/// Public sign-in instructions; the device secret is deliberately excluded.
#[derive(Clone, Debug, PartialEq)]
pub struct DevicePrompt {
    pub verification_uri: String,
    pub user_code: String,
    pub expires_in: u64,
}
/// Secret fields are never Debug-formatted. Serialize only through private credential storage.
#[derive(Serialize, Deserialize)]
pub struct Account {
    pub profile: Profile,
    access_token: String,
    refresh_token: String,
    expires_at: u64,
    client_id: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Profile {
    pub id: uuid::Uuid,
    pub name: String,
}
impl Drop for Account {
    fn drop(&mut self) {
        self.access_token.zeroize();
        self.refresh_token.zeroize();
    }
}
impl Account {
    pub fn belongs_to_application(&self, client_id: &str) -> bool {
        self.client_id == client_id
    }
    pub fn expires_at(&self) -> u64 {
        self.expires_at
    }
    pub fn valid_at(&self, now: u64) -> bool {
        self.expires_at > now.saturating_add(60) && !self.access_token.is_empty()
    }
}
/// Service boundary supports independent fixtures without real credentials or network calls.
pub trait Service: Send + Sync {
    fn request(
        &self,
        stage: &'static str,
        url: &str,
        bearer: Option<&str>,
        body: Option<Value>,
        form: bool,
    ) -> impl std::future::Future<Output = Result<Value, Error>> + Send;
}
pub struct HttpService(reqwest::Client);
impl Default for HttpService {
    fn default() -> Self {
        Self(
            reqwest::Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .timeout(Duration::from_secs(30))
                .build()
                .expect("TLS client"),
        )
    }
}
impl Service for HttpService {
    async fn request(
        &self,
        stage: &'static str,
        url: &str,
        bearer: Option<&str>,
        body: Option<Value>,
        form: bool,
    ) -> Result<Value, Error> {
        let mut request = if body.is_some() {
            self.0.post(url)
        } else {
            self.0.get(url)
        };
        if let Some(token) = bearer {
            request = request.bearer_auth(token);
        }
        if let Some(body) = body {
            request = if form {
                request.form(&body)
            } else {
                request.json(&body)
            };
        }
        let mut response = request.send().await.map_err(|_| Error::Service(stage))?;
        let status = response.status();
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|_| Error::Service(stage))? {
            if bytes.len() + chunk.len() > 1024 * 1024 {
                return Err(Error::Service(stage));
            }
            bytes.extend_from_slice(&chunk);
        }
        if status == reqwest::StatusCode::NO_CONTENT {
            return Ok(Value::Null);
        }
        let value: Value = serde_json::from_slice(&bytes).map_err(|_| Error::Service(stage))?;
        // Only OAuth polling errors are admitted as data; never echo response bodies.
        if !status.is_success() && !(stage == "Microsoft token" && value.get("error").is_some()) {
            if stage == "Xbox Java authorization"
                && let Some(code) = value["XErr"].as_u64()
            {
                return Err(Error::AccountRestricted(code));
            }
            return Err(Error::Rejected(stage, status.as_u16()));
        }
        Ok(value)
    }
}
pub struct Client<S = HttpService> {
    client_id: String,
    service: S,
}
impl Client<HttpService> {
    pub fn new(client_id: String) -> Result<Self, Error> {
        Self::with_service(client_id, HttpService::default())
    }
}
impl<S: Service> Client<S> {
    pub fn with_service(client_id: String, service: S) -> Result<Self, Error> {
        if uuid::Uuid::parse_str(&client_id).is_err() {
            return Err(Error::MissingClient);
        }
        Ok(Self { client_id, service })
    }
    /// Device authorization is supported only for this fork's configured public client.
    pub async fn sign_in(
        &self,
        cancel: &CancellationToken,
        prompt: impl FnOnce(DevicePrompt),
    ) -> Result<Account, Error> {
        tokio::select! {biased;_=cancel.cancelled()=>Err(Error::Cancelled),result=self.device_flow(prompt)=>result}
    }
    async fn device_flow(&self, prompt: impl FnOnce(DevicePrompt)) -> Result<Account, Error> {
        let device = self
            .service
            .request(
                "Microsoft device code",
                &format!("{AUTHORITY}/devicecode"),
                None,
                Some(json!({"client_id":self.client_id,"scope":SCOPE})),
                true,
            )
            .await?;
        let secret = zeroize::Zeroizing::new(required(&device, "device_code")?);
        let expires = device["expires_in"]
            .as_u64()
            .filter(|n| *n <= 86400)
            .ok_or(Error::Credentials)?;
        let mut interval = device["interval"].as_u64().unwrap_or(5).clamp(1, 86400);
        prompt(DevicePrompt {
            verification_uri: required(&device, "verification_uri")?,
            user_code: required(&device, "user_code")?,
            expires_in: expires,
        });
        let deadline = tokio::time::Instant::now() + Duration::from_secs(expires);
        loop {
            if tokio::time::Instant::now() + Duration::from_secs(interval) >= deadline {
                return Err(Error::Expired);
            }
            tokio::time::sleep(Duration::from_secs(interval)).await;
            // A request in flight must not extend the device-code lifetime, including
            // after the app resumes from suspension. A valid token ends device polling.
            let token = tokio::time::timeout_at(
                deadline,
                self.service.request(
                    "Microsoft token",
                    &format!("{AUTHORITY}/token"),
                    None,
                    Some(
                        json!({"grant_type":"urn:ietf:params:oauth:grant-type:device_code",
                    "client_id":self.client_id,"device_code":secret.as_str()}),
                    ),
                    true,
                ),
            )
            .await
            .map_err(|_| Error::Expired)??;
            match token["error"].as_str() {
                Some("authorization_pending") => continue,
                Some("slow_down") => {
                    interval += 5;
                    continue;
                }
                Some("authorization_declined" | "access_denied") => return Err(Error::Declined),
                Some("expired_token") => return Err(Error::Expired),
                Some(_) => return Err(Error::Service("Microsoft token")),
                None => return self.exchange(token, None).await,
            }
        }
    }
    /// Refresh is cancellable; failure leaves the caller's existing cache untouched.
    pub async fn refresh(
        &self,
        old: &Account,
        cancel: &CancellationToken,
    ) -> Result<Account, Error> {
        if old.client_id != self.client_id {
            return Err(Error::Credentials);
        }
        tokio::select! {biased;_=cancel.cancelled()=>Err(Error::Cancelled), result=async {
            let token=self.service.request("Microsoft token",&format!("{AUTHORITY}/token"),None,Some(json!({"grant_type":"refresh_token","client_id":self.client_id,"scope":SCOPE,"refresh_token":old.refresh_token})),true).await?;
            let refreshed=self.exchange(token,Some(&old.refresh_token)).await?;
            if refreshed.profile.id != old.profile.id { return Err(Error::Credentials); }
            Ok(refreshed)
        }=>result}
    }
    async fn exchange(
        &self,
        oauth: Value,
        previous_refresh: Option<&str>,
    ) -> Result<Account, Error> {
        let access = required(&oauth, "access_token")?;
        let refresh = oauth["refresh_token"]
            .as_str()
            .or(previous_refresh)
            .ok_or(Error::Credentials)?
            .to_owned();
        let xbl=self.service.request("Xbox user authentication","https://user.auth.xboxlive.com/user/authenticate",None,Some(json!({"Properties":{"AuthMethod":"RPS","SiteName":"user.auth.xboxlive.com","RpsTicket":format!("d={access}")},"RelyingParty":"http://auth.xboxlive.com","TokenType":"JWT"})),false).await?;
        let xsts=self.service.request("Xbox Java authorization","https://xsts.auth.xboxlive.com/xsts/authorize",None,Some(json!({"Properties":{"SandboxId":"RETAIL","UserTokens":[required(&xbl,"Token")?]},"RelyingParty":"rp://api.minecraftservices.com/","TokenType":"JWT"})),false).await?;
        let hash = xsts["DisplayClaims"]["xui"][0]["uhs"]
            .as_str()
            .ok_or(Error::Credentials)?;
        let mc = self
            .service
            .request(
                "Minecraft login",
                "https://api.minecraftservices.com/authentication/login_with_xbox",
                None,
                Some(
                    json!({"identityToken":format!("XBL3.0 x={hash};{}",required(&xsts,"Token")?)}),
                ),
                false,
            )
            .await?;
        let access_token = required(&mc, "access_token")?;
        let entitlements = self
            .service
            .request(
                "Minecraft entitlements",
                "https://api.minecraftservices.com/entitlements/mcstore",
                Some(&access_token),
                None,
                false,
            )
            .await?;
        if !entitlements["items"].as_array().is_some_and(|items| {
            items.iter().any(|i| {
                matches!(
                    i["name"].as_str(),
                    Some("game_minecraft" | "product_minecraft")
                )
            })
        }) {
            return Err(Error::Entitlement);
        }
        let p = self
            .service
            .request(
                "Minecraft Java profile",
                "https://api.minecraftservices.com/minecraft/profile",
                Some(&access_token),
                None,
                false,
            )
            .await?;
        let profile = Profile {
            id: uuid::Uuid::parse_str(&required(&p, "id")?).map_err(|_| Error::Entitlement)?,
            name: required(&p, "name")?,
        };
        let seconds = mc["expires_in"]
            .as_u64()
            .filter(|n| *n > 60 && *n <= 86400)
            .ok_or(Error::Credentials)?;
        Ok(Account {
            profile,
            access_token,
            refresh_token: refresh,
            expires_at: unix_now().saturating_add(seconds),
            client_id: self.client_id.clone(),
        })
    }
}
fn required(value: &Value, key: &str) -> Result<String, Error> {
    value[key]
        .as_str()
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .ok_or(Error::Credentials)
}
pub fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
/// Authorizes exactly this server hash with a Java token, never a Bedrock identity.
pub async fn join(account: &Account, hash: &str) -> Result<(), Error> {
    if !account.valid_at(unix_now()) {
        return Err(Error::Expired);
    }
    HttpService::default().request("Minecraft session join","https://sessionserver.mojang.com/session/minecraft/join",None,Some(json!({"accessToken":account.access_token,"selectedProfile":account.profile.id.simple().to_string(),"serverId":hash})),false).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    struct Fake(Mutex<std::collections::VecDeque<(&'static str, Value)>>);
    impl Service for Fake {
        async fn request(
            &self,
            stage: &'static str,
            _url: &str,
            _bearer: Option<&str>,
            _body: Option<Value>,
            _form: bool,
        ) -> Result<Value, Error> {
            assert!(_url.starts_with("https://"));
            if stage == "Xbox Java authorization" {
                assert_eq!(
                    _body.as_ref().unwrap()["RelyingParty"],
                    "rp://api.minecraftservices.com/"
                );
            }
            if stage == "Minecraft entitlements" || stage == "Minecraft Java profile" {
                assert!(_bearer.is_some());
                assert!(_body.is_none());
            }
            if stage == "Microsoft token" {
                assert!(_form);
                assert_eq!(
                    _body.as_ref().unwrap()["client_id"],
                    "00000000-0000-4000-8000-000000000001"
                );
            }
            let (expected, result) = self
                .0
                .lock()
                .unwrap()
                .pop_front()
                .expect("unexpected service request");
            assert_eq!(stage, expected);
            Ok(result)
        }
    }
    fn client(results: Vec<(&'static str, Value)>) -> Client<Fake> {
        Client::with_service(
            "00000000-0000-4000-8000-000000000001".into(),
            Fake(Mutex::new(results.into())),
        )
        .unwrap()
    }
    #[tokio::test(start_paused = true)]
    async fn device_pending_slowdown_and_java_entitlement_exchange() {
        let client = client(vec![
            (
                "Microsoft device code",
                json!({"device_code":"fixture-secret","user_code":"PUBLIC","verification_uri":"https://microsoft.com/devicelogin","expires_in":900,"interval":1}),
            ),
            ("Microsoft token", json!({"error":"authorization_pending"})),
            ("Microsoft token", json!({"error":"slow_down"})),
            (
                "Microsoft token",
                json!({"access_token":"fixture-microsoft","refresh_token":"fixture-refresh"}),
            ),
            ("Xbox user authentication", json!({"Token":"fixture-xbl"})),
            (
                "Xbox Java authorization",
                json!({"Token":"fixture-xsts","DisplayClaims":{"xui":[{"uhs":"fixture-hash"}]}}),
            ),
            (
                "Minecraft login",
                json!({"access_token":"fixture-java","expires_in":3600}),
            ),
            (
                "Minecraft entitlements",
                json!({"items":[{"name":"game_minecraft"}]}),
            ),
            (
                "Minecraft Java profile",
                json!({"id":"00000000000040008000000000000002","name":"Fixture"}),
            ),
        ]);
        let account = client
            .sign_in(&CancellationToken::new(), |p| {
                assert_eq!(p.user_code, "PUBLIC")
            })
            .await
            .unwrap();
        assert_eq!(account.profile.name, "Fixture");
        assert!(account.valid_at(unix_now()));
        assert!(!account.valid_at(account.expires_at));
        assert!(client.service.0.lock().unwrap().is_empty());
    }
    #[tokio::test]
    async fn cancelled_signin_makes_no_request() {
        let client = client(vec![]);
        let cancel = CancellationToken::new();
        cancel.cancel();
        assert!(matches!(
            client
                .sign_in(&cancel, |_| panic!("cancelled prompt"))
                .await,
            Err(Error::Cancelled)
        ));
    }
    struct StalledToken;
    impl Service for StalledToken {
        async fn request(
            &self,
            stage: &'static str,
            _: &str,
            _: Option<&str>,
            _: Option<Value>,
            _: bool,
        ) -> Result<Value, Error> {
            match stage {
                "Microsoft device code" => Ok(json!({"device_code":"fixture-secret",
                    "user_code":"PUBLIC","verification_uri":"https://microsoft.com/devicelogin",
                    "expires_in":10,"interval":1})),
                "Microsoft token" => std::future::pending().await,
                _ => panic!("expired/cancelled grant must not reach Java services"),
            }
        }
    }

    #[tokio::test(start_paused = true)]
    async fn expiry_interrupts_an_in_flight_poll() {
        let client =
            Client::with_service("00000000-0000-4000-8000-000000000001".into(), StalledToken)
                .unwrap();
        let start = tokio::time::Instant::now();
        assert!(matches!(
            client.sign_in(&CancellationToken::new(), |_| {}).await,
            Err(Error::Expired)
        ));
        assert_eq!(start.elapsed(), Duration::from_secs(10));
    }

    #[tokio::test(start_paused = true)]
    async fn cancellation_after_device_prompt_interrupts_polling() {
        let client =
            Client::with_service("00000000-0000-4000-8000-000000000001".into(), StalledToken)
                .unwrap();
        let cancel = CancellationToken::new();
        let start = tokio::time::Instant::now();
        assert!(matches!(
            client.sign_in(&cancel, |_| cancel.cancel()).await,
            Err(Error::Cancelled)
        ));
        assert_eq!(start.elapsed(), Duration::ZERO);
    }
    #[test]
    fn missing_application_is_actionable() {
        assert!(matches!(
            Client::new(String::new()),
            Err(Error::MissingClient)
        ));
    }
    #[tokio::test(start_paused = true)]
    async fn expired_device_grant_never_reaches_java_services() {
        let client = client(vec![(
            "Microsoft device code",
            json!({"device_code":"fixture","user_code":"PUBLIC","verification_uri":"https://microsoft.com/devicelogin","expires_in":0}),
        )]);
        assert!(matches!(
            client.sign_in(&CancellationToken::new(), |_| {}).await,
            Err(Error::Expired)
        ));
        assert!(client.service.0.lock().unwrap().is_empty());
    }
    #[tokio::test]
    async fn refresh_with_wrong_application_never_sends_credentials() {
        let client = client(vec![]);
        let account = Account {
            profile: Profile {
                id: uuid::Uuid::nil(),
                name: "Fixture".into(),
            },
            access_token: "test-only".into(),
            refresh_token: "test-only".into(),
            expires_at: 0,
            client_id: "different-app".into(),
        };
        assert!(matches!(
            client.refresh(&account, &CancellationToken::new()).await,
            Err(Error::Credentials)
        ));
    }
    #[tokio::test]
    async fn refresh_rotates_tokens_and_keeps_the_java_profile() {
        let client = client(vec![
            (
                "Microsoft token",
                json!({"access_token":"fixture-ms-new","refresh_token":"fixture-rotated"}),
            ),
            ("Xbox user authentication", json!({"Token":"fixture-xbl"})),
            (
                "Xbox Java authorization",
                json!({"Token":"fixture-xsts","DisplayClaims":{"xui":[{"uhs":"fixture-hash"}]}}),
            ),
            (
                "Minecraft login",
                json!({"access_token":"fixture-java-new","expires_in":3600}),
            ),
            (
                "Minecraft entitlements",
                json!({"items":[{"name":"game_minecraft"}]}),
            ),
            (
                "Minecraft Java profile",
                json!({"id":"00000000000040008000000000000002","name":"Fixture"}),
            ),
        ]);
        let old = Account {
            profile: Profile {
                id: uuid::Uuid::parse_str("00000000000040008000000000000002").unwrap(),
                name: "Fixture".into(),
            },
            access_token: "fixture-old".into(),
            refresh_token: "fixture-refresh-old".into(),
            expires_at: 0,
            client_id: client.client_id.clone(),
        };
        let refreshed = client
            .refresh(&old, &CancellationToken::new())
            .await
            .unwrap();
        assert_eq!(refreshed.profile, old.profile);
        assert_eq!(refreshed.refresh_token, "fixture-rotated");
        assert!(refreshed.valid_at(unix_now()));
        assert!(!old.valid_at(unix_now()));
    }
}
