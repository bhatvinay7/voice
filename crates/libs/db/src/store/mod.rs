pub mod listings;
pub mod users;
pub mod bookings;
pub mod contracts;
pub mod calls;
pub mod seed_data;
pub mod seed_knowledge;

use crate::models::*;
use config::AppConfig;
use redis::Client;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct DatabaseStore {
    pub(crate) listings: Arc<Mutex<Vec<PropertyListing>>>,
    pub(crate) users: Arc<Mutex<HashMap<String, UserProfile>>>,
    pub(crate) bookings: Arc<Mutex<Vec<ViewingBooking>>>,
    pub(crate) idempotency_store: Arc<Mutex<HashMap<String, BookingResponse>>>,
    pub stream_events: Arc<Mutex<Vec<StreamEventRecord>>>,
    pub(crate) contracts: Arc<Mutex<Vec<TenancyContract>>>,
    pub(crate) outbox: Arc<Mutex<Vec<OutboxNotification>>>,
    pub(crate) call_summaries: Arc<Mutex<Vec<CallSummaryRecord>>>,
    pub(crate) call_endpoints: Arc<Mutex<Vec<CallEndpointsRecord>>>,
    pub(crate) graph_nodes: Arc<Mutex<Vec<AdminGraphNode>>>,
    pub(crate) knowledge_docs: Arc<Mutex<Vec<KnowledgeDocument>>>,
    pub(crate) redis_client: Option<Client>,
    pub bucket_name: String,
}

impl DatabaseStore {
    pub fn new() -> Self {
        let listings = seed_data::get_default_listings();

        let mut users = HashMap::new();
        users.insert(
            "user-verified".to_string(),
            UserProfile {
                id: "user-verified".to_string(),
                name: "Tariq Al-Mansoor".to_string(),
                email: "tariq@example.ae".to_string(),
                phone: "+971501234567".to_string(),
                is_verified: true,
                emirates_id_number: Some("784-1990-1234567-1".to_string()),
                role: "user".to_string(),
                auth_methods: vec!["email".to_string()],
            },
        );
        users.insert(
            "user-admin".to_string(),
            UserProfile {
                id: "user-admin".to_string(),
                name: "System Administrator".to_string(),
                email: "admin@estate.ae".to_string(),
                phone: "+971500000999".to_string(),
                is_verified: true,
                emirates_id_number: Some("784-1990-9999999-9".to_string()),
                role: "admin".to_string(),
                auth_methods: vec!["email".to_string()],
            },
        );
        users.insert(
            "user-unverified".to_string(),
            UserProfile {
                id: "user-unverified".to_string(),
                name: "John Doe".to_string(),
                email: "john@example.com".to_string(),
                phone: "+971509876543".to_string(),
                is_verified: false,
                emirates_id_number: None,
                role: "user".to_string(),
                auth_methods: vec!["email".to_string()],
            },
        );

        let initial_graph = seed_data::get_default_graph_nodes();
        let knowledge_docs = seed_knowledge::get_default_knowledge_documents();

        let cfg = AppConfig::load().unwrap_or_else(|_| AppConfig::load_default());
        let redis_client = Client::open(cfg.redis_url).ok();

        Self {
            listings: Arc::new(Mutex::new(listings)),
            users: Arc::new(Mutex::new(users)),
            bookings: Arc::new(Mutex::new(Vec::new())),
            idempotency_store: Arc::new(Mutex::new(HashMap::new())),
            stream_events: Arc::new(Mutex::new(Vec::new())),
            contracts: Arc::new(Mutex::new(Vec::new())),
            outbox: Arc::new(Mutex::new(Vec::new())),
            call_summaries: Arc::new(Mutex::new(Vec::new())),
            call_endpoints: Arc::new(Mutex::new(Vec::new())),
            graph_nodes: Arc::new(Mutex::new(initial_graph)),
            knowledge_docs: Arc::new(Mutex::new(knowledge_docs)),
            redis_client,
            bucket_name: cfg.r2_bucket_name,
        }
    }

    pub fn get_knowledge_documents(&self) -> Vec<KnowledgeDocument> {
        this_lock(&self.knowledge_docs)
            .map(|guard| guard.clone())
            .unwrap_or_default()
    }

    pub fn add_knowledge_document(&self, doc: KnowledgeDocument) {
        if let Ok(mut guard) = this_lock(&self.knowledge_docs) {
            guard.retain(|d| d.id != doc.id);
            guard.push(doc);
        }
    }
}

pub(crate) fn this_lock<T>(mutex: &Mutex<T>) -> Result<std::sync::MutexGuard<'_, T>, String> {
    mutex.lock().map_err(|e| format!("Mutex poisoned: {}", e))
}
