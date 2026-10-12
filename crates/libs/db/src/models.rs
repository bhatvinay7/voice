use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PropertyListing {
    pub id: String,
    pub title: String,
    pub secondary_title: String,
    pub description: String,
    pub secondary_description: String,
    pub city: String,
    pub location: String,
    pub price_aed: u64,
    pub listing_type: String, // "rent" | "buy"
    pub bedrooms: u8,
    pub bathrooms: u8,
    pub area_sqft: u32,
    pub pet_friendly: bool,
    pub near_metro: bool,
    pub agent_id: String,
    pub available_slots: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserProfile {
    pub id: String,
    pub name: String,
    pub email: String,
    pub phone: String,
    pub is_verified: bool,
    pub emirates_id_number: Option<String>,
    pub role: String, // "user" | "admin"
    pub auth_methods: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ViewingBooking {
    pub id: String,
    pub listing_id: String,
    pub user_id: String,
    pub slot_time: String,
    pub status: String,
    pub created_at: String,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BookingResponse {
    pub success: bool,
    pub booking: Option<ViewingBooking>,
    pub error_code: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamEventRecord {
    pub stream: String,
    pub session_id: String,
    pub trace_id: String,
    pub event_type: String,
    pub payload_json: String,
    pub timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TenancyContract {
    pub id: String,
    pub filename: String,
    pub r2_vault_path: String,
    pub landlord_name: String,
    pub tenant_name: String,
    pub property_address: String,
    pub annual_rent_aed: u64,
    pub security_deposit_aed: u64,
    pub cheques_count: u8,
    pub notice_period_days: u16,
    pub confidence_score: f32,
    pub needs_human_review: bool,
    pub status: String,
    pub clause_summary: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutboxNotification {
    pub id: String,
    pub booking_id: String,
    pub user_id: String,
    pub channel: String,
    pub recipient: String,
    pub payload_json: String,
    pub status: String,
    pub created_at: String,
    pub processed_at: Option<String>,
}

// Call Transcript & AI Summary Record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallSummaryRecord {
    pub call_id: String,
    pub user_id: String,
    pub duration_seconds: u32,
    pub text_presigned_url: String,
    pub audio_presigned_url: String,
    pub summary: String,
    pub qualified_budget_aed: u64,
    pub preferred_location: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallEndpointsRecord {
    pub session_id: String,
    pub audio_url: String,
    pub text_url: String,
    pub created_at: String,
}

// Graph Workflow Admin Node
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdminGraphNode {
    pub id: String,
    pub step_type: String,
    pub instruction: String,
    pub status: String,
    pub is_preset: bool,
}

/// RAG Knowledge Document for real estate procedures, regulations, Golden Visa rules, and closing fees.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeDocument {
    pub id: String,
    pub title: String,
    pub category: String, // e.g. "buying_procedure", "golden_visa", "fees_breakdown", "regulations", "offplan"
    pub content: String,
    pub keywords: Vec<String>,
    pub metadata: serde_json::Value,
}

