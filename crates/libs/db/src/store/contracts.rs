use crate::models::{OutboxNotification, TenancyContract};
use chrono::Utc;
use uuid::Uuid;
use super::{DatabaseStore, DbResult, postgres::error};

impl DatabaseStore {
    pub async fn upload_tenancy_contract(
        &self,
        agent_id: &str,
        filename: &str,
        landlord_name: &str,
        tenant_name: &str,
        property_address: &str,
        annual_rent_aed: u64,
        security_deposit_aed: u64,
        cheques_count: u8,
        notice_period_days: u16,
        confidence_score: f32,
    ) -> DbResult<TenancyContract> {
        let contract_id = format!("contract-{}", Uuid::new_v4());
        let r2_path = self.generate_r2_vault_path(agent_id, "contracts", filename);
        let needs_human_review = confidence_score < 0.85;

        let clause_summary = format!(
            "Standard UAE Tenancy Agreement. Rent: {} AED/yr payable in {} cheques. Security deposit: {} AED. Notice period: {} days.",
            annual_rent_aed, cheques_count, security_deposit_aed, notice_period_days
        );

        let contract = TenancyContract {
            id: contract_id,
            filename: filename.to_string(),
            r2_vault_path: r2_path,
            landlord_name: landlord_name.to_string(),
            tenant_name: tenant_name.to_string(),
            property_address: property_address.to_string(),
            annual_rent_aed,
            security_deposit_aed,
            cheques_count,
            notice_period_days,
            confidence_score,
            needs_human_review,
            status: if needs_human_review {
                "pending_review".to_string()
            } else {
                "approved".to_string()
            },
            clause_summary,
            created_at: Utc::now().to_rfc3339(),
        };

        self.save(&contract).await?;
        Ok(contract)
    }

    pub async fn get_contract_review_queue(&self) -> DbResult<Vec<TenancyContract>> {
        self.fetch("SELECT row_to_json(t)::text FROM public.tenancy_contracts t WHERE needs_human_review AND status = 'pending_review' ORDER BY created_at", &[]).await
    }
    pub async fn approve_contract_review(&self, contract_id: &str) -> DbResult<Option<TenancyContract>> {
        Ok(self.fetch("UPDATE public.tenancy_contracts t SET status = 'approved', needs_human_review = false WHERE id = $1 AND status = 'pending_review' RETURNING row_to_json(t)::text", &[contract_id]).await?.pop())
    }
    pub async fn process_outbox_notifications(&self) -> DbResult<usize> {
        Ok(sqlx::query("UPDATE public.outbox_notifications SET status = 'sent', processed_at = $1 WHERE status = 'pending'")
            .bind(Utc::now().to_rfc3339()).execute(&self.pool).await.map_err(error)?.rows_affected() as usize)
    }
    pub async fn get_outbox_notifications(&self) -> DbResult<Vec<OutboxNotification>> { self.all().await }
}
