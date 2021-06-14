mod error;

use reqwest::Method;
use reqwest::header;
use reqwest::Client as HttpClient;
use reqwest::{StatusCode};
use serde::{Deserialize, Serialize};
// use tokio_stream::{Stream, StreamExt};
use tracing::{info, instrument};
use url::Url;
use uuid::Uuid;

pub use error::ClientError;

/// A shorthand `Result` type that always uses `ClientError` as its error variant
pub type Result<T> = std::result::Result<T, ClientError>;

const JSON_MIME_TYPE: &str = "application/json";

/// A client type for interacting with a Hippo server
#[derive(Clone)]
pub struct Client {
    client: HttpClient,
    base_url: Url,
    auth_token: String,
}

pub struct ClientOptions {
    pub danger_accept_invalid_certs: bool,
}

impl Default for ClientOptions {
    fn default() -> Self {
        Self {
            danger_accept_invalid_certs: false,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct CreateTokenResponse {
    token: String,
    expiration: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct RegisterRevisionRequest {
    app_id: Option<String>,  // Uuid would be better but gives serialisation errors that I am not interested in looking into right now
    app_storage_id: Option<String>,
    revision_number: String,
}

impl RegisterRevisionRequest {
    pub fn for_app(app_id: impl Into<String>, revision_number: impl Into<String>) -> Self {
        Self {
            app_id: Some(app_id.into()),
            app_storage_id: None,
            revision_number: revision_number.into(),
        }
    }

    pub fn for_bindle_name(bindle_name: impl Into<String>, revision_number: impl Into<String>) -> Self {
        Self {
            app_id: None,
            app_storage_id: Some(bindle_name.into()),
            revision_number: revision_number.into(),
        }
    }
}

impl Client {
    /// Returns a new Client with the given URL.
    pub async fn new(base_url: &str, username: &str, password: &str) -> Result<Self> {
        Self::new_with_options(base_url, username, password, ClientOptions::default()).await
    }

    /// Returns a new Client with the given URL.
    pub async fn new_with_options(base_url: &str, username: &str, password: &str, options: ClientOptions) -> Result<Self> {
        // Note that the trailing slash is important, otherwise the URL parser will treat is as a
        // "file" component of the URL. So we need to check that it is added before parsing
        let mut base = base_url.to_owned();
        if !base.ends_with('/') {
            info!("Provided base URL missing trailing slash, adding...");
            base.push('/');
        }
        let base_parsed = Url::parse(&base)?;
        let mut headers = header::HeaderMap::new();
        headers.insert(header::ACCEPT, JSON_MIME_TYPE.parse().unwrap());
        headers.insert(header::CONTENT_TYPE, JSON_MIME_TYPE.parse().unwrap());
        // TODO: As this evolves, we might want to allow for setting timeouts etc.
        let client = HttpClient::builder()
            // .http2_prior_knowledge()
            .and_if(options.danger_accept_invalid_certs, |b| b.danger_accept_invalid_certs(true))
            .default_headers(headers)
            .build()
            .map_err(|e| ClientError::Other(e.to_string()))?;
        let base_url = base_parsed;
        let auth_token = Self::create_token(&client, &base_url, username, password).await?;
        Ok(Client {
            client,
            base_url,
            auth_token,
        })
    }

    /// Performs a raw request using the underlying HTTP client and returns the raw response. The
    /// path is just the path part of your URL. It will be joined with the configured base URL for
    /// the client.
    #[instrument(level = "trace", skip(self, body))]
    pub async fn raw(
        &self,
        method: reqwest::Method,
        path: &str,
        body: Option<String>,
    ) -> anyhow::Result<reqwest::Response> {
        let req = self.client
            .request(method, self.base_url.join(path)?)
            .bearer_auth(&self.auth_token);

        let req = match body {
            Some(b) => {
                req.header(header::CONTENT_LENGTH, b.as_bytes().len())
                   .body(b.clone())
            },
            None => {
                req.header(header::CONTENT_LENGTH, 0)
            },
        };

        req.send().await.map_err(|e| e.into())
    }

    #[instrument(level = "trace", skip(password))]
    async fn create_token(
        client: &HttpClient,
        base_url: &Url,
        username: &str,
        password: &str,
    ) -> Result<String> {
        let body = format!("{{ \"username\": \"{}\", \"password\": \"{}\" }}", username, password);
        let req = client.request(Method::POST, base_url.join("account/createtoken")?)
            .body(body);
        let response = req.send().await.map_err(|e| ClientError::HttpClientError(e))?;
        let response_body = response.bytes().await?;
        let token_response: CreateTokenResponse = serde_json::from_slice(&response_body).map_err(|e| ClientError::SerializationError(e))?;
        Ok(token_response.token)
    }

    //////////////// Register Revision ////////////////

    /// Registers the given revision
    #[instrument(level = "trace", skip(self, revision_number), fields(revision_number = %revision_number))]
    pub async fn register_revision_by_application(
        &self,
        application_id: &Uuid,
        revision_number: &str,
    ) -> Result<()> {
        let path = "api/revision";
        let request = RegisterRevisionRequest::for_app(application_id.to_string(), revision_number);
        let request_json = serde_json::to_string(&request).map_err(|e| ClientError::SerializationError(e))?;
        let response = self.raw(Method::POST, &path, Some(request_json)).await.map_err(|e| ClientError::Other(format!("{}", e)))?;
        if response.status() == StatusCode::CREATED {
            Ok(())
        } else {
            Err(ClientError::InvalidRequest { status_code: response.status(), message: Some(core::str::from_utf8(&response.bytes().await.unwrap()).unwrap().to_owned()) })
        }
    }

    /// Registers the given revision
    #[instrument(level = "trace", skip(self, revision_number), fields(revision_number = %revision_number))]
    pub async fn register_revision_by_storage_id(
        &self,
        bindle_name: &str,
        revision_number: &str,
    ) -> Result<()> {
        let path = "api/revision";
        let request = RegisterRevisionRequest::for_bindle_name(bindle_name, revision_number);
        let request_json = serde_json::to_string(&request).map_err(|e| ClientError::SerializationError(e))?;
        let response = self.raw(Method::POST, &path, Some(request_json)).await.map_err(|e| ClientError::Other(format!("{}", e)))?;
        if response.status() == StatusCode::CREATED {
            Ok(())
        } else {
            Err(ClientError::InvalidRequest { status_code: response.status(), message: Some(core::str::from_utf8(&response.bytes().await.unwrap()).unwrap().to_owned()) })
        }
    }
}

trait ConditionalBuilder {
    fn and_if(self, condition: bool, build_method: impl Fn(Self) -> Self) -> Self
    where
        Self: Sized
    {
        if condition {
            build_method(self)
        } else {
            self
        }
    }
}

impl ConditionalBuilder for reqwest::ClientBuilder {}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn can_log_in() -> Result<()> {
        let options = ClientOptions { danger_accept_invalid_certs: true };
        let client = Client::new_with_options("https://localhost:5001/", "admin", "Passw0rd!", options).await?;
        client.register_revision_by_storage_id("hippos.rocks/helloworld", "1.1.1").await?;
        Ok(())
    }
}
