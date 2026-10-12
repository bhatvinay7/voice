use crate::models::KnowledgeDocument;
use serde_json::json;

/// Default RAG knowledge documents covering Dubai real estate purchase procedures,
/// DLD conveyancing, Golden Visa eligibility, fee structures, and property laws.
pub fn get_default_knowledge_documents() -> Vec<KnowledgeDocument> {
    vec![
        KnowledgeDocument {
            id: "doc-dubai-buy-procedure".to_string(),
            title: "Dubai Real Estate Purchase Procedure & DLD Conveyancing".to_string(),
            category: "buying_procedure".to_string(),
            content: "To purchase property in Dubai, follow the official 4-step Dubai Land Department (DLD) process: 1) Contract Signing: Buyer and seller enter into Unified Form F (MOU) outlining the agreed transaction price and closing date. A 10% security deposit cheque is held in escrow by the certified RERA broker or trustee. 2) Developer NOC: The seller applies for a No Objection Certificate (NOC) from the developer (e.g., Emaar, Nakheel, DAMAC) confirming that all service charges and community fees are fully settled. 3) Ownership Transfer: Buyer, seller, and brokers convene at an authorized DLD Registration Trustee office to settle remaining payments via bank manager's cheques, the 4% DLD transfer fee, AED 4,000 + VAT trustee registration fee, and 2% broker commission. 4) Title Deed Issuance: The Dubai Land Department immediately prints and registers the official digital Title Deed in the buyer's name.".to_string(),
            keywords: vec![
                "procedure".to_string(),
                "how to buy".to_string(),
                "buying process".to_string(),
                "steps to buy".to_string(),
                "form f".to_string(),
                "mou".to_string(),
                "dld".to_string(),
                "transfer".to_string(),
                "noc".to_string(),
                "title deed".to_string(),
            ],
            metadata: json!({
                "governing_body": "Dubai Land Department (DLD)",
                "legal_basis": "Law No. 7 of 2006",
                "document_type": "standard_operating_procedure"
            }),
        },
        KnowledgeDocument {
            id: "doc-uae-golden-visa".to_string(),
            title: "UAE 10-Year Real Estate Golden Visa Regulations".to_string(),
            category: "golden_visa".to_string(),
            content: "Foreign property buyers investing AED 2,000,000 or more in designated freehold real estate across Dubai or Abu Dhabi qualify for the UAE 10-Year Golden Visa. Key criteria include: 1) The purchase value or cumulative property portfolio value must equal or exceed AED 2,000,000 on the official Title Deed or DLD Oqood certificate. 2) Mortgaged properties are eligible provided an NOC from the financing bank confirms a minimum paid-up equity of AED 2,000,000. 3) Off-plan properties qualify if registered under DLD Oqood with AED 2M paid. 4) Holders receive 10-year renewable residency without the need for an employment sponsor, 100% foreign business ownership, and full family sponsorship rights for spouse, children, and domestic staff.".to_string(),
            keywords: vec![
                "golden visa".to_string(),
                "visa eligibility".to_string(),
                "10-year visa".to_string(),
                "investor visa".to_string(),
                "2 million".to_string(),
                "2m".to_string(),
                "residency".to_string(),
                "freehold visa".to_string(),
            ],
            metadata: json!({
                "governing_body": "Federal Authority for Identity, Citizenship, Customs and Port Security (ICP)",
                "minimum_investment_aed": 2000000,
                "validity_years": 10
            }),
        },
        KnowledgeDocument {
            id: "doc-dubai-fees-breakdown".to_string(),
            title: "Dubai Property Purchase Fees and Closing Costs Schedule".to_string(),
            category: "fees_breakdown".to_string(),
            content: "Closing costs for acquiring ready secondary or off-plan property in Dubai typically amount to approximately 6% to 7% of purchase price: 1) DLD Transfer Fee: 4% of total purchase price + AED 580 administrative knowledge and innovation fee. 2) Registration Trustee Fee: AED 4,000 (+ 5% VAT) for properties equal to or above AED 500,000; AED 2,000 (+ 5% VAT) for properties below AED 500,000. 3) Real Estate Broker Commission: 2% of purchase price (+ 5% VAT). 4) Developer NOC Fee: AED 500 to AED 5,000 depending on developer guidelines. 5) Mortgage Registration Fee (if financed): 0.25% of loan amount + AED 290 fee.".to_string(),
            keywords: vec![
                "fees".to_string(),
                "closing costs".to_string(),
                "dld fee".to_string(),
                "transfer fee".to_string(),
                "broker commission".to_string(),
                "trustee fee".to_string(),
                "noc fee".to_string(),
                "expenses".to_string(),
            ],
            metadata: json!({
                "dld_transfer_fee_pct": 4.0,
                "broker_commission_pct": 2.0,
                "trustee_fee_aed": 4000
            }),
        },
        KnowledgeDocument {
            id: "doc-dubai-freehold-zones".to_string(),
            title: "Designated Freehold Real Estate Enclaves in Dubai".to_string(),
            category: "regulations".to_string(),
            content: "Under Dubai Law No. 7 of 2006, foreign non-GCC citizens enjoy absolute freehold ownership (unrestricted title deed in perpetuity) in designated master-planned zones. Premier freehold districts include Palm Jumeirah, Downtown Dubai, Dubai Marina, Business Bay, Dubai Hills Estate, Emirates Hills, Arabian Ranches, Jumeirah Village Circle (JVC), Bluewaters Island, and Mohammed Bin Rashid (MBR) City. Properties outside designated freehold zones are available only on leasehold terms (up to 99 years).".to_string(),
            keywords: vec![
                "freehold".to_string(),
                "foreign ownership".to_string(),
                "expat buying".to_string(),
                "freehold zones".to_string(),
                "leasehold".to_string(),
                "ownership law".to_string(),
            ],
            metadata: json!({
                "law": "Dubai Law No. 7 of 2006",
                "ownership_type": "Freehold in Perpetuity"
            }),
        },
        KnowledgeDocument {
            id: "doc-dubai-offplan-escrow".to_string(),
            title: "Dubai Off-Plan Property Regulations and Escrow Account Protections".to_string(),
            category: "offplan".to_string(),
            content: "Off-plan property investments in Dubai are protected under Dubai Escrow Law (Law No. 8 of 2007). Developers must hold 100% land ownership and complete 20% construction or submit a 20% bank guarantee before marketing off-plan units. All buyer instalment payments must be deposited directly into a RERA-regulated project escrow account. Funds are released to the developer in tranches only upon independent engineering inspection and milestone verification. Off-plan contracts are officially registered via DLD Oqood before final handover.".to_string(),
            keywords: vec![
                "off plan".to_string(),
                "offplan".to_string(),
                "escrow account".to_string(),
                "law no 8".to_string(),
                "oqood".to_string(),
                "rera".to_string(),
                "construction milestone".to_string(),
            ],
            metadata: json!({
                "law": "Law No. 8 of 2007",
                "protection": "Project Escrow Guarantee"
            }),
        },
    ]
}
