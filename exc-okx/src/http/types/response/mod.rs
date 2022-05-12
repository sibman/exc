use anyhow::anyhow;
use exc::{error::InstrumentError, ExchangeError};
use serde::Deserialize;

/// Candle.
pub mod candle;

pub use candle::Candle;

/// Okx HTTP API Response (with `code` and `msg`).
#[derive(Debug, Deserialize)]
pub struct FullHttpResponse {
    /// Code.
    pub code: String,
    /// Message.
    pub msg: String,
    /// Data.
    #[serde(default)]
    pub data: Vec<ResponseData>,
}

/// Okx HTTP API Response.
#[derive(Debug)]
pub struct HttpResponse {
    /// Data.
    pub data: Vec<ResponseData>,
}

impl TryFrom<FullHttpResponse> for HttpResponse {
    type Error = ExchangeError;

    fn try_from(full: FullHttpResponse) -> Result<Self, Self::Error> {
        let code = full.code;
        let msg = full.msg;
        match code.as_str() {
            "0" => Ok(Self { data: full.data }),
            "51001" => Err(ExchangeError::Instrument(InstrumentError::NotFound)),
            "50011" => Err(ExchangeError::RateLimited(anyhow!("{msg}"))),
            "50013" => Err(ExchangeError::Unavailable(anyhow!("{msg}"))),
            _ => Err(ExchangeError::Api(anyhow!("code={code} msg={msg}",))),
        }
    }
}

/// Response data types.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum ResponseData {
    /// Candle.
    Candle(Candle),
    /// Placeholder.
    Placeholder,
}
