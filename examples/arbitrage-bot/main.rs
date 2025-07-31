pub mod cli;
pub mod context;
pub mod stream;

use tycho_atomic_arbitrage::errors::Result;

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("arbitrage_bot=info".parse().unwrap())
        )
        .pretty()
        .compact()
        .with_file(false)
        .with_line_number(false)
        .with_target(false)
        .init();

    let args = cli::parse_cli_args()?;
    let mut stream = stream::TychoStream::new(&args).await?;
    let mut ctx = context::Context::new(args)?;

    tracing::info!("Starting atomic arbitrage bot");

    loop {
        match stream.next().await {
            Some(block_update) => {
                match ctx.apply(block_update).await {
                    Ok(updated_pools) => {
                        if let Err(e) = ctx.search(updated_pools).await {
                            tracing::error!(error = %e, "Search operation failed");
                        }
                    }
                    Err(e) => {
                        tracing::error!(error = %e, "Failed to apply block update");
                    }
                }
            }
            None => {
                tracing::debug!("No block update received, continuing");
                continue;
            }
        }
    }
}
