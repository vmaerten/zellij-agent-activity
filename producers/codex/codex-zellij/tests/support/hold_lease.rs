//! Subprocess driver for the actual ownership implementation. Not an installed command.
use anyhow::{Context, Result};
use codex_zellij::ownership::Ownership;
use std::{
    io::{Read, Write},
    path::Path,
};
#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let root = args.get(1).context("coordination directory required")?;
    let thread = args.get(2).context("thread required")?;
    println!("acquiring");
    std::io::stdout().flush()?;
    let _ownership = Ownership::acquire(Path::new(root), thread).await?;
    println!("owned");
    std::io::stdout().flush()?;
    std::io::stdin().read_to_end(&mut Vec::new())?;
    Ok(())
}
