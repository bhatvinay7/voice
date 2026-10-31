#[tokio::main]
async fn main() -> Result<(), String> {
    let db = db::DatabaseStore::new();
    db.initialize().await?;
    let (summaries, recordings, events) = db.import_legacy_redis().await?;
    println!("Imported {summaries} summaries, {recordings} recording records, and {events} events into Neon.");
    Ok(())
}
