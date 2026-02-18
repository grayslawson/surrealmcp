pub mod cli;
pub mod cloud;
pub mod db;
pub mod engine;
pub mod logs;
pub mod prompts;
pub mod resources;
pub mod server;
pub mod tools;
pub mod utils;

use anyhow::Result;
use clap::Parser;
use crate::server::ServerConfig;

#[tokio::main]
async fn main() -> Result<()> {
    if rustls::crypto::ring::default_provider()
        .install_default()
        .is_err()
    {
        tracing::error!("Failed to install default crypto provider");
    }

    // Parse command line arguments
    let cli = cli::Cli::parse();
    // Run the specified command
    match cli.command {
        cli::Commands::Start {
            endpoint,
            ns,
            db,
            user,
            pass,
            server_url,
            bind_address,
            socket_path,
            auth_disabled,
            rate_limit_rps,
            rate_limit_burst,
            auth_server,
            auth_audience,
            cloud_access_token,
            cloud_refresh_token,
        } => {
            // Create the server config
            let config = ServerConfig {
                endpoint,
                ns,
                db,
                user,
                pass,
                server_url,
                bind_address,
                socket_path,
                auth_disabled,
                rate_limit_rps,
                rate_limit_burst,
                auth_server,
                auth_audience,
                cloud_access_token,
                cloud_refresh_token,
            };
            server::start_server(config).await
        }
    }
}
