use axum::{extract::State, response::IntoResponse, Json};
use serde::Deserialize;
use serde_json::json;
use crate::AppState;

#[derive(Deserialize)]
pub struct ContractUploadReq {
    pub agent_id: String,
    pub filename: String,
    pub landlord_name: String,
    pub tenant_name: String,
    pub property_address: String,
    pub annual_rent_aed: u64,
    pub security_deposit_aed: u64,
    pub cheques_count: u8,
    pub notice_period_days: u16,
    pub confidence_score: f32,
}

pub async fn upload_contract(
    State(state): State<AppState>,
    Json(payload): Json<ContractUploadReq>,
) -> impl IntoResponse {
    let contract = db_try!(state.db.upload_tenancy_contract(
        &payload.agent_id,
        &payload.filename,
        &payload.landlord_name,
        &payload.tenant_name,
        &payload.property_address,
        payload.annual_rent_aed,
        payload.security_deposit_aed,
        payload.cheques_count,
        payload.notice_period_days,
        payload.confidence_score,
    ));
    Json(json!({ "success": true, "contract": contract })).into_response()
}

use crate::api::AdminUser;

pub async fn get_review_queue(
    _admin: AdminUser,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let queue = db_try!(state.db.get_contract_review_queue());
    Json(json!({ "success": true, "review_queue": queue })).into_response()
}

#[derive(Deserialize)]
pub struct ApproveContractReq {
    pub contract_id: String,
}

pub async fn approve_contract(
    _admin: AdminUser,
    State(state): State<AppState>,
    Json(payload): Json<ApproveContractReq>,
) -> impl IntoResponse {
    match db_try!(state.db.approve_contract_review(&payload.contract_id)) {
        Some(approved) => Json(json!({ "success": true, "contract": approved })).into_response(),
        None => {
            Json(json!({ "success": false, "error": "Contract not found or already approved" })).into_response()
        }
    }
}
