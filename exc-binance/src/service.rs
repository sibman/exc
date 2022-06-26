use std::task::{Context, Poll};

use exc::transport::http::channel::HttpsChannel;
use futures::{future::BoxFuture, FutureExt, TryFutureExt};
use tower::{
    ready_cache::{error::Failed, ReadyCache},
    util::Either,
    Service,
};

use crate::{
    endpoint::Endpoint,
    http::{
        request::{Payload, RestRequest},
        BinanceRestApi,
    },
    types::{request::Request, response::Response},
    websocket::{request::WsRequest, BinanceWebsocketApi},
    Error,
};

type Http = BinanceRestApi<HttpsChannel>;
type Ws = BinanceWebsocketApi;

pub(crate) const HTTP_KEY: &str = "http";
pub(crate) const WS_KEY: &str = "ws";

impl Service<Request> for Http {
    type Response = Response;
    type Error = Error;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Service::<RestRequest<Payload>>::poll_ready(self, cx).map_err(Error::from)
    }

    fn call(&mut self, req: Request) -> Self::Future {
        if let Request::Http(req) = req {
            Service::call(self, req)
                .map_ok(Response::Http)
                .map_err(Error::from)
                .boxed()
        } else {
            futures::future::ready(Err(Error::WrongResponseType)).boxed()
        }
    }
}

impl Service<Request> for Ws {
    type Response = Response;
    type Error = Error;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Service::<WsRequest>::poll_ready(self, cx).map_err(Error::from)
    }

    fn call(&mut self, req: Request) -> Self::Future {
        if let Request::Ws(req) = req {
            Service::call(self, req)
                .map_ok(Response::Ws)
                .map_err(Error::from)
                .boxed()
        } else {
            futures::future::ready(Err(Error::WrongResponseType)).boxed()
        }
    }
}

/// Binance.
pub struct Binance {
    pub(crate) svcs: ReadyCache<&'static str, Either<Http, Ws>, Request>,
}

impl Binance {
    /// Usd-margin futures endpoint.
    pub fn usd_margin_futures() -> Endpoint {
        Endpoint::usd_margin_futures()
    }
}

impl Service<Request> for Binance {
    type Response = Response;
    type Error = Error;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.svcs
            .poll_pending(cx)
            .map_err(|Failed(k, s)| Error::Unknown(anyhow::anyhow!("poll {k} ready error: {s}")))
    }

    fn call(&mut self, req: Request) -> Self::Future {
        match &req {
            Request::Http(_) => self
                .svcs
                .call_ready(&HTTP_KEY, req)
                .map_err(|err| match err.downcast::<Error>() {
                    Ok(err) => *err,
                    Err(err) => Error::Unknown(anyhow::anyhow!("{}", err)),
                })
                .boxed(),
            Request::Ws(_) => self
                .svcs
                .call_ready(&WS_KEY, req)
                .map_err(|err| match err.downcast::<Error>() {
                    Ok(err) => *err,
                    Err(err) => Error::Unknown(anyhow::anyhow!("{}", err)),
                })
                .boxed(),
        }
    }
}
