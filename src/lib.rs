mod error;

use reqwest::Method;
use reqwest::header;
use reqwest::Client as HttpClient;
use reqwest::{Body, RequestBuilder, StatusCode};
use serde::{Deserialize, Serialize};
use tokio_stream::{Stream, StreamExt};
use tracing::{debug, info, instrument, trace};
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

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct CreateTokenResponse {
    token: String,
    expiration: String,
}

impl Client {
    /// Returns a new Client with the given URL.
    pub async fn new_from_login(base_url: &str, username: &str, password: &str) -> Result<Self> {
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
        // TODO: As this evolves, we might want to allow for setting time outs and accepting
        // self-signed certs
        let client = HttpClient::builder()
            // .http2_prior_knowledge()
            .default_headers(headers)
            // TODO: Awoogah, Dave. There's an emergency.
            .danger_accept_invalid_certs(true)
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
        body: Option<impl Into<reqwest::Body>>,
    ) -> anyhow::Result<reqwest::Response> {
        let req = self.client
            .request(method, self.base_url.join(path)?)
            .bearer_auth(&self.auth_token);

        let req = match body {
            Some(b) => req.body(b),
            None => req,
        };

        let req = req.header(header::CONTENT_LENGTH, 0);  // TODO: WTF
        // println!("{:?}", &req);
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
        let token_response: CreateTokenResponse = serde_json::from_slice(&response_body).map_err(|e| ClientError::DeserializationError(e))?;
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
        let full_path = format!("api/revision?AppId={}&RevisionNumber={}", application_id, revision_number);
        let response = self.raw(Method::POST, &full_path, Option::<String>::None).await.map_err(|e| ClientError::Other(format!("{}", e)))?;
        if response.status() == StatusCode::OK {
            Ok(())
        } else {
            Err(ClientError::InvalidRequest { status_code: response.status(), message: Some(core::str::from_utf8(&response.bytes().await.unwrap()).unwrap().to_owned()) })
        }
    }

    /// Registers the given revision
    #[instrument(level = "trace", skip(self, revision_number), fields(revision_number = %revision_number))]
    pub async fn register_revision_by_storage_id(
        &self,
        storage_id: &str,
        revision_number: &str,
    ) -> Result<()> {
        let full_path = format!("api/revision?AppStorageId={}&RevisionNumber={}", storage_id, revision_number);
        let response = self.raw(Method::POST, &full_path, Option::<String>::None).await.map_err(|e| ClientError::Other(format!("{}", e)))?;
        if response.status() == StatusCode::OK {
            Ok(())
        } else {
            Err(ClientError::InvalidRequest { status_code: response.status(), message: Some(core::str::from_utf8(&response.bytes().await.unwrap()).unwrap().to_owned()) })
        }
    }

    // /// Same as [`create_invoice`](Client::create_invoice), but takes a path to an invoice file
    // /// instead. This will load the invoice file directly into the request, skipping serialization
    // #[instrument(level = "trace", skip(self, file_path), fields(path = %file_path.as_ref().display()))]
    // pub async fn create_invoice_from_file<P: AsRef<Path>>(
    //     &self,
    //     file_path: P,
    // ) -> Result<crate::InvoiceCreateResponse> {
    //     // Create an owned version of the path to avoid worrying about lifetimes here for the stream
    //     let path = file_path.as_ref().to_owned();
    //     debug!("Loading invoice from file");
    //     let inv_stream = load::raw(path).await?;
    //     debug!("Successfully loaded invoice stream");
    //     let req = self
    //         .create_invoice_builder()
    //         .body(Body::wrap_stream(inv_stream));
    //     self.create_invoice_request(req).await
    // }

    // fn create_invoice_builder(&self) -> RequestBuilder {
    //     // We can unwrap here because any URL error would be programmers fault
    //     self.client
    //         .post(self.base_url.join(INVOICE_ENDPOINT).unwrap())
    //         .header(header::CONTENT_TYPE, TOML_MIME_TYPE)
    // }

    // async fn create_invoice_request(
    //     &self,
    //     req: RequestBuilder,
    // ) -> Result<crate::InvoiceCreateResponse> {
    //     trace!(?req);
    //     let resp = req.send().await?;
    //     let resp = unwrap_status(resp, Endpoint::Invoice).await?;
    //     Ok(toml::from_slice(&resp.bytes().await?)?)
    // }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn can_log_in() -> Result<()> {
        let client = Client::new_from_login("https://localhost:5001/", "admin", "Passw0rd!").await?;
        client.register_revision_by_storage_id("hippos.rocks/helloworld", "1.1.3").await?;
        Ok(())
    }
}
