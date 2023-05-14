use std::{str::FromStr, time::Duration};

use clap::Parser;
use exc::{
    core::types::{SubscribeTickers, SubscribeTrades, Ticker, Trade},
    types::instrument::InstrumentMeta,
    util::instrument::PollInstrumentsLayer,
    ExcExt, ExcLayer, IntoExc, SubscribeInstrumentsService, SubscribeTickersService,
    SubscribeTradesService,
};
use futures::{Stream, StreamExt, TryStreamExt};
use rust_decimal::Decimal;
use tower::{
    layer::{layer_fn, util::Stack},
    ServiceExt,
};

struct Exchange {
    ticker: Box<dyn SubscribeTickersService>,
    trade: Box<dyn SubscribeTradesService>,
    inst: Box<dyn SubscribeInstrumentsService>,
}

impl Exchange {
    fn binance() -> Self {
        let binance = exc::Binance::usd_margin_futures().connect_exc();
        Self {
            ticker: Box::new(binance.clone().into_subscribe_tickers()),
            // We cannot clone `Binance` here because the user may subscribe to both ticker and trade
            // channels which will undelryingly subscribe trade channel twice.
            trade: Box::new(exc::Binance::usd_margin_futures().connect_exc()),
            inst: Box::new(binance.layer(&Stack::new(
                ExcLayer::default(),
                PollInstrumentsLayer::new(Duration::from_secs(3600)),
            ))),
        }
    }

    fn okx() -> Self {
        let okx = exc::Okx::endpoint().connect_exc();
        // We cannot clone `Okx` here because the user may subscribe to both ticker and trade
        // channels which will undelryingly subscribe to ticker channel twice.
        let trade_svc = exc::Okx::endpoint()
            .connect_exc()
            .layer(&layer_fn(|svc: exc::Okx| {
                svc.into_exc()
                    // We use `adapt` method to convert the request type to `SubscribeTickers`.
                    .adapt::<SubscribeTickers>()
                    .map_request(|req: SubscribeTrades| SubscribeTickers {
                        instrument: req.instrument,
                    })
                    .map_response(|res| {
                        res.map_ok(|t| Trade {
                            ts: t.ts,
                            price: t.last,
                            size: t.size,
                            // Just as an example, it is not recommended.
                            buy: t.buy.unwrap_or(true),
                        })
                        .boxed()
                    })
            }));
        Self {
            ticker: Box::new(okx.clone()),
            trade: Box::new(trade_svc),
            inst: Box::new(okx.layer(&Stack::new(
                ExcLayer::default(),
                PollInstrumentsLayer::new(Duration::from_secs(3600)),
            ))),
        }
    }

    async fn subscribe_tickers(
        &mut self,
        inst: impl AsRef<str>,
    ) -> anyhow::Result<impl Stream<Item = exc::Result<Ticker>>> {
        Ok(self.ticker.subscribe_tickers(inst.as_ref()).await?)
    }

    async fn subscribe_trades(
        &mut self,
        inst: impl AsRef<str>,
    ) -> anyhow::Result<impl Stream<Item = exc::Result<Trade>>> {
        Ok(self.trade.subscribe_trades(inst.as_ref()).await?)
    }

    async fn subscribe_insts(
        &mut self,
        tag: impl AsRef<str>,
    ) -> anyhow::Result<impl Stream<Item = exc::Result<InstrumentMeta<Decimal>>>> {
        Ok(self.inst.subscribe_instruments(tag.as_ref()).await?)
    }
}

impl FromStr for Exchange {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "binance" => Ok(Self::binance()),
            "okx" => Ok(Self::okx()),
            s => anyhow::bail!("unsupported exchange: {s}"),
        }
    }
}

#[derive(Parser)]
struct Cli {
    #[arg(long)]
    exchange: String,
    #[arg(long)]
    inst: String,
    #[arg(long, default_value = "")]
    tag: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(tracing_subscriber::EnvFilter::new(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "info".into()),
        ))
        .init();
    let cli = Cli::try_parse()?;
    let mut exchange: Exchange = cli.exchange.parse()?;
    let mut stream = exchange.subscribe_tickers(cli.inst.clone()).await?;
    let ticker_handle = tokio::spawn(async move {
        while let Some(ticker) = stream.try_next().await? {
            println!("ticker: {ticker}");
        }
        anyhow::Result::<_>::Ok(())
    });
    let mut stream = exchange.subscribe_trades(cli.inst.clone()).await?;
    let trade_handle = tokio::spawn(async move {
        while let Some(trade) = stream.try_next().await? {
            println!("trade: {trade}");
        }
        anyhow::Result::<_>::Ok(())
    });
    let mut stream = exchange.subscribe_insts(cli.tag.clone()).await?;
    let inst_handle = tokio::spawn(async move {
        while let Some(inst) = stream.try_next().await? {
            println!("inst: {inst}");
        }
        anyhow::Result::<_>::Ok(())
    });
    ticker_handle.await??;
    trade_handle.await??;
    inst_handle.await??;
    Ok(())
}
