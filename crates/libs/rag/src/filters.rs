use db::DatabaseStore;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SearchQueryFilters {
    pub city: Option<String>,
    pub location: Option<String>,
    pub max_price_aed: Option<u64>,
    pub bedrooms: Option<u8>,
    pub listing_type: Option<String>, // "rent" | "buy"
    pub pet_friendly: Option<bool>,
    pub near_metro: Option<bool>,
}

/// Dynamically parses natural language queries into structured search filters.
pub fn parse_query_to_filters(db: &DatabaseStore, query: &str) -> SearchQueryFilters {
    let lower = query.to_lowercase();
    let mut filters = SearchQueryFilters::default();
    let cfg = config::AppConfig::load().unwrap_or_else(|_| config::AppConfig::load_default());

    // 1. Primary listing type from config synonyms
    if cfg
        .filter_type_a_synonyms
        .iter()
        .any(|syn| lower.contains(syn))
    {
        filters.listing_type = Some(cfg.filter_primary_type_a.clone());
    } else if cfg
        .filter_type_b_synonyms
        .iter()
        .any(|syn| lower.contains(syn))
    {
        filters.listing_type = Some(cfg.filter_primary_type_b.clone());
    }

    // 2. Units / Bedrooms (dynamically parsed)
    for i in (1..=8).rev() {
        if lower.contains(&format!("{}br", i))
            || lower.contains(&format!("{}-bedroom", i))
            || lower.contains(&format!("{} bedroom", i))
            || lower.contains(&format!("{} bed", i))
        {
            filters.bedrooms = Some(i as u8);
            break;
        }
    }
    if filters.bedrooms.is_none() {
        if lower.contains("studio") || lower.contains("one bed") {
            filters.bedrooms = Some(1);
        } else if lower.contains("two bed") {
            filters.bedrooms = Some(2);
        } else if lower.contains("three bed") {
            filters.bedrooms = Some(3);
        } else if lower.contains("four bed") {
            filters.bedrooms = Some(4);
        } else if lower.contains("five bed") {
            filters.bedrooms = Some(5);
        }
    }

    // 3. Location & City (dynamically matched from database entries)
    let listings = db.get_listings();
    for listing in &listings {
        if !listing.location.is_empty() && lower.contains(&listing.location.to_lowercase()) {
            filters.location = Some(listing.location.clone());
            filters.city = Some(listing.city.clone());
            break;
        }
    }
    if filters.city.is_none() {
        for listing in &listings {
            if !listing.city.is_empty() && lower.contains(&listing.city.to_lowercase()) {
                filters.city = Some(listing.city.clone());
                break;
            }
        }
    }

    // 4. Feature flags from config synonyms
    if cfg
        .filter_feature_1_synonyms
        .iter()
        .any(|syn| lower.contains(syn))
    {
        filters.pet_friendly = Some(true);
    }
    if cfg
        .filter_feature_2_synonyms
        .iter()
        .any(|syn| lower.contains(syn))
    {
        filters.near_metro = Some(true);
    }

    // 5. Dynamic budget / price extraction
    filters.max_price_aed = parse_dynamic_budget(&lower);

    filters
}

/// Helper function to parse dynamic monetary quantities from user natural language query.
pub fn parse_dynamic_budget(lower: &str) -> Option<u64> {
    let tokens: Vec<&str> = lower.split_whitespace().collect();

    for (idx, token) in tokens.iter().enumerate() {
        let clean: String = token
            .chars()
            .filter(|c| c.is_alphanumeric() || *c == '.')
            .collect();

        // Cases like "2m", "12.5m", "35m"
        if clean.ends_with('m') && clean.len() > 1 {
            if let Ok(num) = clean[..clean.len() - 1].parse::<f64>() {
                return Some((num * 1_000_000.0) as u64);
            }
        }

        // Cases like "120k", "85k", "250k"
        if clean.ends_with('k') && clean.len() > 1 {
            if let Ok(num) = clean[..clean.len() - 1].parse::<f64>() {
                return Some((num * 1_000.0) as u64);
            }
        }

        // Cases like "2 million" or "150 thousand"
        if idx + 1 < tokens.len() {
            let next = tokens[idx + 1].trim_matches(|c: char| !c.is_alphabetic());
            if next.starts_with("million") {
                if let Ok(num) = clean.parse::<f64>() {
                    return Some((num * 1_000_000.0) as u64);
                }
            } else if next.starts_with("thousand") {
                if let Ok(num) = clean.parse::<f64>() {
                    return Some((num * 1_000.0) as u64);
                }
            }
        }

        // Raw numbers >= 10,000 (e.g. 75000, 115000, 3500000)
        if let Ok(val) = clean.parse::<u64>() {
            if val >= 10000 {
                return Some(val);
            }
        }
    }

    None
}
