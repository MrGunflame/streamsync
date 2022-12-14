use reqwest::header::{HeaderValue, AUTHORIZATION};
use serde::{Deserialize, Serialize};

use crate::http::Request;
use crate::{Client, Result};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Stream {
    pub id: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Session {
    pub resource_id: String,
    pub session_id: String,
}

impl Session {
    pub async fn create(client: &Client, stream: String) -> Result<Self> {
        let mut req = Request::post(format!("/v1/streams/{}/sessions", stream));

        if let Some(token) = &*client.auth.read().unwrap() {
            req = req.header(
                AUTHORIZATION,
                HeaderValue::from_maybe_shared(format!("Bearer {}", token)).unwrap(),
            );
        }

        let resp = client.send(req).await?;
        Ok(resp.json().await?)
    }
}
