use crate::models::{OutboxNotification, TenancyContract};
use chrono::Utc;
use uuid::Uuid;
use super::{this_lock, DatabaseStore};

impl DatabaseStore {
    pub fn upload_tenancy_contract(
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
    ) -> TenancyContract {
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

        if let Ok(mut guard) = self.contracts.lock() {
            guard.push(contract.clone());
        }

        contract
    }

    pub fn get_contract_review_queue(&self) -> Vec<TenancyContract> {
        let guard = match this_lock(&self.contracts) {
            Ok(g) => g,
            Err(_) => return Vec::new(),
        };
        guard
            .iter()
            .filter(|c| c.needs_human_review && c.status == "pending_review")
            .cloned()
            .collect()
    }

    pub fn approve_contract_review(&self, contract_id: &str) -> Option<TenancyContract> {
        let mut guard = match this_lock(&self.contracts) {
            Ok(g) => g,
            Err(_) => return None,
        };

        for contract in guard.iter_mut() {
            if contract.id == contract_id {
                contract.status = "approved".to_string();
                contract.needs_human_review = false;
                return Some(contract.clone());
            }
        }

        None
    }

    pub fn process_outbox_notifications(&self) -> usize {
        let mut guard = match this_lock(&self.outbox) {
            Ok(g) => g,
            Err(_) => return 0,
        };

        let mut processed_count = 0;
        let now = Utc::now().to_rfc3339();

        for notif in guard.iter_mut() {
            if notif.status == "pending" {
                notif.status = "sent".to_string();
                notif.processed_at = Some(now.clone());
                processed_count += 1;
            }
        }

        processed_count
    }

    pub fn get_outbox_notifications(&self) -> Vec<OutboxNotification> {
        let guard = match this_lock(&self.outbox) {
            Ok(g) => g,
            Err(_) => return Vec::new(),
        };
        guard.clone()
    }
}
