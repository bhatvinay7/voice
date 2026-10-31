use crate::models::PropertyListing;
use super::{DatabaseStore, DbResult, postgres::error};

impl DatabaseStore {
    pub async fn add_listing(&self, listing: PropertyListing) -> DbResult<()> { self.save(&listing).await }
    pub async fn update_listing(&self, listing_id: &str, new_listing: PropertyListing) -> DbResult<bool> {
        if listing_id != new_listing.id { return Err("Listing ID cannot be changed".into()); }
        let json = serde_json::to_string(&new_listing).map_err(|_| "Invalid listing")?;
        Ok(sqlx::query("UPDATE public.property_listings SET (title, secondary_title, description, secondary_description, city, location, price_aed, listing_type, bedrooms, bathrooms, area_sqft, pet_friendly, near_metro, agent_id, available_slots) = (SELECT title, secondary_title, description, secondary_description, city, location, price_aed, listing_type, bedrooms, bathrooms, area_sqft, pet_friendly, near_metro, agent_id, available_slots FROM json_populate_record(NULL::public.property_listings, $2::json)) WHERE id = $1")
            .bind(listing_id).bind(json).execute(&self.pool).await.map_err(error)?.rows_affected() > 0)
    }
    pub async fn delete_listing(&self, listing_id: &str) -> DbResult<bool> { self.delete::<PropertyListing>(listing_id).await }
    pub async fn get_listings(&self) -> DbResult<Vec<PropertyListing>> { self.all().await }
}
