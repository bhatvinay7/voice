use super::{DatabaseStore, DbResult};
use crate::models::*;
use serde::{Serialize, de::DeserializeOwned};

pub(super) fn error(error: sqlx::Error) -> String {
    // Do not expose connection details or SQL values in API errors.
    eprintln!("PostgreSQL operation failed (code: {:?})", error.as_database_error().and_then(|e| e.code()));
    "Database operation failed".to_string()
}

pub(super) trait Record: Serialize + DeserializeOwned {
    const TABLE: &'static str;
    const KEY: &'static str;
    const UPSERT: &'static str;
}

impl DatabaseStore {
    pub(super) async fn all<T: Record>(&self) -> DbResult<Vec<T>> {
        self.fetch(&format!("SELECT row_to_json(t)::text FROM public.{} t ORDER BY {}", T::TABLE, T::KEY), &[]).await
    }
    pub(super) async fn one<T: Record>(&self, id: &str) -> DbResult<Option<T>> {
        Ok(self.fetch(&format!("SELECT row_to_json(t)::text FROM public.{} t WHERE {} = $1", T::TABLE, T::KEY), &[id]).await?.pop())
    }
    pub(super) async fn fetch<T: DeserializeOwned>(&self, sql: &str, args: &[&str]) -> DbResult<Vec<T>> {
        let mut query = sqlx::query_scalar::<_, String>(sql);
        for arg in args { query = query.bind(*arg); }
        query.fetch_all(&self.pool).await.map_err(error)?.into_iter()
            .map(|text| serde_json::from_str(&text).map_err(|_| "Invalid database record".to_string())).collect()
    }
    pub(super) async fn save<T: Record>(&self, record: &T) -> DbResult<()> {
        let json = serde_json::to_string(record).map_err(|_| "Could not encode database record")?;
        sqlx::query(T::UPSERT).bind(json).execute(&self.pool).await.map_err(error)?;
        Ok(())
    }
    pub(super) async fn delete<T: Record>(&self, id: &str) -> DbResult<bool> {
        Ok(sqlx::query(&format!("DELETE FROM public.{} WHERE {} = $1", T::TABLE, T::KEY))
            .bind(id).execute(&self.pool).await.map_err(error)?.rows_affected() > 0)
    }
}

impl Record for PropertyListing {
 const TABLE: &'static str = "property_listings";
 const KEY: &'static str = "id";
 const UPSERT: &'static str = "INSERT INTO public.property_listings SELECT * FROM json_populate_record(NULL::public.property_listings, $1::json) ON CONFLICT (id) DO UPDATE SET title = EXCLUDED.title, secondary_title = EXCLUDED.secondary_title, description = EXCLUDED.description, secondary_description = EXCLUDED.secondary_description, city = EXCLUDED.city, location = EXCLUDED.location, price_aed = EXCLUDED.price_aed, listing_type = EXCLUDED.listing_type, bedrooms = EXCLUDED.bedrooms, bathrooms = EXCLUDED.bathrooms, area_sqft = EXCLUDED.area_sqft, pet_friendly = EXCLUDED.pet_friendly, near_metro = EXCLUDED.near_metro, agent_id = EXCLUDED.agent_id, available_slots = EXCLUDED.available_slots";
}

impl Record for UserProfile {
 const TABLE: &'static str = "user_profiles";
 const KEY: &'static str = "id";
 const UPSERT: &'static str = "INSERT INTO public.user_profiles SELECT * FROM json_populate_record(NULL::public.user_profiles, $1::json) ON CONFLICT (id) DO UPDATE SET name = EXCLUDED.name, email = EXCLUDED.email, phone = EXCLUDED.phone, is_verified = EXCLUDED.is_verified, emirates_id_number = EXCLUDED.emirates_id_number, role = EXCLUDED.role, auth_methods = EXCLUDED.auth_methods";
}

