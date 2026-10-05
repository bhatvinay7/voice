export interface TenancyContract {
  id: string;
  filename: string;
  r2_vault_path: string;
  landlord_name: string;
  tenant_name: string;
  property_address: string;
  annual_rent: number;
  security_deposit: number;
  cheques_count: number;
  notice_period_days: number;
  confidence_score: number;
  needs_human_review: boolean;
  status: "pending_review" | "approved" | "rejected";
  clause_summary: string;
  created_at: string;
}
