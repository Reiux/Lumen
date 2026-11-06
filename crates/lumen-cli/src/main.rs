use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    lumen_cli::main_entry().await
}