impl Record for TenancyContract {
 const TABLE: &'static str = "tenancy_contracts";
 const KEY: &'static str = "id";
 const UPSERT: &'static str = "INSERT INTO public.tenancy_contracts SELECT * FROM json_populate_record(NULL::public.tenancy_contracts, $1::json) ON CONFLICT (id) DO UPDATE SET filename = EXCLUDED.filename, r2_vault_path = EXCLUDED.r2_vault_path, landlord_name = EXCLUDED.landlord_name, tenant_name = EXCLUDED.tenant_name, property_address = EXCLUDED.property_address, annual_rent_aed = EXCLUDED.annual_rent_aed, security_deposit_aed = EXCLUDED.security_deposit_aed, cheques_count = EXCLUDED.cheques_count, notice_period_days = EXCLUDED.notice_period_days, confidence_score = EXCLUDED.confidence_score, needs_human_review = EXCLUDED.needs_human_review, status = EXCLUDED.status, clause_summary = EXCLUDED.clause_summary, created_at = EXCLUDED.created_at";
}

impl Record for OutboxNotification {
 const TABLE: &'static str = "outbox_notifications";
 const KEY: &'static str = "id";
 const UPSERT: &'static str = "INSERT INTO public.outbox_notifications SELECT * FROM json_populate_record(NULL::public.outbox_notifications, $1::json) ON CONFLICT (id) DO UPDATE SET booking_id = EXCLUDED.booking_id, user_id = EXCLUDED.user_id, channel = EXCLUDED.channel, recipient = EXCLUDED.recipient, payload_json = EXCLUDED.payload_json, status = EXCLUDED.status, created_at = EXCLUDED.created_at, processed_at = EXCLUDED.processed_at";
}

impl Record for CallSummaryRecord {
 const TABLE: &'static str = "call_summaries";
 const KEY: &'static str = "call_id";
 const UPSERT: &'static str = "INSERT INTO public.call_summaries SELECT * FROM json_populate_record(NULL::public.call_summaries, $1::json) ON CONFLICT (call_id) DO UPDATE SET user_id = EXCLUDED.user_id, duration_seconds = EXCLUDED.duration_seconds, text_presigned_url = EXCLUDED.text_presigned_url, audio_presigned_url = EXCLUDED.audio_presigned_url, summary = EXCLUDED.summary, qualified_budget_aed = EXCLUDED.qualified_budget_aed, preferred_location = EXCLUDED.preferred_location, created_at = EXCLUDED.created_at";
}

impl Record for CallEndpointsRecord {
 const TABLE: &'static str = "call_endpoints";
 const KEY: &'static str = "session_id";
 const UPSERT: &'static str = "INSERT INTO public.call_endpoints SELECT * FROM json_populate_record(NULL::public.call_endpoints, $1::json) ON CONFLICT (session_id) DO UPDATE SET audio_url = EXCLUDED.audio_url, text_url = EXCLUDED.text_url, created_at = EXCLUDED.created_at";
}

impl Record for AdminGraphNode {
 const TABLE: &'static str = "admin_graph_nodes";
 const KEY: &'static str = "id";
 const UPSERT: &'static str = "INSERT INTO public.admin_graph_nodes SELECT * FROM json_populate_record(NULL::public.admin_graph_nodes, $1::json) ON CONFLICT (id) DO UPDATE SET step_type = EXCLUDED.step_type, instruction = EXCLUDED.instruction, status = EXCLUDED.status, is_preset = EXCLUDED.is_preset";
}

impl Record for KnowledgeDocument {
 const TABLE: &'static str = "knowledge_documents";
 const KEY: &'static str = "id";
 const UPSERT: &'static str = "INSERT INTO public.knowledge_documents SELECT * FROM json_populate_record(NULL::public.knowledge_documents, $1::json) ON CONFLICT (id) DO UPDATE SET title = EXCLUDED.title, category = EXCLUDED.category, content = EXCLUDED.content, keywords = EXCLUDED.keywords, metadata = EXCLUDED.metadata";
}
