use std::borrow::Cow;
use std::sync::RwLock;

use http::Request;
use reqwest::{Response, StatusCode};

mod http;
pub mod v1;

#[derive(Debug)]
pub struct Client {
    base_url: Cow<'static, str>,
    client: reqwest::Client,
    auth: RwLock<Option<String>>,
}

impl Client {
    pub fn new<T>(base_url: T) -> Self
    where
        T: Into<Cow<'static, str>>,
    {
        Self {
            base_url: base_url.into(),
            client: reqwest::Client::new(),
            auth: RwLock::new(None),
        }
    }

    pub fn authorize(&self, token: String) {
        *self.auth.write().unwrap() = Some(token);
    }

    pub(crate) async fn send(&self, mut req: Request) -> Result<Response> {
        req.url = format!("{}{}", self.base_url, req.url);

        let resp = self.client.execute(req.into()).await?;

        if !resp.status().is_success() {
            return Err(Error::BadStatus(resp.status()));
        }

        Ok(resp)
    }
}

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Http(#[from] reqwest::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error("bad status: {0}")]
    BadStatus(StatusCode),
}
