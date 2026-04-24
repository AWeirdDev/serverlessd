use std::cell::OnceCell;

use reqwest::Client;

use super::Block;

/// An HTTP client extension.
#[repr(transparent)]
#[derive(Default)]
pub struct HttpClientBlock {
    client: OnceCell<Client>,
}

impl HttpClientBlock {
    #[inline(always)]
    pub const fn new() -> Self {
        Self {
            client: OnceCell::new(),
        }
    }

    /// Adds an HTTP client to the state, ignoring if already exists.
    pub fn add_client(&self) {
        self.client
            .set(
                Client::builder()
                    .tls_backend_rustls()
                    .default_headers({
                        use reqwest::header::{HeaderMap, HeaderValue};

                        let mut headers = HeaderMap::new();
                        headers.append("User-Agent", HeaderValue::from_static("Serverless"));

                        headers
                    })
                    .build()
                    .expect("reqwest: cannot initialize tls backend or resolver error"),
            )
            .ok();
    }

    /// Gets the HTTP client.
    #[inline(always)]
    pub fn get_client(&self) -> Option<&Client> {
        self.client.get()
    }
}

impl Block for HttpClientBlock {}
