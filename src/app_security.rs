use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use salvo::http::StatusCode;
use salvo::{Depot, FlowCtrl, Handler, Request, Response};

pub(super) struct AuthMiddleware {
    key: DecodingKey,
}

impl AuthMiddleware {
    #[inline(always)]
    pub(super) fn new(secret: String) -> Self {
        Self {
            key: DecodingKey::from_secret(secret.as_bytes()),
        }
    }
}

#[async_trait::async_trait]
impl Handler for AuthMiddleware {
    async fn handle(
        &self,
        req: &mut Request,
        depot: &mut Depot,
        res: &mut Response,
        ctrl: &mut FlowCtrl,
    ) {
        let token = match req.header::<String>("Authorization") {
            Some(t) => t,
            None => {
                res.status_code(StatusCode::UNAUTHORIZED);
                ctrl.skip_rest(); // stop further handlers
                return;
            }
        };

        // strip "Bearer " prefix if present
        let token = token.trim_start_matches("Bearer ");

        match decode::<Claims>(token, &self.key, &Validation::new(Algorithm::HS256)) {
            Ok(token_data) => {
                let claims = token_data.claims;
                if claims.hash != hex::encode(Sha256::digest(req.payload().await.unwrap().clone()))
                {
                    res.status_code(StatusCode::BAD_REQUEST);
                    return;
                }

                depot.insert::<&str, Claims>("claims", claims);

                ctrl.call_next(req, depot, res).await;
            }
            Err(_) => {
                res.status_code(StatusCode::UNAUTHORIZED);
                ctrl.skip_rest();
            }
        }
    }
}

/// payload = {
///     "sub": "subdomain.worker.dev",
///     "for": "webhook",
///     "hash": body sha256 hash,
///     "iat": datetime.now(timezone.utc),
///     "exp": datetime.now(timezone.utc) + timedelta(seconds=30),
///     "jti": uuid.uuid4()
/// }
#[derive(Serialize, Deserialize)]
pub(super) struct Claims {
    sub: String,
    #[serde(rename = "for")]
    for_: String,
    hash: String,
    iat: u64,
    exp: u64,
    jti: String,
}

// pub(super) fn make_jwt(
//     subdomain: &str,
//     body: &[u8],
//     secret: &[u8],
// ) -> Result<String, jsonwebtoken::errors::Error> {
//     let now = std::time::SystemTime::now()
//         .duration_since(std::time::UNIX_EPOCH)
//         .unwrap()
//         .as_secs();

//     let hash = hex::encode(Sha256::digest(body));

//     let claims = Claims {
//         sub: subdomain.to_string(),
//         for_: "webhook".to_string(),
//         hash,
//         iat: now,
//         exp: now + 30,
//         jti: Uuid::new_v4().to_string(),
//     };

//     encode(
//         &Header::default(),
//         &claims,
//         &EncodingKey::from_secret(secret),
//     )
// }
