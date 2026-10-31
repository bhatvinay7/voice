use super::*;
use uuid::Uuid;

/// Opt-in: writes uniquely named fixtures to the configured database and cleans them up.
#[tokio::test]
#[ignore = "requires a migrated PostgreSQL database; creates and removes isolated fixtures"]
async fn neon_persistence_and_booking_concurrency() {
    let cfg = AppConfig::load().unwrap();
    let first = DatabaseStore::from_config(&cfg).unwrap();
    first.initialize().await.unwrap();
    let id = format!("persistence-test-{}", Uuid::new_v4());
    let mut listing = seed_data::get_default_listings().remove(0);
    listing.id = id.clone();
    listing.price_aed = u64::MAX;
    first.add_listing(listing.clone()).await.unwrap();
    first.upsert_user(&id, "Persistence test", &format!("{id}@example.invalid"), "user", "test").await.unwrap();
    first.verify_user_document(&id, "test-document").await.unwrap();
    first.add_knowledge_document(KnowledgeDocument { id: id.clone(), title: "Persistence test".into(), category: "test".into(), content: "test".into(), keywords: vec!["test".into()], metadata: serde_json::json!({"test": true}) }).await.unwrap();
    first.add_graph_node(AdminGraphNode { id: id.clone(), step_type: "test".into(), instruction: "before".into(), status: "active".into(), is_preset: false }).await.unwrap();
    // The queue has a separate Redis dependency; exercise the same SQL metadata write directly.
    first.save(&CallEndpointsRecord { session_id: id.clone(), audio_url: "r2://test/recording.wav".into(), text_url: "".into(), created_at: chrono::Utc::now().to_rfc3339() }).await.unwrap();
    first.save_call_summary(&id, &id, 12, "", "r2://test/recording.wav", "Test summary", 100, "Test").await.unwrap();
    first.append_conversation_to_redis(&id, &id, "USER_PROMPT", r#"{"role":"User","text":"Hello"}"#).await.unwrap();
    first.append_conversation_to_redis(&id, &id, "LLM_RESPONSE", r#"{"response_text":"Welcome"}"#).await.unwrap();
    assert_eq!(first.load_redis_conversation(&id).await.unwrap().as_deref(), Some("User: Hello\nAssistant: Welcome"));
    let mut redis = first.open_redis_poll_connection().await.unwrap();
    let ttl: i64 = redis::cmd("TTL").arg(&id).query_async(&mut redis).await.unwrap();
    assert_eq!(ttl, -1, "Conversation streams have no TTL before worker persistence");
    assert_eq!(first.persist_conversation_from_redis_on(&id, &mut redis).await.unwrap().as_deref(), Some("User: Hello\nAssistant: Welcome"));
    first.delete_redis_conversation_on(&id, &mut redis).await.unwrap();
    assert!(first.load_redis_conversation(&id).await.unwrap().is_none());
    assert_eq!(first.load_call_transcript(&id).await.unwrap(), "User: Hello\nAssistant: Welcome");
    first.pool.close().await;

    // A separate pool proves these are database records, not shared Rust memory.
    let second = DatabaseStore::from_config(&cfg).unwrap();
    let third = DatabaseStore::from_config(&cfg).unwrap();
    assert_eq!(second.get_user(&id).await.unwrap().unwrap().name, "Persistence test");
    assert_eq!(second.one::<PropertyListing>(&id).await.unwrap().unwrap().price_aed, u64::MAX);
    assert_eq!(second.get_call_endpoints(&id).await.unwrap().unwrap().audio_url, "r2://test/recording.wav");
    assert_eq!(second.load_call_summary(&id).await.unwrap().unwrap().summary, "Test summary");
    assert_eq!(second.load_call_transcript(&id).await.unwrap(), "User: Hello\nAssistant: Welcome");
    assert!(second.one::<KnowledgeDocument>(&id).await.unwrap().is_some());
    assert!(second.update_graph_node(&id, "after".into()).await.unwrap());
    assert_eq!(third.one::<AdminGraphNode>(&id).await.unwrap().unwrap().instruction, "after");
    listing.title = "Updated listing".into();
    assert!(second.update_listing(&id, listing).await.unwrap());
    assert_eq!(third.one::<PropertyListing>(&id).await.unwrap().unwrap().title, "Updated listing");
    let key_a = format!("{id}-a");
    let key_b = format!("{id}-b");
    let (a, b) = tokio::join!(second.book_viewing_slot(&id, &id, "test-slot", &key_a), third.book_viewing_slot(&id, &id, "test-slot", &key_b));
    let a = a.unwrap(); let b = b.unwrap();
    assert_ne!(a.success, b.success, "only one concurrent request may reserve a slot");
    let repeated = third.book_viewing_slot(&id, &id, "test-slot", &key_a).await.unwrap();
    assert_eq!(serde_json::to_value(a).unwrap(), serde_json::to_value(repeated).unwrap());
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM public.outbox_notifications WHERE user_id = $1").bind(&id).fetch_one(&second.pool).await.unwrap();
    assert_eq!(count, 2);
    let contract = second.upload_tenancy_contract(&id, "test.pdf", "Test", "Test", "Test", 1, 1, 1, 1, 0.5).await.unwrap();
    assert!(third.approve_contract_review(&contract.id).await.unwrap().is_some());
    assert!(third.approve_contract_review(&contract.id).await.unwrap().is_none());

    let mut tx = second.pool.begin().await.unwrap();
    for (table, column, value) in [
        ("outbox_notifications", "user_id", id.as_str()), ("viewing_bookings", "user_id", &id),
        ("booking_idempotency", "idempotency_key", &key_a), ("booking_idempotency", "idempotency_key", &key_b),
        ("stream_events", "trace_id", &id), ("call_endpoints", "session_id", &id),
        ("call_summaries", "call_id", &id), ("tenancy_contracts", "id", &contract.id),
        ("admin_graph_nodes", "id", &id), ("knowledge_documents", "id", &id),
        ("property_listings", "id", &id), ("user_profiles", "id", &id),
    ] {
        sqlx::query(&format!("DELETE FROM public.{table} WHERE {column} = $1")).bind(value).execute(&mut *tx).await.unwrap();
    }
    tx.commit().await.unwrap();
    second.pool.close().await;
    assert!(second.get_listings().await.is_err(), "database failure must not become an empty success");
    assert!(second.upsert_user(&id, "Test", "test@example.invalid", "user", "test").await.is_err());
    third.pool.close().await;
}
