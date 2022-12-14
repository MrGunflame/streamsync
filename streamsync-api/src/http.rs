use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use reqwest::Method;

#[derive(Clone, Debug)]

pub struct Request {
    method: Method,
    pub(super) url: String,
    headers: HeaderMap,
}

impl Request {
    pub fn new(method: Method, url: String) -> Self {
        Self {
            method,
            url,
            headers: HeaderMap::new(),
        }
    }

    pub fn post(url: String) -> Self {
        Self::new(Method::POST, url)
    }

    pub fn header(mut self, key: HeaderName, value: HeaderValue) -> Self {
        self.headers.insert(key, value);
        self
    }
}

impl From<Request> for reqwest::Request {
    fn from(this: Request) -> Self {
        let mut req = reqwest::Request::new(this.method, this.url.parse().unwrap());
        *req.headers_mut() = this.headers;
        req
    }
}
