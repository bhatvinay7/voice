pub mod filters;
pub use filters::*;

use db::{DatabaseStore, KnowledgeDocument, PropertyListing};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RAGSearchResult {
    pub listing: PropertyListing,
    pub relevance_score: f64,
    pub matched_filters: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeSearchResult {
    pub document: KnowledgeDocument,
    pub relevance_score: f64,
    pub matched_keywords: Vec<String>,
}

/// Identifies questions that should be answered from property listing fields first.
pub fn asks_listing_price(query: &str) -> bool {
    let lower = query.to_lowercase();
    [
        "price",
        "pricing",
        "rent",
        "rental",
        "cost",
        "how much",
        "budget",
        "deposit",
        "installment",
        "payment",
        "cheque",
        "aed",
    ]
    .iter()
    .any(|term| lower.contains(term))
}

#[derive(Debug, Clone, Deserialize)]
struct UpstashQueryResult {
    pub id: String,
    pub score: Option<f64>,
}

pub struct RAGPipeline {
    db: DatabaseStore,
    upstash_client: reqwest::Client,
    upstash_url: String,
    upstash_token: String,
}

impl RAGPipeline {
    pub fn new(db: DatabaseStore) -> Self {
        let cfg = config::AppConfig::load().unwrap_or_else(|_| config::AppConfig::load_default());
        Self {
            db,
            upstash_client: reqwest::Client::new(),
            upstash_url: cfg.upstash_vector_rest_url,
            upstash_token: cfg.upstash_vector_rest_token,
        }
    }

    pub fn from_config(db: DatabaseStore, upstash_url: String, upstash_token: String) -> Self {
        Self {
            db,
            upstash_client: reqwest::Client::new(),
            upstash_url,
            upstash_token,
        }
    }

    /// Delegates natural language parsing to filters module.
    pub fn parse_query_to_filters(&self, query: &str) -> SearchQueryFilters {
        filters::parse_query_to_filters(&self.db, query)
    }

    /// Dynamic semantic & keyword search across database property listings.
    pub fn search(&self, query: &str) -> Vec<RAGSearchResult> {
        let filters = self.parse_query_to_filters(query);
        let listings = self.db.get_listings();
        let terms: Vec<String> = query
            .to_lowercase()
            .split(|c: char| !c.is_alphanumeric())
            .filter(|s| s.len() > 2)
            .map(|s| s.to_string())
            .collect();

        let mut results: Vec<RAGSearchResult> = listings
            .into_iter()
            .map(|listing| {
                let mut score = 1.0f64;
                let mut matched_filters = Vec::new();

                if let Some(ref l_type) = filters.listing_type {
                    if &listing.listing_type == l_type {
                        score += 3.0;
                        matched_filters.push(format!("type: {}", l_type));
                    } else {
                        score -= 1.5;
                    }
                }

                if let Some(beds) = filters.bedrooms {
                    if listing.bedrooms == beds {
                        score += 3.0;
                        matched_filters.push(format!("bedrooms: {}", beds));
                    } else if (listing.bedrooms as i16 - beds as i16).abs() == 1 {
                        score += 1.0;
                    } else {
                        score -= 1.0;
                    }
                }

                if let Some(max_price) = filters.max_price_aed {
                    if listing.price_aed <= max_price {
                        score += 3.0;
                        matched_filters.push(format!("price <= {} AED", max_price));
                    } else if listing.price_aed <= (max_price as f64 * 1.15) as u64 {
                        score += 0.5;
                    } else {
                        score -= 2.0;
                    }
                }

                if let Some(ref loc) = filters.location {
                    if listing
                        .location
                        .to_lowercase()
                        .contains(&loc.to_lowercase())
                    {
                        score += 4.0;
                        matched_filters.push(format!("location: {}", loc));
                    }
                } else if let Some(ref city) = filters.city {
                    if listing.city.to_lowercase() == city.to_lowercase() {
                        score += 1.5;
                        matched_filters.push(format!("city: {}", city));
                    }
                }

                if let Some(pet) = filters.pet_friendly {
                    if listing.pet_friendly == pet {
                        score += 2.0;
                        matched_filters.push("pet-friendly".to_string());
                    }
                }

                if let Some(metro) = filters.near_metro {
                    if listing.near_metro == metro {
                        score += 2.0;
                        matched_filters.push("near-metro".to_string());
                    }
                }

                let full_text = format!(
                    "{} {} {} {}",
                    listing.title,
                    listing.secondary_title,
                    listing.description,
                    listing.secondary_description
                )
                .to_lowercase();

                let mut keyword_matches = 0;
                for t in &terms {
                    if full_text.contains(t) {
                        keyword_matches += 1;
                    }
                }
                score += (keyword_matches as f64) * 0.75;

                RAGSearchResult {
                    listing,
                    relevance_score: score.max(0.0),
                    matched_filters,
                }
            })
            .collect();

        // Keep only listings with an actual filter or text match. The base score alone
        // should not make every listing look relevant to questions such as "what is the price?".
        results.retain(|result| result.relevance_score > 1.0);
        results.sort_by(|a, b| {
            b.relevance_score
                .partial_cmp(&a.relevance_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        results
    }

    /// Dynamically searches knowledge documents for UAE real estate purchase procedures,
    /// DLD laws, Golden Visa regulations, fee schedules, and conveyancing rules.
    pub fn search_knowledge(&self, query: &str) -> Vec<KnowledgeSearchResult> {
        let q_lower = query.to_lowercase();
        let query_tokens: Vec<&str> = q_lower
            .split(|c: char| !c.is_alphanumeric())
            .filter(|s| s.len() > 2)
            .collect();

        let docs = self.db.get_knowledge_documents();
        let mut matches: Vec<KnowledgeSearchResult> = docs
            .into_iter()
            .filter_map(|doc| {
                let mut score = 0.0f64;
                let mut matched_keywords = Vec::new();

                for kw in &doc.keywords {
                    let kw_lower = kw.to_lowercase();
                    if q_lower.contains(&kw_lower) {
                        score += 3.5;
                        matched_keywords.push(kw.clone());
                    }
                }

                let content_lower = doc.content.to_lowercase();
                let title_lower = doc.title.to_lowercase();
                for token in &query_tokens {
                    if title_lower.contains(token) {
                        score += 2.0;
                    }
                    if content_lower.contains(token) {
                        score += 0.8;
                    }
                }

                if score > 0.0 {
                    Some(KnowledgeSearchResult {
                        document: doc,
                        relevance_score: score,
                        matched_keywords,
                    })
                } else {
                    None
                }
            })
            .collect();

        matches.sort_by(|a, b| {
            b.relevance_score
                .partial_cmp(&a.relevance_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        matches
    }

    /// Asynchronous vector query against Upstash Vector REST API.
    pub async fn search_vector_db(
        &self,
        query: &str,
        top_k: usize,
    ) -> Result<Vec<RAGSearchResult>, reqwest::Error> {
        let endpoint = format!("{}/query-data", self.upstash_url);
        let payload = serde_json::json!({
            "data": query,
            "topK": top_k,
            "includeMetadata": true,
            "includeData": true
        });

        let resp = self
            .upstash_client
            .post(&endpoint)
            .header("Authorization", format!("Bearer {}", self.upstash_token))
            .json(&payload)
            .send()
            .await?;

        if !resp.status().is_success() {
            return Ok(self.search(query));
        }

        let vector_hits: Vec<UpstashQueryResult> = resp.json().await.unwrap_or_default();
        let listings = self.db.get_listings();

        let mut results = Vec::new();
        for hit in vector_hits {
            if let Some(listing) = listings.iter().find(|l| l.id == hit.id) {
                results.push(RAGSearchResult {
                    listing: listing.clone(),
                    relevance_score: hit.score.unwrap_or(1.0) * 10.0,
                    matched_filters: vec!["vector_semantic_match".to_string()],
                });
            }
        }

        if results.is_empty() {
            Ok(self.search(query))
        } else {
            Ok(results)
        }
    }

    /// Upserts a property listing into the Upstash vector database dynamically.
    pub async fn upsert_property_to_vector(
        &self,
        listing: &PropertyListing,
    ) -> Result<(), reqwest::Error> {
        let full_text = format!(
            "Property: {} - {}\nLocation: {} - {}\nBedrooms: {}\nPrice: {} AED\nDetails: {}\nSpecifications: {}",
            listing.title,
            listing.secondary_title,
            listing.location,
            listing.city,
            listing.bedrooms,
            listing.price_aed,
            listing.description,
            listing.secondary_description
        );

        let endpoint = format!("{}/upsert-data", self.upstash_url);
        let payload = serde_json::json!({
            "id": listing.id,
            "data": full_text,
            "metadata": {
                "type": "property_listing",
                "listing_type": listing.listing_type,
                "city": listing.city,
                "location": listing.location,
                "price_aed": listing.price_aed,
                "bedrooms": listing.bedrooms,
                "pet_friendly": listing.pet_friendly,
                "near_metro": listing.near_metro
            }
        });

        self.upstash_client
            .post(&endpoint)
            .header("Authorization", format!("Bearer {}", self.upstash_token))
            .json(&payload)
            .send()
            .await?;

        Ok(())
    }

    /// Upserts a knowledge document into Upstash vector database dynamically.
    pub async fn upsert_document_to_vector(
        &self,
        doc: &KnowledgeDocument,
    ) -> Result<(), reqwest::Error> {
        let full_text = format!(
            "Knowledge Document: {}\nCategory: {}\nContent: {}\nKeywords: {}",
            doc.title,
            doc.category,
            doc.content,
            doc.keywords.join(", ")
        );

        let endpoint = format!("{}/upsert-data", self.upstash_url);
        let payload = serde_json::json!({
            "id": doc.id,
            "data": full_text,
            "metadata": {
                "type": "knowledge_document",
                "category": doc.category,
                "title": doc.title
            }
        });

        self.upstash_client
            .post(&endpoint)
            .header("Authorization", format!("Bearer {}", self.upstash_token))
            .json(&payload)
            .send()
            .await?;

        Ok(())
    }
}
